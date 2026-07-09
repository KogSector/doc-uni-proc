//! Unified Processor - ConFuse Platform
//!
//! Ingests code and documents, chunks them, runs AST analysis,
//! and stores data for knowledge graph construction.


pub mod core;
pub mod processors;
pub mod infra;
pub mod graph;



// Proto definitions for gRPC services removed.

use std::sync::Arc;

/// Application state shared across handlers
pub struct AppState {
    pub processor: core::orchestrator::UnifiedProcessor,
    pub config: core::Config,
}

impl AppState {
    pub fn new(processor: core::orchestrator::UnifiedProcessor, config: core::Config) -> Arc<Self> {
        Arc::new(Self { processor, config })
    }
}
