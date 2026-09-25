//! Authentication and authorization.
//!
//! # Architecture
//!
//! Server-side sessions with opaque bearer tokens, transmitted via the
//! `Authorization: Bearer <token>` header. No cookies are used.
//!
//! * **Password storage** — Argon2id (OWASP defaults) via the `argon2`
//!   crate. See [`password`].
//! * **Session token storage** — a 32-byte random token is generated
//!   per session, returned to the client exactly once, and stored in the
//!   database only as its SHA-256 hash. See [`session`].
//! * **Authentication middleware** — the [`AuthenticatedUser`] extractor
//!   validates the header, hashes the token, looks up an active session,
//!   and attaches `user_id` + `session_id` to the request. See
//!   [`middleware`].
//! * **Authorization** — receiving `AuthenticatedUser` in a handler
//!   guarantees a verified identity. Future resource handlers MUST use
//!   `auth.user_id` for ownership checks, never a client-supplied ID.
//!   See `docs/AUTH.md`.
//!
//! # Why Bearer tokens, not cookies?
//!
//! Cookies require CSRF defense for state-changing endpoints. A Bearer
//! token in an `Authorization` header is not sent by the browser
//! automatically, so cross-site requests cannot impersonate the user —
//! the attacker would need to already know the token. This is the same
//! reasoning used by the OAuth 2.0 bearer-token spec.

pub mod handlers;
pub mod middleware;
pub mod password;
pub mod router;
pub mod session;
pub mod validation;

pub use middleware::AuthenticatedUser;
