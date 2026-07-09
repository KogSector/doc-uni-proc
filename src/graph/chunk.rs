//! Event Definitions for ConFuse Platform

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// =============================================================================
// Common Types
// =============================================================================

/// Event headers included in all events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventHeaders {
    pub event_id: String,
    pub event_type: String,
    pub timestamp: String,
    pub source_service: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
}

impl EventHeaders {
    pub fn new(source_service: impl Into<String>, event_type: impl Into<String>) -> Self {
        Self {
            event_id: Uuid::new_v4().to_string(),
            event_type: event_type.into(),
            timestamp: Utc::now().to_rfc3339(),
            source_service: source_service.into(),
            correlation_id: None,
            trace_id: None,
        }
    }
    
    pub fn with_correlation_id(mut self, correlation_id: impl Into<String>) -> Self {
        self.correlation_id = Some(correlation_id.into());
        self
    }
    
    pub fn with_trace_id(mut self, trace_id: impl Into<String>) -> Self {
        self.trace_id = Some(trace_id.into());
        self
    }
}

/// Event metadata for processing context
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EventMetadata {
    #[serde(default)]
    pub retry_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_event_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
}

/// File type classification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FileType {
    Unknown,
    Code,
    Document,
}

/// Source types for ingestion
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    #[serde(rename = "unknown")]
    Unknown,
    #[serde(rename = "github")]
    Github,
    #[serde(rename = "gitlab")]
    Gitlab,
    #[serde(rename = "bitbucket")]
    Bitbucket,
    #[serde(rename = "local")]
    Local,
    #[serde(rename = "gdrive")]
    Gdrive,
    #[serde(rename = "notion")]
    Notion,
    #[serde(rename = "file_upload")]
    FileUpload,
    #[serde(rename = "dropbox")]
    Dropbox,
    #[serde(rename = "onedrive")]
    Onedrive,
    #[serde(rename = "web")]
    Web,
    #[serde(rename = "url")]
    Url,
    #[serde(rename = "salesforce")]
    Salesforce,
    #[serde(rename = "hubspot")]
    Hubspot,
    #[serde(rename = "drata")]
    Drata,
    #[serde(rename = "vanta")]
    Vanta,
    #[serde(rename = "confluence")]
    Confluence,
    #[serde(rename = "sql_database")]
    SqlDatabase,
    #[serde(rename = "nosql_database")]
    NosqlDatabase,
}

// =============================================================================
// Chunk Events
// =============================================================================

/// Entity hint pre-identified in a chunk by the agentic chunker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityHint {
    /// The entity surface form as it appears in the text
    pub text: String,
    /// Entity type (organization, person, technology, concept, location, etc.)
    pub entity_type: String,
    /// Confidence that this is a real entity (0.0–1.0)
    pub confidence: f32,
    /// Byte offset where entity starts in the chunk content
    pub start_offset: usize,
    /// Byte offset where entity ends in the chunk content
    pub end_offset: usize,
}

/// Simplified chunk metadata for event serialization
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChunkMetadata {
    /// Line range in source file
    pub line_range: Option<(usize, usize)>,
    /// Byte range in source file
    pub byte_range: Option<(usize, usize)>,
    /// Complexity score (1-10)
    pub complexity_score: u8,
    /// Token count (approximate)
    pub token_count: usize,
    /// Pre-identified entity hints from agentic chunker
    #[serde(default)]
    pub entity_hints: Vec<EntityHint>,
    /// Relationship hints in "subject -> predicate -> object" notation
    #[serde(default)]
    pub relationship_context: Vec<String>,
    /// Custom key-value metadata
    #[serde(default)]
    pub custom: std::collections::HashMap<String, serde_json::Value>,
}

