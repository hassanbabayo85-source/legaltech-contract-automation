# Frontend

The LexHack frontend is a Leptos CSR (client-side rendered) single-page
app compiled to WebAssembly. It talks to the backend REST API over
HTTPS, using the same bearer-token authentication the API exposes.

## Technology

* **Leptos 0.7** — reactive UI framework.
* **leptos_router 0.7** — client-side routing.
* **gloo-net** — fetch-based HTTP client.
* **Trunk** — WASM bundler.
* **wasm-bindgen** — JS/WASM interop.
* **No CSS framework.** A single hand-written stylesheet defines the
  design system.

The frontend has **no server-side rendering**. Everything runs in the
browser.

## Repository layout

    frontend/
    ├── Cargo.toml           # WASM crate
    ├── Trunk.toml           # Trunk build configuration
    ├── index.html           # entry HTML (Trunk injects the bundle)
    ├── styles/main.css      # design system
    └── src/
        ├── main.rs          # WASM entry point (calls `bootstrap`)
        ├── lib.rs           # module root, `bootstrap()` installer
        ├── api/             # HTTP client + wire DTOs
        │   ├── client.rs    # `ApiClient` — the only place fetch lives
        │   ├── config.rs    # build-time API base URL
        │   ├── error.rs     # `ApiError` + classification
        │   ├── models.rs    # typed DTOs matching the backend
        │   └── tests.rs     # pure-logic unit tests
        ├── auth/            # token storage, context, route guard
        ├── components/      # reusable UI (shell, badges, dialog, ...)
        ├── pages/           # route-level components
        ├── state/           # toast notifications
        └── app.rs           # root component + router

## Routes

| Path | Component | Auth |
|---|---|---|
| `/login` | `LoginPage` | public |
| `/register` | `RegisterPage` | public |
| `/` | `DashboardPage` | required |
| `/contracts` | `ContractsPage` | required |
| `/contracts/new` | `ContractNewPage` | required |
| `/contracts/:id` | `ContractDetailPage` | required |
| `/reminders` | `RemindersPage` | required |
| `/notification-channels` | `ChannelsPage` | required |
| `/settings` | `SettingsPage` | required |

Protected routes are wrapped in `<ProtectedRoute>`, which redirects
unauthenticated users to `/login`. **This is a UX guard, not a
security boundary** — the backend independently rejects any request
without a valid bearer token.

## Development

Prerequisites:

* Rust toolchain (stable).
* `wasm32-unknown-unknown` target:
  `rustup target add wasm32-unknown-unknown`
* `trunk`:
  `cargo install trunk --locked`
* `wasm-bindgen-cli` matching the version in `Cargo.lock`. Trunk will
  install it automatically if it can reach the network.

Run the dev server:

    cd frontend
    trunk serve

This starts a server at `http://localhost:8080` with hot reload. Open
it in a browser.

The backend must be running at `http://localhost:3000` (its default
`PORT`). Its `CORS_ALLOWED_ORIGINS` must include
`http://localhost:8080` — this is the default in `.env.example`.

### API base URL

The frontend reads the backend URL at **build time** from the
`API_BASE_URL` environment variable. It is baked into the WASM bundle.
There is no runtime configuration and no secret here — the API base URL
is a public value.

Development (defaults to `http://localhost:3000`):

    trunk serve

Production:

    API_BASE_URL=https://api.lexhack.example.com trunk build --release

**Only non-secret values may be passed to `API_BASE_URL`.** The value
is visible in the compiled WASM.

## Production build

    cd frontend
    API_BASE_URL=https://api.lexhack.example.com trunk build --release

Output goes to `frontend/dist/`. It contains:

* `index.html`
* `main-<hash>.css`
* `lexhack-frontend-<hash>.js`
* `lexhack-frontend-<hash>_bg.wasm`

Serve the directory with any static host (nginx, Caddy, S3+CloudFront,
...). Configure the host to rewrite all non-file paths to `index.html`
so client-side routing works on refresh.

Example nginx snippet:

    location / {
        root /srv/lexhack/dist;
        try_files $uri $uri/ /index.html;
    }

## Design system

A small, hand-written stylesheet. Key rules:

* **Light/dark mode** via `prefers-color-scheme` — no JS toggle needed.
* **Risk levels are never colour-only.** Every badge carries text and,
  for risks, a small glyph (`● ◐ ▲ ■`).
* **Focus outlines are never suppressed.** `:focus-visible` has an
  explicit, high-contrast outline.
* **Reduced motion is respected.** Animations are disabled or slowed
  under `prefers-reduced-motion`.

## Accessibility

* Semantic HTML (`<nav>`, `<main>`, `<article>`, `<section>`,
  `<table>`, `<dialog>`).
* Labels on every form input.
* `aria-live` on the toast region and on inline status messages.
* `<dialog>` for confirmations, with focus trap and escape-to-close
  supplied by the browser.
* Error messages are rendered in `<div role="alert">` next to their
  form.
* Keyboard navigation works end-to-end. No interactive control requires
  a mouse.

## Testing

Unit tests (pure logic):

    cd frontend
    wasm-pack test --headless --chrome

Or, if `wasm-pack` is not installed:

    cd frontend
    cargo test --target wasm32-unknown-unknown --lib

Only the pure-logic tests in `src/api/tests.rs` run without a browser.
Component tests would require a DOM harness and are out of scope for
this Part.

## Known limitations

* No offline support, no service worker.
* No live update push — reminders and analysis status are refetched
  on demand.
* No SSR. The initial page load is a blank shell until the WASM bundle
  runs.
