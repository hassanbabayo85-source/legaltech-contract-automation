# LexGuard — AI-Assisted Contract Risk & Deadline Dispatcher

LexGuard analyzes legal contracts, surfaces risks, extracts deadlines,
and dispatches reminders. This repository contains the complete backend
(Rust / Axum / SQLx) and a Leptos CSR frontend compiled to WebAssembly.

**Status:** Parts 01–10 complete, plus a focused **AI trust-hardening
pass**. The full pipeline — register → contract → AI analysis →
risks/obligations → reminders → notification delivery → audit — works
end to end and was verified against a real AI provider.

> **Legal disclaimer.** LexGuard provides AI-assisted contract risk
> analysis and deadline management. It does **not** provide legal
> advice and does **not** replace a qualified legal professional.

> **Read this before trusting any output:**
> [`docs/KNOWN_LIMITATIONS.md`](docs/KNOWN_LIMITATIONS.md) — a blunt
> list of what LexGuard cannot do and where its AI is unreliable.

## Architecture

    Frontend (Leptos CSR / WASM)
       ↓  HTTP / REST + Bearer token
    Backend (Axum)
       ├── Auth           — Argon2id, opaque bearer sessions
       ├── Contracts      — CRUD, ownership-enforced
       ├── AI analysis    — OpenAI-compatible, validated JSON,
       │                    fuzzy evidence verification
       ├── PDF extraction — pdf-extract (embedded text layer)
       ├── Image OCR      — Groq vision model (Qwen 3.8)
       ├── Reminders      — idempotent generation, worker dispatcher
       ├── Channels       — Telegram / Discord / webhook, encrypted
       └── Worker         — claims due reminders, retries, delivers,
                            plus crash recovery for reminders **and**
                            stale AI analyses
       ↓
    PostgreSQL (SQLx migrations, 7 schema revisions)
       ↓
    External: AI provider, Telegram, Discord, webhooks

See `docs/` for the design notes:
`DATABASE.md`, `AUTH.md`, `AI_ANALYSIS.md`, `REMINDERS.md`,
`WORKER.md`, `SECURITY.md`, `CHANNELS.md`, `FRONTEND.md`,
`FRONTEND_SECURITY.md`, `DEMO.md`, and `KNOWN_LIMITATIONS.md`.

## Features

* **Auth** — register, login, logout, session restore, rate limited.
* **Contracts** — create, list (paginated, sortable, filterable),
  view, edit (bumps `content_version`), delete. `raw_text` never
  appears in list responses.
* **Contract input** — paste text, upload a **PDF** (text layer
  extracted server-side), or upload an **image** (JPEG / PNG / WebP,
  OCR’d by a vision model).
* **AI analysis** — OpenAI-compatible provider; server-side
  structural validation of every field; retries with bounded
  exponential backoff; timeouts; stale-result protection; **fuzzy
  evidence verification** that rejects hallucinated quotes.
* **Risks and obligations** — extracted from AI output, rendered
  with evidence excerpts, filterable by risk level.
* **Reminders** — generated from deadlines (7 / 3 / 1 / 0 days),
  custom reminders, cancel, idempotent regeneration.
* **Notification channels** — Telegram, Discord, generic HTTPS
  webhook. Credentials encrypted at rest (ChaCha20-Poly1305).
* **Worker** — atomic claim, bounded concurrency, exponential
  backoff, `Retry-After` support, graceful shutdown, crash recovery
  for both reminders and analyses.
* **Security** — SSRF blocklist (DNS-rebinding pinned), no secret
  logging, per-user rate limits, request body caps, security
  response headers.
* **Frontend** — responsive Leptos CSR app; no untrusted HTML is
  ever rendered.

## Repository layout

    .
    ├── Cargo.toml               # workspace (backend + frontend)
    ├── Dockerfile               # multi-stage production image
    ├── docker-compose.yml       # local stack: postgres + migrate + app
    ├── .env.example             # every env var documented
    ├── migrations/              # SQLx migrations, applied in order
    ├── scripts/seed_demo.sh     # demo data via the REST API
    ├── src/                     # backend source
    ├── tests/                   # backend integration tests
    ├── frontend/                # Leptos CSR crate
    ├── docs/                    # design + security + demo docs
    └── .github/workflows/ci.yml # CI (Postgres service, Trunk build)

## Requirements

