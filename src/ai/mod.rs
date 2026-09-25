//! AI provider abstraction and contract-analysis types.
//!
//! The application depends on [`AiProvider`] — an object-safe trait with
//! a single `analyze_contract` operation. The concrete implementation
//! (today: [`openai::OpenAiProvider`], which speaks the OpenAI chat
//! completions protocol and therefore works with any OpenAI-compatible
//! endpoint) is chosen at startup and injected into [`AppState`].
//!
//! Business logic never sees the wire protocol. It sees a validated
//! [`validation::ValidatedAnalysis`] or a typed [`AiError`].

pub mod openai;
pub mod prompt;
pub mod provider;
pub mod schema;
pub mod validation;

pub use provider::{AiError, AiProvider};
pub use schema::AiAnalysis;
pub use validation::ValidatedAnalysis;
