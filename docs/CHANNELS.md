# Notification channels

Users configure their own notification destinations through
`/api/notification-channels`. The worker dispatches each reminder
through the enabled channel of the reminder's channel_type for the
contract's owner.

## Supported channels

| Type | Credentials |
|---|---|
| telegram | bot_token, chat_id |
| discord | webhook_url (a credential) |
| webhook | url, optional auth (bearer or hmac) |

## Endpoints

| Method | Path | Purpose |
|---|---|---|
| POST | /api/notification-channels | Create |
| GET | /api/notification-channels | List (paginated) |
| GET | /api/notification-channels/:id | Get one |
| PATCH | /api/notification-channels/:id | Update |
| DELETE | /api/notification-channels/:id | Delete |
| POST | /api/notification-channels/:id/test | Send a test message |

All endpoints require Authorization: Bearer <token>.

## Create

Request:

    POST /api/notification-channels
    {
      "channel_type": "telegram",
      "name": "My Telegram",
      "bot_token": "123456:ABC...",
      "chat_id": "-1001234567890"
    }

Response:

    {
      "id": "uuid",
      "channel_type": "telegram",
      "name": "My Telegram",
      "enabled": true,
      "created_at": "...",
      "updated_at": "...",
      "last_tested_at": null,
      "last_test_error": null
    }

The bot_token and chat_id are not echoed. They are encrypted and
stored in notification_channels.encrypted_config.

## Update

PATCH accepts any subset of name, enabled, and the channel config
fields. If no config field is present, the existing encrypted config
is preserved - a PATCH {"name": "New name"} will not erase the bot
token.

## Test

POST /api/notification-channels/:id/test sends a fixed test
notification through the channel's currently stored config. It:

- does not modify any reminder
- does not create a reminder
- uses no contract data
- passes through the same SSRF validator as real dispatches

Response:

    { "ok": true, "error": null }

or

    { "ok": false, "error": "ssrf_blocked" }

The error field is a short category string, never a provider
response.

## Delivery behaviour

The worker looks up the enabled channel of the reminder's type for the
contract owner. If no such channel exists, the reminder fails
permanently with channel_not_configured - no retry budget is consumed.

## Enabled uniqueness

At most one enabled channel per (user_id, channel_type) is allowed,
enforced by a partial unique index. A disabled channel does not count,
so a user can keep a Telegram channel around while trying Discord.
