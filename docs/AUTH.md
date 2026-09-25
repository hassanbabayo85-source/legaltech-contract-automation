# Authentication

Server-side sessions with opaque bearer tokens, transmitted via the
`Authorization: Bearer <token>` header.

## Why Bearer tokens, not cookies

Cookies are sent by browsers automatically, so any state-changing
endpoint needs CSRF defense. A Bearer token in an `Authorization` header
is not sent automatically, so a cross-site request cannot impersonate
the user without already knowing the token. This matches the OAuth 2.0
bearer-token model and avoids pulling CSRF tokens into a backend that
currently has no browser client.

## Password hashing

Argon2id via the `argon2` crate. Parameters come from `Config`:

| Parameter | Default | Meaning |
|---|---|---|
| `ARGON2_M_COST` | 19456 KiB (19 MiB) | Memory cost |
| `ARGON2_T_COST` | 2 | Iterations |
| `ARGON2_P_COST` | 1 | Parallelism |

These are OWASP's 2026 baseline. Hashes are stored in PHC format
(`$argon2id$v=19$m=19456,t=2,p=1$<salt>$<hash>`), which encodes the
parameters used — so a future cost change does not invalidate existing
hashes.

Plaintext passwords are never stored, never logged, never returned in
any response. Verification uses the crate's constant-time comparison.

## Session tokens

A session token is **32 random bytes**, base64url-encoded (43 ASCII
characters). It is returned to the client **exactly once**, in the
`token` field of the register/login response.

The server stores only `sha256(raw_token)` in `sessions.token_hash`.
That means:

* A leaked database cannot be turned into live sessions (attacker would
  need to reverse SHA-256 on a 256-bit random input).
* Lookups are a single indexed equality check.

SHA-256 is deliberate: Argon2 is a *password* KDF, designed to make up
for passwords' low entropy. A session token already has 256 bits of
entropy, so a slow KDF would only add latency to every authenticated
request without improving security.

## Session lifetime and revocation

* Lifetime: `SESSION_LIFETIME_SECONDS` (default 7 days).
* `POST /api/auth/logout` revokes the current session (`revoked_at` set
  to `NOW()`), and the token is immediately rejected on subsequent
  requests.
* Expired sessions (`expires_at <= NOW()`) and revoked sessions
  (`revoked_at IS NOT NULL`) are both filtered out by the lookup query.
* `db::sessions::delete_stale_sessions(cutoff)` removes expired rows and
  rows revoked before `cutoff`. A future worker (PART 05+) should call
  it periodically; the function is provided and tested now.

## Endpoints

| Method | Path | Body | Response |
|---|---|---|---|
| POST | `/api/auth/register` | `{email, password, full_name}` | `201 {user, token, expires_at}` |
| POST | `/api/auth/login` | `{email, password}` | `200 {user, token, expires_at}` |
| POST | `/api/auth/logout` | — (Bearer token) | `204` |
| GET | `/api/auth/me` | — (Bearer token) | `200 {id, email, full_name, created_at}` |

`user` never includes `password_hash`. `token` is returned once and
never again.

## Authentication middleware

`AuthenticatedUser` is an Axum extractor that:

1. Requires an `Authorization: Bearer <token>` header.
2. Hashes the token with SHA-256.
3. Looks up an active session (`token_hash` match, not revoked, not
   expired).
4. Touches `last_used_at` best-effort.
5. Attaches `user_id` and `session_id` to the request.

Any handler that takes `AuthenticatedUser` as a parameter is guaranteed
to be running for a verified user. **Handlers must never read an
identity out of the request body or query string** — the only trusted
source is `AuthenticatedUser`.

## Authorization (for PART 04+)

`AuthenticatedUser.user_id` is the *only* basis for ownership checks.
Resource queries must be written as:

```sql
SELECT ... FROM contracts WHERE id = $1 AND user_id = $2
