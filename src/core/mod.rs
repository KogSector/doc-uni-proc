//! Core functionality for the unified processor

pub mod config;
pub mod error;
pub mod orchestrator;
pub mod chunking;
pub mod scanner;

pub use config::Config;
pub use error::{ProcessorError, Result};
pub use orchestrator::UnifiedProcessor;

pub mod routes;
