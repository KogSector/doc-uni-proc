// Migrated from relationships
pub mod models;

pub mod extractors;

pub mod chunk;
pub mod episode;
pub mod consumer;
pub mod correlation;

pub use consumer::{EventConsumer, UnifiedEventConsumer};
pub use chunk::{
    EventHeaders, EventMetadata, FileType, SourceType, EntityHint, 
    ChunkRawEvent, EmbeddingGeneratedEvent,
    SimplifiedChunkMetadata, SimplifiedChunk, SimplifiedChunkRawEvent,
    SimplifiedEmbedding, SimplifiedEmbeddingGeneratedEvent
};
pub use episode::*;
pub use correlation::correlation_middleware;
