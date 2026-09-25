//! Reusable presentational components.

pub mod icons;

pub mod badge;
pub mod dialog;
pub mod loading;
pub mod safe_text;
pub mod shell;
pub mod sidebar;

pub use badge::{Badge, RiskBadge, StatusBadge};
pub use dialog::ConfirmDialog;
pub use loading::{EmptyState, ErrorState, ListSkeleton, PageLoading, Spinner};
pub use safe_text::SafeText;
pub use shell::AppShell;
