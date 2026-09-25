//! Notification delivery.
//!
//! The worker (see `src/worker/`) uses [`NotificationChannel`] to send
//! a reminder. Three concrete channels are provided:
//!
//! * [`telegram::TelegramChannel`] — Telegram Bot API.
//! * [`discord::DiscordChannel`] — Discord webhook.
//! * [`webhook::WebhookChannel`] — generic HTTP POST with JSON body.
//!
//! Channels are selected by `reminders.channel_type` (see
//! `src/reminders/types.rs::ChannelType`). An unknown channel is a
//! permanent failure — the worker never silently substitutes a
//! different channel.
//!
//! # Delivery guarantee
//!
//! The system provides **at-least-once** delivery. A reminder may be
//! retried if the worker crashes after the provider accepted the
//! request but before the state transition to `sent` was committed.
//! Idempotency is requested from the provider via the
//! `Idempotency-Key: <reminder_id>` header where applicable.

pub mod channel;
pub mod config;
pub mod discord;
pub mod http;
pub mod secrets;
pub mod ssrf;
pub mod store;
pub mod telegram;
pub mod webhook;

pub use channel::{DeliveryError, Notification, NotificationChannel};