* Rust stable toolchain (edition 2021).
* PostgreSQL 13 or newer.
* For the frontend: `wasm32-unknown-unknown` target and `trunk`.
* For Docker: Docker + Docker Compose (optional).

## Environment variables

Every variable is documented inline in `.env.example`. Summary:

| Variable | Required | Default | Purpose |
|---|---|---|---|
| `DATABASE_URL` | yes | — | PostgreSQL connection string |
| `HOST` / `PORT` | no | `0.0.0.0` / `3000` | Bind address |
| `RUST_LOG` | no | `info` | tracing EnvFilter |
| `DATABASE_MAX_CONNECTIONS` | no | `5` | Pool size |
| `SESSION_LIFETIME_SECONDS` | no | `604800` | Session lifetime |
| `ARGON2_M_COST` / `_T_` / `_P_` | no | 19456 / 2 / 1 | Argon2id cost |
| `RATE_LIMIT_*` | no | see `.env.example` | Rate limits |
| `AI_API_KEY` | **optional** | — | `analyze` returns 502 without it |
| `AI_BASE_URL` / `AI_MODEL` | no | OpenAI defaults | Provider choice |
| `AI_TIMEOUT_SECONDS` / `AI_MAX_INPUT_CHARS` / `AI_MAX_RETRIES` | no | 60 / 100000 / 2 | AI controls |
| `WORKER_*` | no | see `.env.example` | Worker tuning |
| `NOTIFICATION_*` | no | see `.env.example` | Delivery timeouts and retries |
| `NOTIFICATION_SECRET_KEY` | **yes (prod)** | — | 32-byte base64 AEAD key |
| `TELEGRAM_BOT_TOKEN` / `TELEGRAM_CHAT_ID` | no | — | Env-based channel (optional) |
| `DISCORD_WEBHOOK_URL` | no | — | Env-based channel (optional) |
| `WEBHOOK_URL` | no | — | Env-based channel (optional) |
| `WEBHOOK_ALLOW_LOOPBACK` | no | `false` | **TEST ONLY** — leave false |
| `CORS_ALLOWED_ORIGINS` | no | dev origin | Exact origins, no wildcard |
| `FRONTEND_DIST` | no | — | Serve frontend static files if set |

The server refuses to start in production without
`NOTIFICATION_SECRET_KEY`. Generate one with `openssl rand -base64 32`.

### Provider compatibility notes

`AI_BASE_URL` must **not** include `/v1`. The client appends the
correct suffix. Tested combinations:

| Provider | `AI_BASE_URL` | `AI_MODEL` |
|---|---|---|
| Groq | `https://api.groq.com/openai` | `openai/gpt-oss-120b` |
| OpenAI | `https://api.openai.com` | `gpt-4o-mini` |
| Gemini | `https://generativelanguage.googleapis.com/v1beta/openai` | `gemini-3.6-flash` |

Groq rejected `response_format: {"type": "json_object"}` on the models
we tried, so the prompt relies on explicit JSON instructions instead.

## Database

    # Once, if not already installed:
    cargo install sqlx-cli --no-default-features --features rustls,postgres

    export DATABASE_URL="postgres://lexhack:lexhack_dev_pw@127.0.0.1:5432/lexhack_test"

    # Apply migrations — either with sqlx-cli:
    sqlx migrate run

    # …or using the backend binary itself:
    cargo run -- --migrate

Migrations are numbered `0001_` through `0007_` and are applied in
lexicographic order. Never edit an applied migration — add a new one.

## Running the backend

The backend reads its configuration from a **`.env` file** at the
repository root (via `dotenvy`). You do not need to `export` anything.

    cp .env.example .env
    # edit .env — set NOTIFICATION_SECRET_KEY and (optionally) AI_API_KEY
    cargo run

A helper script is provided:

    ./run.sh          # start the server
    ./run.sh migrate  # apply migrations then exit
    ./run.sh test     # run the full test suite

The server starts the HTTP API on `:3005` (as configured in `.env`)
and the background worker.

## Running the frontend

    cd frontend
    API_BASE_URL="http://localhost:3005" trunk serve --port 8080

The dev server listens on `http://localhost:8080`. The backend must
be running on the URL you passed, and `CORS_ALLOWED_ORIGINS` must
include `http://localhost:8080`.

Production build:

    cd frontend
    API_BASE_URL=https://api.lexguard.example.com trunk build --release

Output goes to `frontend/dist/`. To serve it from the backend itself,
set `FRONTEND_DIST=/path/to/frontend/dist` when starting the backend.

