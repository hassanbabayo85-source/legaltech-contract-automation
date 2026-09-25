//! Background reminder worker.
//!
//! Polls for due reminders, atomically claims them, dispatches through
//! a [`NotificationChannel`], and updates the reminder's state. Safe to
//! run concurrently across multiple application instances: the
//! database claim (`SELECT ... FOR UPDATE SKIP LOCKED` +
//! `pending → processing`) is the only place selection happens, so
//! two workers cannot both send the same reminder.
//!
//! # Lifecycle
//!
//! The worker is spawned by `main.rs` and cancelled through a
//! [`tokio_util::sync::CancellationToken`]. On cancellation it stops
//! polling, waits up to `WORKER_SHUTDOWN_TIMEOUT_SECONDS` for in-flight
//! deliveries, then returns. Any reminder still in `processing` when a
//! worker dies is recovered by the periodic sweep (see
//! [`recovery`]) — never silently lost.

pub mod claim;
pub mod outcome;
pub mod poller;
pub mod recovery;

pub use poller::spawn;
