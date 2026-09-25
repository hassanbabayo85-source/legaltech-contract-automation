# Frontend security model

The frontend is untrusted code that runs in the user's browser. The
backend enforces every security property; the frontend only *shows*.
This document describes the frontend-side decisions that matter.

## Where the session token lives

The bearer token issued by `POST /api/auth/login` (and `/register`) is
stored in `localStorage` under `lexhack.session.token`. It is read on
app start, attached to every authenticated request as
`Authorization: Bearer <token>`, and removed on logout or when the
backend reports `401`.

### Why `localStorage`

The SPA must survive a page reload. A session token held only in
memory (a signal, or `sessionStorage`) would be lost on every refresh.
Persisting in `localStorage` is the simplest option that keeps a user
signed in across reloads.

### Why this is acceptable

`localStorage` is **vulnerable to XSS**. If an attacker can execute
arbitrary JavaScript on the page, no client-side storage is safe — a
token in memory is equally exposed. This codebase therefore treats
XSS prevention as the actual security boundary, not `localStorage`:

* The frontend **never renders untrusted content as HTML**. Contract
  text, AI risk descriptions, AI evidence, and every user-supplied
  string reaches the DOM as **text nodes**, via Leptos' `view!` macro
  and the `SafeText` component. The `inner_html` and
  `dangerously_set_inner_html` APIs are not used anywhere.
* No third-party scripts are loaded. The WASM bundle is the only
  script on the page.
* The `Content-Security-Policy` header is not currently set by the
  backend. When it is, the policy should disallow `unsafe-inline` and
  restrict script sources to `'self'`.

### What is NOT stored

* Passwords (never sent to the frontend, never stored).
* Password hashes (never sent to the frontend).
* Notification-channel credentials (Telegram bot tokens, Discord
  webhook URLs, webhook auth secrets). The backend's `ChannelResponse`
  DTO does not include config; the frontend only ever shows a
  "Configured" indicator.
* Contract text in URLs, query strings, or `localStorage`. Contract
  text is fetched on demand and rendered as text; it is not persisted
  client-side.

## XSS prevention

Every dynamic string in the UI goes through one of:

* Leptos' `{value}` interpolation inside `view!`, which inserts a text
  node.
* `<SafeText text=.../>`, a thin wrapper that does the same thing and
  exists so its usage can be grepped and audited.

`<SafeText>` is the recommended way to render contract text, AI
descriptions, AI evidence, and user names. It never interprets HTML.

Do not introduce `inner_html`, `dangerously_set_inner_html`, or manual
`element.set_inner_html(...)` calls. If a future feature needs rich
formatting (e.g. Markdown), sanitize it with a well-known library and
document the sanitizer here.

## API error handling

The `ApiError` type classifies every failure into one of a small set
of categories. **Only `user_message()` is shown to the user.** It
never includes:

* The raw backend error body (except its `message` field, which the
  backend has already sanitized).
* SQL details, stack traces, filesystem paths.
* Provider responses.
* Any credential.

`category()` is used only for local logging and never displayed.

## Logging

The frontend does not log secrets. `tracing_wasm` writes structured
events to the browser console, but no call site passes a token,
password, or contract text into a tracing field:

* The auth module does not log the token. It only logs the *presence*
  of a token on bootstrap failure (via `category`).
* The API client does not log request or response bodies.
* Components log nothing except through toasts (which are user-visible
  and contain only safe text).

## Untrusted input

Contract text and AI output are treated as untrusted:

* They are rendered as text (`SafeText`).
* They are never placed in a URL, a query string, a `data:` URL, or an
  HTML attribute that would be interpreted as markup.
* They are never sent to analytics, log shippers, or third-party
  services.

## What the frontend does *not* do

* It does not perform authorization. A signed-in user could, in
  principle, force a route to render with a modified state; the
  backend would still reject the underlying requests.
* It does not enforce rate limits. The backend's per-user and per-IP
  rate limits are the authority.
* It does not encrypt anything. Channel credentials are encrypted by
  the backend before storage; the frontend never holds the plaintext
  after submission.
* It does not send secrets to the backend more than once. Channel
  credentials are POSTed at creation, replaced on PATCH if the user
  supplies new values, and never read back.
