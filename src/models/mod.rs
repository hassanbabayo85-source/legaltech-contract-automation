//! Strongly typed representations of database rows.
//!
//! Each struct corresponds to one table in the schema defined under
//! `migrations/`. They are plain data — no behavior, no persistence
//! concerns — and are used with `sqlx::query_as` to decode rows.
//!
//! These types deliberately do **not** derive `Serialize`/`Deserialize`.
//! The wire representation of a contract or a user will differ from the
//! database row (for example, `password_hash` must never leave the
//! server), so API DTOs belong in the API layer, not here.

pub mod audit;
pub mod contract;
pub mod notification_channel;
pub mod obligation;
pub mod reminder;
pub mod risk;
pub mod session;
pub mod user;

pub use audit::AuditLog;
pub use contract::{Contract, ContractSummaryRow};
pub use notification_channel::NotificationChannel;
pub use obligation::ContractObligation;
pub use reminder::Reminder;
pub use risk::ContractRisk;
pub use session::Session;
pub use user::User;
