//! Binary entry point.
//!
//! Two modes:
//!
//! * **default** — runs the HTTP server plus the background worker.
//! * **`--migrate`** — applies all migrations from `./migrations` and
//!   exits. Used by `docker-compose.yml` and CI to prepare the schema
//!   before the app starts. Requires only `DATABASE_URL`.

use std::net::SocketAddr;

use axum::http::{header, HeaderValue};
use lexhack_backend::config::Config;
use lexhack_backend::state::AppState;
use lexhack_backend::worker;
use lexhack_backend::{api, db, telemetry};
use tokio_util::sync::CancellationToken;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Subcommand: `--migrate` applies migrations and exits.
    //
    // Deliberately handled before any other startup step so that a
    // migration run does not need a valid `NOTIFICATION_SECRET_KEY` or
    // any other runtime configuration. Only `DATABASE_URL` is required.
    if std::env::args().any(|a| a == "--migrate") {
        let _ = dotenvy::dotenv();
        let database_url = std::env::var("DATABASE_URL")
            .map_err(|_| anyhow::anyhow!("DATABASE_URL must be set to run --migrate"))?;
        telemetry::init();
        tracing::info!("running migrations");
        let pool = sqlx::PgPool::connect(&database_url).await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        tracing::info!("migrations applied");
        return Ok(());
    }

    // 1. Configuration.
    let config = match Config::load() {
        Ok(config) => config,
        Err(err) => {
            eprintln!("failed to load configuration: {err}");
            std::process::exit(1);
        }
    };

    // 2. Logging/tracing.
    telemetry::init();
    tracing::info!(?config, "configuration loaded");

    // 3. Database initialization.
    let db_pool = db::init_pool(&config).await.map_err(|err| {
        tracing::error!(error = %err, "database initialization failed, aborting startup");
        err
    })?;

    // 4. Application state.
    let socket_addr: SocketAddr = config.socket_addr().parse().map_err(|err| {
        anyhow::anyhow!(
            "invalid HOST/PORT combination `{}`: {err}",
            config.socket_addr()
        )
    })?;
    let state = AppState::new(config.clone(), db_pool.clone());

    // 5. Cancellation token shared between the HTTP server and the worker.
    let cancel = CancellationToken::new();

    // 6. Background worker.
    let worker_handle = if config.worker_enabled {
        Some(worker::spawn(
            config.clone(),
            db_pool.clone(),
            state.channels(),
            std::sync::Arc::new(state.secret_key().clone()),
            cancel.clone(),
        ))
    } else {
        tracing::info!("worker disabled by configuration");
        None
    };

    // 7. Router + middleware.
    let trace_layer = TraceLayer::new_for_http().make_span_with(
        |request: &axum::http::Request<axum::body::Body>| {
            tracing::info_span!(
                "http_request",
                method = %request.method(),
                path = %request.uri().path(),
                version = ?request.version(),
            )
        },
    );

    let xcto = SetResponseHeaderLayer::overriding(
        header::HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    );
    let referrer = SetResponseHeaderLayer::overriding(
        header::HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("no-referrer"),
    );
    let cache = SetResponseHeaderLayer::overriding(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store"),
    );

    let cors = build_cors_layer(&config)?;

    let mut app = api::build_router(state);

    // Optional: serve the built frontend from the same origin.
    //
    // Enabled by setting FRONTEND_DIST to the directory containing
    // `index.html` and the hashed WASM bundle. When unset (the default
    // for backend-only deployments and local development with
    // `trunk serve`), the backend does not serve static files.
    //
    // SPA routing: any request whose path does not start with `/api/`
    // and does not match a real file falls back to `index.html`, so
    // client-side routes survive a page refresh.
    if let Ok(dist) = std::env::var("FRONTEND_DIST") {
        let index = format!("{dist}/index.html");
        let serve_dir = ServeDir::new(&dist).not_found_service(ServeFile::new(index));
        app = app.fallback_service(axum::routing::any_service(serve_dir));
        tracing::info!(dist = %dist, "serving frontend static files");
    }

    let app = app
        .layer(cache)
        .layer(referrer)
        .layer(xcto)
        .layer(cors)
        .layer(trace_layer);

    // 8. HTTP server, with graceful shutdown.
    tracing::info!(%socket_addr, "starting HTTP server");
    let listener = tokio::net::TcpListener::bind(socket_addr).await?;

    let server_cancel = cancel.clone();
    let server = async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(shutdown_signal(server_cancel))
        .await
    };

    // 9. Run server; on exit cancel the worker and drain it.
    let result = server.await;

    if let Some(handle) = worker_handle {
        match tokio::time::timeout(std::time::Duration::from_secs(60), handle).await {
            Ok(Ok(())) => {}
            Ok(Err(err)) => tracing::error!(error = ?err, "worker task panicked"),
            Err(_) => tracing::warn!("worker did not stop within 60s; continuing shutdown"),
        }
    }

    result?;
    tracing::info!("server shut down cleanly");
    Ok(())
}

/// Builds the CORS layer from `CORS_ALLOWED_ORIGINS`.
///
/// Exact origins only — `Config` rejects `*`. Only the methods and
/// headers the API actually exposes. `allow_credentials` is off: auth is
/// bearer-token-based, not cookie-based.
fn build_cors_layer(config: &Config) -> anyhow::Result<CorsLayer> {
    use axum::http::{header, HeaderValue, Method};

    if config.cors_allowed_origins.is_empty() {
        return Ok(CorsLayer::new());
    }

    let origins: Vec<HeaderValue> = config
        .cors_allowed_origins
        .iter()
        .map(|o| {
            HeaderValue::from_str(o)
                .map_err(|err| anyhow::anyhow!("invalid CORS origin `{o}`: {err}"))
        })
        .collect::<Result<_, _>>()?;

    Ok(CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
        .max_age(std::time::Duration::from_secs(3600)))
}

async fn shutdown_signal(cancel: CancellationToken) {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("received Ctrl+C, shutting down"),
        _ = terminate => tracing::info!("received SIGTERM, shutting down"),
    }

    cancel.cancel();
}