## Docker

A full local stack:

    cp .env.example .env
    # edit .env — set NOTIFICATION_SECRET_KEY and (optionally) AI_API_KEY
    docker compose up --build

Then open `http://localhost:3000`.

The Compose stack runs PostgreSQL, applies migrations via the
backend binary's `--migrate` mode, and starts the app. Data persists
in the `lexhack_pgdata` volume.

> **Note:** The Docker build was **not** exercised in the development
> environment used for this build (limited data budget). The Compose
> configuration and Dockerfile were statically reviewed, and
> `docker compose config` parses cleanly. Do not assume the image
> builds without a first successful local run.

## Testing

Backend:

    export DATABASE_URL="postgres://lexhack:lexhack_dev_pw@127.0.0.1:5432/lexhack_test"
    cargo test --workspace --all-targets      # 324 tests
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    cargo fmt --all -- --check

Frontend:

    cd frontend
    cargo fmt -- --check
    cargo check --all-targets
    cargo clippy --all-targets --all-features -- -D warnings
    cargo check --target wasm32-unknown-unknown --all-targets
    API_BASE_URL=http://localhost:3005 trunk build --release

Integration tests use `#[sqlx::test]` and require a real PostgreSQL.
They fail loudly if the database is unavailable — never silently skip.

## Security

Full model in `docs/SECURITY.md`. Highlights:

* **Passwords** — Argon2id, per-hash salt, constant-time verify.
* **Sessions** — 32-byte random tokens; only SHA-256 of the token
  is stored.
* **Ownership** — every query filters by `user_id`.
* **Channel credentials** — ChaCha20-Poly1305 at rest; never returned
  by any API response.
* **SSRF** — HTTPS-only, DNS-resolved IP blocklist (loopback,
  private, link-local, CGNAT, metadata, IPv6 ULA), redirects
  disabled, and the outbound HTTP client is **pinned** to the
  validated addresses to close the DNS-rebinding window.
* **Rate limits** — per-IP for unauth, per-user for auth endpoints.
* **Body limits** — 32 KiB for channels, 8 KiB for auth, 4 MiB for
  contracts, 20 MiB for PDF/image uploads.
* **Headers** — `X-Content-Type-Options: nosniff`,
  `Referrer-Policy: no-referrer`, `Cache-Control: no-store`.
* **XSS** — frontend never renders untrusted content as HTML.
* **Prompt injection** — the contract is explicitly labelled
  untrusted data in the system prompt.

## AI limitations

* Analysis output is AI-generated. It can be wrong, incomplete, or
  miss legally significant terms.
* Every extracted field is validated server-side before storage.
  Validation checks *structure* and *evidence provenance* — not
  *legal correctness*.
* **Evidence verification is fuzzy.** Quoted evidence must appear in
  the source contract (exact substring, or ≥ 70 % significant-token
  overlap). This rejects a meaningful class of hallucinations but
  does not eliminate them.
* **Output is non-deterministic.** The same contract analyzed twice
  can produce different results.
* The system presents AI-identified risks with evidence excerpts;
  it does not conclude that a contract is legally risky.
* Risk scores are informational, not legal judgments.
* For the full list, see
  [`docs/KNOWN_LIMITATIONS.md`](docs/KNOWN_LIMITATIONS.md).

## Demo

See `docs/DEMO.md` for a scripted walkthrough. The demo uses a
clearly fictional contract; the seed script is
`scripts/seed_demo.sh`.

## Known limitations (short list)

* **Frontend component tests** — only pure-logic unit tests are
  included (`frontend/src/api/tests.rs`). A DOM-based component test
  harness is out of scope.
* **Key rotation** — `NOTIFICATION_SECRET_KEY` cannot be rotated
  without re-encrypting every channel credential.
* **Multi-instance rate limits** — the in-memory limiter is
  single-process.
* **Worker delivery guarantee** — at-least-once. A reminder may be
  delivered more than once if a worker crashes after the provider
  accepted the request.
* **Docker build** — not exercised in the development environment
  used for this build.
* **Provider coverage** — end-to-end analysis was verified against
  Groq only; the code path is OpenAI-compatible but other providers
  were not exercised.
* **Trunk deprecation warning** — `clean.dist` in `Trunk.toml` is
  deprecated in Trunk 0.21 but still works.

The full, blunt list lives in
[`docs/KNOWN_LIMITATIONS.md`](docs/KNOWN_LIMITATIONS.md).
