# Reminder Worker & Notification Dispatcher

PART 07 introduces the background worker that turns due reminder
records into delivered notifications. It runs inside the same Tokio
runtime as the HTTP server and shuts down when the server does.

## Architecture

The worker is spawned by src/main.rs and cancelled through a shared
CancellationToken. It is disabled by setting WORKER_ENABLED=false.

## Configuration

| Variable | Default | Description |
|---|---|---|
| WORKER_ENABLED | true | Enable the background worker |
| WORKER_POLL_INTERVAL_SECONDS | 10 | Sleep between polls |
| WORKER_BATCH_SIZE | 50 | Max reminders claimed per poll |
| WORKER_MAX_CONCURRENCY | 8 | Max in-flight deliveries |
| WORKER_CLAIM_TIMEOUT_SECONDS | 300 | After this, a stale processing row is reset to pending |
| WORKER_SHUTDOWN_TIMEOUT_SECONDS | 30 | Drain window on shutdown |
| NOTIFICATION_REQUEST_TIMEOUT_SECONDS | 15 | Outbound HTTP timeout |
| NOTIFICATION_MAX_RETRIES | 5 | Max delivery attempts per reminder |
| TELEGRAM_BOT_TOKEN | - | Telegram bot token |
| TELEGRAM_CHAT_ID | - | Default Telegram chat id |
| DISCORD_WEBHOOK_URL | - | Discord webhook URL |
| WEBHOOK_URL | - | Default generic webhook URL |
| WEBHOOK_ALLOW_LOOPBACK | false | TEST ONLY. Never enable in production |

Missing channel credentials are not errors - a reminder for an
unconfigured channel fails with channel_not_configured (permanent).

## Claiming (single instance, multi instance, at-least-once)

The claim query uses SELECT ... FOR UPDATE SKIP LOCKED in the same
transaction as the pending -> processing transition:

    SELECT id, ... FROM reminders
     WHERE status = 'pending'
       AND reminder_date <= NOW()
       AND (next_attempt_at IS NULL OR next_attempt_at <= NOW())
     ORDER BY reminder_date, id
     FOR UPDATE SKIP LOCKED
     LIMIT $batch;

Because the row locks and the state update share one transaction:

- Two workers running simultaneously will observe disjoint sets of
  due reminders.
- A worker that crashes mid-flight leaves its rows in processing; the
  periodic recovery sweep resets any processing row whose claimed_at
  is older than WORKER_CLAIM_TIMEOUT_SECONDS back to pending.

Neither network delivery nor the state update is transactional with
the other. The system is therefore at-least-once: a reminder may be
delivered more than once if a worker dies after the provider accepted
the request but before the sent update committed. We do NOT claim
exactly-once delivery.

## Retry policy

After a failed delivery the worker calls schedule_retry_or_fail:

- Permanent (channel_not_configured, ssrf_blocked, invalid_url,
  HTTP 4xx other than 429): status -> failed immediately.
- Retryable (timeout, rate_limited, transient, HTTP 5xx):
  attempts += 1, next_attempt_at is set to NOW() + backoff, status
  stays pending.
- When attempts >= NOTIFICATION_MAX_RETRIES the reminder is moved to
  failed.

Backoff formula: min(5min, 30s * 2^attempt) + jitter(0..30s).
A provider Retry-After header parsed from a 429 overrides the
computed backoff.

## Channels

### Telegram

POST https://api.telegram.org/bot<token>/sendMessage with a JSON
body. Bot token from TELEGRAM_BOT_TOKEN, chat id from
TELEGRAM_CHAT_ID.

### Discord

POST DISCORD_WEBHOOK_URL with a JSON body. The URL is a credential -
it is never logged or returned in error responses.

### Generic webhook

POST WEBHOOK_URL with a structured JSON body:

    {
      "event": "contract_reminder_due",
      "reminder_id": "...",
      "contract_id": "...",
      "obligation_id": "...",
      "title": "...",
      "message": "...",
      "due_date": "2026-10-20",
      "reminder_type": "on_deadline"
    }

The raw_text of the contract is never included.

## SSRF protection

Every outbound webhook is validated before the request (see
src/notifications/ssrf.rs):

- Only https:// URLs are accepted. http://, file://, ftp://,
  gopher://, unix:// are rejected.
- The host is resolved via the system resolver.
- Every resolved IP must pass a blocklist that covers loopback
  (127.0.0.0/8, ::1), private IPv4 (10/8, 172.16/12, 192.168/16),
  carrier-grade NAT (100.64/10), link-local (169.254/16, fe80::/10 -
  this includes the metadata endpoint 169.254.169.254), IPv6
  unique-local (fc00::/7), unspecified (0.0.0.0, ::), and broadcast.
- The HTTP client is configured with redirect::Policy::none(), so
  redirects are treated as terminal failures.

WEBHOOK_ALLOW_LOOPBACK=true relaxes only the loopback rule. It exists
solely for integration tests using wiremock. Never enable it in
production.

## Graceful shutdown

On SIGINT/SIGTERM:

1. The HTTP server stops accepting new connections.
2. The worker cancellation token is fired.
3. The worker stops polling and waits up to
   WORKER_SHUTDOWN_TIMEOUT_SECONDS for in-flight deliveries.
4. Any reminder still in processing when a worker dies is recovered
   by the next instance sweep. Reminders are never marked sent
   without a confirmed delivery.

## Observability

Structured tracing events:

- worker_started / worker_stopped
- claimed_reminders (count)
- notification_sent (reminder_id, channel, duration_ms)
- notification_failed (reminder_id, channel, category, retryable,
  duration_ms)
- recovered_stale_processing (count)

Secret values, Authorization headers, and contract raw_text are never
logged. The Debug impl of Config redacts every credential field.

## Troubleshooting

Reminder stuck in processing - the worker that claimed it crashed.
The next instance recovery sweep resets it after
WORKER_CLAIM_TIMEOUT_SECONDS.

Channel always returns channel_not_configured - the corresponding
env var is unset.

Webhook rejected with ssrf_blocked - the destination resolved to a
private/loopback/link-local IP. This is intentional.

Duplicate notifications - expected under at-least-once semantics.
Providers that support idempotency keys receive
Idempotency-Key: <reminder_id> on each request.
