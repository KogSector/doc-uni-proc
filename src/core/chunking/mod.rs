//! Unified Chunking System
//! 
//! Provides robust, boundary-aware token-density chunking inspired by graphiti logic.

use async_trait::async_trait;

pub mod types;
pub mod hybrid;

pub use types::{
    ChunkingConfig,
    Chunk, ChunkMetadata, ChunkType, ChunkLevel, ProcessingTier,
    ChunkingResult, ChunkingStats, QualityScore,
    CodeSemanticType, DocumentSemanticType, WebSemanticType,
    SourcePermissions, ProvenanceEntry,
    EntityHint,
};
pub use hybrid::HybridChunker;

/// Trait for chunking strategies
#[async_trait]
pub trait ChunkingStrategy: Send + Sync {
    /// Process content and generate chunks
    async fn process(
        &self,
        content: &str,
        filename: &str,
        source_id: &str,
        config: &ChunkingConfig,
    ) -> anyhow::Result<ChunkingResult>;
    
    /// Get strategy name
    fn name(&self) -> &str;
    
    /// Check if strategy supports this file type
    fn supports(&self, filename: &str) -> bool;
}
