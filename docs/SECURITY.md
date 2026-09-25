# Security model

This document describes the security controls in place as of PART 08.

## Authentication and sessions

- Passwords hashed with Argon2id (OWASP defaults).
- Sessions are server-side rows. The client receives a 32-byte
  base64url bearer token exactly once; only its SHA-256 hash is
  stored in `sessions.token_hash`.
- Bearer tokens are transmitted in the `Authorization: Bearer ...`
  header. Cookies are not used.
- Logout revokes the session server-side.
- Expired and revoked sessions are rejected on every request.

## Authorization

- Every contract-scoped query includes `WHERE user_id = $N` in SQL.
- Every channel-scoped query includes `WHERE user_id = $N` in SQL.
- Every reminder-scoped query joins through `contracts.user_id`.
- Nonexistent and foreign resources return 404.

## Notification channel secrets

Channel credentials (Telegram bot tokens, Discord webhook URLs,
webhook authentication secrets) are:

1. Parsed into typed Rust structures.
2. Validated.
3. Serialized to JSON.
4. Encrypted with ChaCha20-Poly1305 (AEAD) using a 256-bit key
   from `NOTIFICATION_SECRET_KEY`.
5. Stored as nonce(12) || ciphertext+tag in
   `notification_channels.encrypted_config`.

The raw configuration never appears in any API response, log line,
or error message.

A fresh 96-bit nonce is generated for every encryption.

### Key management

- The key must be present at startup in production.
- The key must be 32 bytes, base64-encoded. Use
  `openssl rand -base64 32` to generate one.
- Rotation is not supported in this Part. See below.

## SSRF protection

Every outbound webhook destination is validated before the request:

1. Only https:// is accepted in production.
2. The host is resolved via the system resolver.
3. Every resolved IP must pass a blocklist:
   - Loopback (127.0.0.0/8, ::1)
   - Private IPv4 (10/8, 172.16/12, 192.168/16)
   - Carrier-grade NAT (100.64/10)
   - Link-local (169.254/16, fe80::/10)
   - IPv6 unique-local (fc00::/7)
   - Unspecified (0.0.0.0, ::)
   - Broadcast, documentation ranges
4. The HTTP client is configured with redirect::Policy::none().

DNS rebinding is closed by two layers:

1. Every resolved address must pass the blocklist above.
2. The outbound HTTP client is built per-request with
   `reqwest::ClientBuilder::resolve_to_addrs(host, &validated)` so
   the connection is made only to an IP that was previously
   validated. A second DNS lookup during the TLS/connect phase
   cannot redirect the request.

## Rate limiting

Per-IP (unauthenticated endpoints):

- POST /api/auth/login: 5 / minute / IP
- POST /api/auth/register: 3 / hour / IP

Per-user (authenticated endpoints):

- POST /api/contracts/:id/analyze: 10 / hour / user
- POST /api/notification-channels/:id/test: 5 / minute / user
- Channel mutations: 20 / hour / user

All limits are configurable via environment variables. Over-limit
responses are 429 Too Many Requests with a Retry-After header.

The limiter is in-memory and single-instance.

## Request body limits

- Contract create / update: 4 MiB
- Auth endpoints: 8 KiB
- Notification channels: 32 KiB

## Outbound request headers

Every outbound request is built with an explicit header list. Incoming
request headers are never forwarded.

## Response headers

Every HTTP response carries:

- X-Content-Type-Options: nosniff
- Referrer-Policy: no-referrer
- Cache-Control: no-store

## Sensitive logging

The following are never written to logs, error responses, or audit
metadata:

- Telegram bot tokens
- Discord webhook credentials
- Generic webhook secrets
- NOTIFICATION_SECRET_KEY
- API keys
- Passwords or password hashes
- Session tokens
- Authorization / Cookie headers
- Contract raw_text

## Error responses

Client-facing errors never expose SQL errors, stack traces,
filesystem paths, provider credentials, raw provider responses,
internal network addresses, encryption keys, or configuration values.

## Audit logs

Audit entries record safe operational metadata only. No secret
material and no contract text ever appears in audit_logs.metadata.

## Known limitations

- Multi-instance rate limiting: the limiter is in-process.
- Key rotation: not supported.
- Database error details are reduced to a category string.
- DNS rebinding on connection: closed by pinning the per-request
  HTTP client to the exact addresses that were validated (see
  "DNS rebinding" above). The connection cannot resolve the host a
  second time.
