//! Subcomponents for the contract detail page.

pub mod custom_reminder_form;
pub mod edit_form;
pub mod reminders;
pub mod risks_and_obligations;

pub use custom_reminder_form::CustomReminderForm;
pub use edit_form::ContractEditForm;
pub use reminders::RemindersSection;
pub use risks_and_obligations::{ObligationsSection, RisksAndObligations, RisksSection};