/// Event published when raw chunks are created with entity hints
/// Emitted by unified-processor after intelligent chunking; consumed by embeddings-service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkRawEvent {
    pub headers: EventHeaders,
    #[serde(default)]
    pub metadata: EventMetadata,
    pub source_id: String,
    pub file_id: String,
    pub chunk_id: String,
    /// Raw text content of chunk
    pub content: String,
    /// Chunk type (code, text, table, etc.)
    pub chunk_type: String,
    /// Granularity level
    pub level: String,
    /// Processing tier applied
    pub tier: String,
    /// Confidence score (0.0-1.0)
    pub confidence: f32,
    /// Quality score (0.0-1.0, populated after enhancement)
    #[serde(default)]
    pub quality_score: Option<f32>,
    /// Chunk metadata
    pub chunk_metadata: ChunkMetadata,
    /// Pre-identified entity hints from agentic chunker
    #[serde(default)]
    pub entity_hints: Vec<EntityHint>,
    /// Relationship hints in "subject -> predicate -> object" notation
    #[serde(default)]
    pub relationship_context: Vec<String>,
    /// Creation timestamp
    #[serde(default = "chrono::Utc::now")]
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl ChunkRawEvent {
    pub fn topic() -> &'static str {
        "chunks.raw"
    }
}

// =============================================================================
// Embedding Events
// =============================================================================

/// Event published when embeddings have been generated
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingGeneratedEvent {
    pub headers: EventHeaders,
    #[serde(default)]
    pub metadata: EventMetadata,
    pub file_id: String,
    pub source_id: String,
    pub chunk_ids: Vec<String>,
    pub embedding_model: String,
    pub embedding_dimension: u32,
    pub total_chunks: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vector_storage_location: Option<String>,
    pub processing_time_ms: u64,
}

impl EmbeddingGeneratedEvent {
    pub fn topic() -> &'static str {
        "embedding.generated"
    }
}

// =============================================================================
// =============================================================================

/// Simplified chunk metadata for raw chunks
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SimplifiedChunkMetadata {
    /// Line range in source file
    pub line_range: Option<(usize, usize)>,
    /// Byte range in source file
    pub byte_range: Option<(usize, usize)>,
    /// Complexity score (1-10)
    pub complexity_score: u8,
    /// Token count (approximate)
    pub token_count: usize,
    /// Quality score (0.0-1.0)
    #[serde(default)]
    pub quality_score: Option<f32>,
    /// Language for code chunks
    #[serde(default)]
    pub language: Option<String>,
    /// Start line number
    #[serde(default)]
    pub start_line: Option<u32>,
    /// End line number
    #[serde(default)]
    pub end_line: Option<u32>,
    /// Confidence score
    #[serde(default)]
    pub confidence: Option<f32>,
}

/// Simplified chunk structure for raw chunks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimplifiedChunk {
    pub id: String,
    pub file_id: String,
    pub chunk_type: String, // function, class, etc.
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality_score: Option<f32>,
}

/// Event published when raw chunks are created (simplified flow)
/// Emitted by unified-processor; consumed by embeddings-service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimplifiedChunkRawEvent {
    pub headers: EventHeaders,
    #[serde(default)]
    pub metadata: EventMetadata,
    pub source_id: String,
    pub repo_name: Option<String>,
    pub chunks: Vec<SimplifiedChunk>,
    pub timestamp: String,
}

impl SimplifiedChunkRawEvent {
    pub fn topic() -> &'static str {
        "chunks.raw"
    }
}

/// Simplified embedding structure (without content - unified-processor already has it)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimplifiedEmbedding {
    pub id: String,
    pub file_id: String,
    pub chunk_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    pub embedding: Vec<f32>,
    pub model: String,
    pub dimension: u32,
}

/// Event published when embeddings are generated (simplified flow)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimplifiedEmbeddingGeneratedEvent {
    pub headers: EventHeaders,
    #[serde(default)]
    pub metadata: EventMetadata,
    pub source_id: String,
    pub repo_name: Option<String>,
    pub chunks: Vec<SimplifiedEmbedding>,
    pub model: String,
    pub timestamp: String,
}

impl SimplifiedEmbeddingGeneratedEvent {
    pub fn topic() -> &'static str {
        "embedding.generated"
    }
}
