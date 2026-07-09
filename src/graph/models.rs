//! Chunk-to-chunk relationship models with proper UUID-based connections.
//!
//! This module defines the core data structures for chunk relationships
//! where both source and target are always identified by chunk UUIDs.

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

/// A relationship between two chunks, always identified by their UUIDs.
///
/// This is the unified relationship type that replaces the disconnected
/// `Relationship` (extractor.rs) and `TypedRelationship` (types.rs).
/// Both `source_chunk_id` and `target_chunk_id` are always chunk UUIDs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkRelationship {
    pub id: Uuid,
    /// Source chunk UUID — always a chunk, never a file or repo ID
    pub source_chunk_id: Uuid,
    /// Target chunk UUID — always a chunk, never a file or repo ID
    pub target_chunk_id: Uuid,
    /// Type of relationship between the two chunks
    pub relationship_type: ChunkRelationType,
    /// Confidence score (0.0–1.0)
    pub confidence: f32,
    /// Evidence supporting this relationship
    pub evidence: Vec<RelationshipEvidence>,
    /// Additional metadata
    pub metadata: ChunkRelationshipMetadata,
    /// Natural language description of the fact
    pub fact: Option<String>,
    /// When the fact became true
    pub valid_at: Option<DateTime<Utc>>,
    /// When the fact stopped being true
    pub invalid_at: Option<DateTime<Utc>>,
    /// When this relationship was created
    pub created_at: DateTime<Utc>,
}

impl ChunkRelationship {
    pub fn new(
        source_chunk_id: Uuid,
        target_chunk_id: Uuid,
        relationship_type: ChunkRelationType,
        confidence: f32,
    ) -> Self {
        // Deterministic ID generation based on source, target, and type
        let namespace = Uuid::new_v5(&Uuid::NAMESPACE_OID, b"unified-processor-relationship");
        let hash_input = format!("{}_{}_{:?}", source_chunk_id, target_chunk_id, relationship_type);
        let id = Uuid::new_v5(&namespace, hash_input.as_bytes());

        Self {
            id,
            source_chunk_id,
            target_chunk_id,
            relationship_type,
            confidence: confidence.clamp(0.0, 1.0),
            evidence: Vec::new(),
            metadata: ChunkRelationshipMetadata::default(),
            fact: None,
            valid_at: None,
            invalid_at: None,
            created_at: Utc::now(),
        }
    }

    pub fn with_evidence(mut self, evidence: Vec<RelationshipEvidence>) -> Self {
        self.evidence = evidence;
        self
    }

    pub fn with_metadata(mut self, metadata: ChunkRelationshipMetadata) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn with_fact(mut self, fact: impl Into<String>) -> Self {
        self.fact = Some(fact.into());
        self
    }

    pub fn with_valid_at(mut self, dt: DateTime<Utc>) -> Self {
        self.valid_at = Some(dt);
        self
    }

    pub fn with_invalid_at(mut self, dt: DateTime<Utc>) -> Self {
        self.invalid_at = Some(dt);
        self
    }
}

/// Enumeration of relationship types between chunks.
///
/// Covers all source types: code, documents, web, conversations,
/// schemas, and transcripts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ChunkRelationType {
    // ── Code ──────────────────────────────────────────────────────────────────
    /// Function chunk calls another function chunk
    FunctionCalls,
    /// Class chunk inherits from another class chunk
    ClassInherits,
    /// File chunk imports another file chunk
    FileImports,
    /// Class chunk contains a method (function chunk)
    ClassContainsMethod,
    /// Function chunk references a variable chunk
    FunctionReferencesVariable,
    /// File or module chunk defines a function or class
    Defines,
    /// Chunk implements a trait/interface defined in another chunk
    Implements,
    /// Test chunk tests a function/class chunk
    Tests,
    /// Chunk instantiates a class from another chunk
    Instantiates,
    /// Chunk decorates another function/class (e.g. Python decorators, TS annotations)
    Decorates,
    /// Chunk references a type defined in another chunk (parameter, return, field, generic)
    TypeReference,

    // ── Document ──────────────────────────────────────────────────────────────
    /// Document chunk references a code chunk
    DocumentReferencesCode,
    /// Document chunk links to another document chunk (markdown / HTML link)
    DocumentReferencesDoc,

    // ── Web ───────────────────────────────────────────────────────────────────
    /// Web page chunk links to another web page chunk via hyperlink
    HyperlinkReference,
    /// Web page chunk is the canonical version of another page chunk
    CanonicalUrl,

    // ── Conversation ──────────────────────────────────────────────────────────
    /// Message chunk is a reply to another message chunk
    ThreadReply,
    /// Message chunk @-mentions an entity linked to another chunk
    MentionReference,
    /// Conversation segment continues the topic from a preceding segment
    TopicContinuation,

    // ── Schema ────────────────────────────────────────────────────────────────
    /// DB table chunk references another table chunk via foreign key
    ForeignKeyRelation,
    /// API endpoint chunk uses a data model chunk
    ApiEndpointUsesModel,

    // ── Transcript ────────────────────────────────────────────────────────────
    /// Action item chunk derives from a decision chunk
    ActionItemFromDecision,
    /// Question chunk paired with its answer chunk
    QaPair,
    /// Sequential speaker-turn chunks in a transcript
    SpeakerTurnSequence,

    // ── Universal ─────────────────────────────────────────────────────────────
    /// Sequential chunk in reading order (all source types)
    NextChunk,
    /// Parent-child hierarchical relationship (all source types)
    ParentChild,
    /// Sibling chunks sharing the same parent (all source types)
    Sibling,
    /// Configuration chunk affects another chunk
    Configures,
    /// Generic dependency relationship
    DependsOn,
    /// Fallback relationship when no other structural relationships are found
    SameSource,

    // ── Semantic / Dynamic ────────────────────────────────────────────────────
    /// Arbitrary semantic relationship (e.g. extracted via LLMs, like WORKS_AT)
    Semantic(String),
}

impl ChunkRelationType {
    /// Human-readable label for graph edge display
    pub fn label(&self) -> &str {
        match self {
            // Code
            Self::FunctionCalls => "CALLS",
            Self::ClassInherits => "INHERITS",
            Self::FileImports => "IMPORTS",
            Self::ClassContainsMethod => "CONTAINS_METHOD",
            Self::FunctionReferencesVariable => "REFERENCES",
            Self::Defines => "DEFINES",
            Self::Implements => "IMPLEMENTS",
            Self::Tests => "TESTS",
            Self::Instantiates => "INSTANTIATES",
            Self::TypeReference => "TYPE_REF",
            Self::Decorates => "DECORATES",
            // Document
            Self::DocumentReferencesCode => "DOCUMENTS_CODE",
            Self::DocumentReferencesDoc => "LINKS_TO",
            // Web
            Self::HyperlinkReference => "HYPERLINKS_TO",
            Self::CanonicalUrl => "CANONICAL_OF",
            // Conversation
            Self::ThreadReply => "REPLIES_TO",
            Self::MentionReference => "MENTIONS",
            Self::TopicContinuation => "CONTINUES_TOPIC",
            // Schema
            Self::ForeignKeyRelation => "FOREIGN_KEY",
            Self::ApiEndpointUsesModel => "USES_MODEL",
            // Transcript
            Self::ActionItemFromDecision => "ACTION_FROM_DECISION",
            Self::QaPair => "ANSWERS",
            Self::SpeakerTurnSequence => "FOLLOWED_BY",
            // Universal
            Self::NextChunk => "NEXT_CHUNK",
            Self::ParentChild => "PARENT_OF",
            Self::Sibling => "SIBLING_OF",
            Self::Configures => "CONFIGURES",
            Self::DependsOn => "DEPENDS_ON",
            Self::SameSource => "SAME_SOURCE",
            Self::Semantic(ref label) => label,
        }
    }

    /// Category for grouping and filtering in the knowledge graph
    pub fn category(&self) -> &str {
        match self {
            Self::FunctionCalls
            | Self::ClassInherits
            | Self::ClassContainsMethod
            | Self::FunctionReferencesVariable
            | Self::Defines
            | Self::Implements
            | Self::FileImports
            | Self::Tests
            | Self::Instantiates
            | Self::Decorates
            | Self::TypeReference => "code",
            Self::DocumentReferencesCode
            | Self::DocumentReferencesDoc => "document",
            Self::HyperlinkReference | Self::CanonicalUrl => "web",
            Self::ThreadReply | Self::MentionReference | Self::TopicContinuation => "conversation",
            Self::ForeignKeyRelation | Self::ApiEndpointUsesModel => "schema",
            Self::ActionItemFromDecision | Self::QaPair | Self::SpeakerTurnSequence => "transcript",
            Self::NextChunk | Self::ParentChild | Self::Sibling => "hierarchy",
            Self::Configures | Self::DependsOn | Self::SameSource => "structural",
            Self::Semantic(_) => "semantic",
        }
    }
}

/// Evidence supporting a chunk relationship.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationshipEvidence {
    /// Type of evidence (e.g., "function_call", "import_statement", "inheritance")
    pub evidence_type: String,
    /// Location in source (line number, AST node path)
    pub location: String,
    /// Raw evidence snippet from source
    pub snippet: Option<String>,
}

/// Additional metadata for a chunk relationship.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChunkRelationshipMetadata {
    /// Extraction method used (e.g., "ast_analysis", "regex_pattern", "structural")
    pub extraction_method: String,
    /// Source chunk's semantic type (e.g., "Function", "Class")
    pub source_chunk_type: String,
    /// Target chunk's semantic type
    pub target_chunk_type: String,
    /// Additional key-value metadata
    pub custom: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ASTNodeDef {
    pub name: String,
    pub node_type: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_line: usize,
    pub end_line: usize,
}

/// AST reference data stored in chunk metadata.
///
/// Used by the unified relationship extractor to map AST nodes
/// (function names, class names) back to chunk UUIDs.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ASTData {
    /// Function names declared in this chunk
    pub function_names: Vec<String>,
    /// Class/struct names declared in this chunk
    pub class_names: Vec<String>,
    /// Granular definitions with byte ranges (for semantic chunking)
    pub defined_nodes: Vec<ASTNodeDef>,
    /// Imported module/file paths in this chunk
    pub import_paths: Vec<String>,
    /// Function names called within this chunk
    pub function_calls: Vec<String>,
    /// Classes inherited by classes in this chunk
    pub parent_classes: Vec<String>,
    /// Variables referenced in this chunk
    pub variable_references: Vec<String>,
    /// Classes instantiated in this chunk
    pub instantiations: Vec<String>,
    /// Decorators/annotations applied in this chunk
    pub decorators: Vec<String>,
    /// The programming language of this chunk (if code)
    pub language: Option<String>,
    /// Type references from parameters, return types, fields, generics
    pub type_references: Vec<TypeRef>,
    /// Trait/interface implementations: (implementing_type, trait_name)
    pub trait_implementations: Vec<(String, String)>,
}

/// A type reference extracted from code — parameter type, return type, field type, or generic arg.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeRef {
    /// The type name referenced
    pub type_name: String,
    /// Context: "parameter_type", "return_type", "field", "generic_arg"
    pub context: String,
}

/// Document-specific relationship data.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DocumentData {
    /// Links to other documents or external resources
    pub links: Vec<DocumentLink>,
    /// Code blocks referenced in the document
    pub code_blocks: Vec<CodeReference>,
    /// Tables and figures referenced
    pub tables: Vec<TableReference>,
    /// Cross-document references
    pub document_references: Vec<String>,
}

/// Web page specific relationship data.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WebData {
    /// Hyperlinks found on the page
    pub hyperlinks: Vec<Hyperlink>,
    /// Canonical URL if specified
    pub canonical_url: Option<String>,
    /// Navigation structure items
    pub navigation_items: Vec<NavigationItem>,
    /// External resource references
    pub external_resources: Vec<String>,
}

/// Conversation-specific relationship data.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConversationData {
    /// Thread reply structure
    pub thread_replies: Vec<ThreadReply>,
    /// @-mentions of entities or users
    pub mentions: Vec<Mention>,
    /// Topic continuation indicators
    pub topic_continuations: Vec<TopicLink>,
    /// Cross-channel references
    pub cross_channel_refs: Vec<String>,
}

/// Schema-specific relationship data.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SchemaData {
    /// Foreign key relationships
    pub foreign_keys: Vec<ForeignKey>,
    /// API endpoint to model relationships
    pub endpoint_model_refs: Vec<EndpointModelRef>,
    /// Type dependencies
    pub type_dependencies: Vec<TypeDependency>,
    /// Cross-schema references
    pub external_schema_refs: Vec<String>,
}

/// Transcript-specific relationship data.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TranscriptData {
    /// Action items extracted from content
    pub action_items: Vec<ActionItem>,
    /// Question-answer pairs
    pub qa_pairs: Vec<QAPair>,
    /// Speaker turn sequence
    pub speaker_turns: Vec<SpeakerTurn>,
    /// Decision references
    pub decisions: Vec<Decision>,
}

// Supporting structures for document relationships
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentLink {
    pub url: String,
    pub text: String,
    pub link_type: String, // "internal", "external", "code"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeReference {
    pub language: String,
    pub code_snippet: String,
    pub file_reference: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableReference {
    pub table_id: String,
    pub caption: Option<String>,
    pub reference_type: String, // "figure", "table"
}

// Supporting structures for web relationships
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hyperlink {
    pub url: String,
    pub anchor_text: String,
    pub link_type: String, // "internal", "external", "navigation"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavigationItem {
    pub text: String,
    pub target_url: String,
    pub hierarchy_level: u8,
}

// Supporting structures for conversation relationships
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadReply {
    pub parent_message_id: String,
    pub reply_message_id: String,
    pub thread_depth: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mention {
    pub mentioned_entity: String,
    pub mention_type: String, // "user", "channel", "external"
    pub context: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicLink {
    pub from_topic: String,
    pub to_topic: String,
    pub confidence: f32,
}

// Supporting structures for schema relationships
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForeignKey {
    pub from_table: String,
    pub to_table: String,
    pub column: String,
    pub constraint_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointModelRef {
    pub endpoint_path: String,
    pub model_name: String,
    pub operation: String, // "GET", "POST", "PUT", "DELETE"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeDependency {
    pub from_type: String,
    pub to_type: String,
    pub dependency_type: String, // "extends", "implements", "uses"
}

// Supporting structures for transcript relationships
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionItem {
    pub description: String,
    pub assignee: Option<String>,
    pub due_date: Option<String>,
    pub source_decision: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QAPair {
    pub question: String,
    pub answer: String,
    pub questioner: String,
    pub answerer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeakerTurn {
    pub speaker: String,
    pub text: String,
    pub sequence_order: u32,
    pub timestamp_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub description: String,
    pub decision_maker: String,
    pub timestamp_ms: Option<u64>,
    pub action_items: Vec<String>,
}

/// Batch of chunk relationships for efficient storage/transmission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkRelationshipBatch {
    /// Source identifier (repo URL, document ID)
    pub source_id: String,
    /// All relationships in this batch
    pub relationships: Vec<ChunkRelationship>,
    /// When this batch was created
    pub created_at: DateTime<Utc>,
}

impl ChunkRelationshipBatch {
    pub fn new(source_id: String, relationships: Vec<ChunkRelationship>) -> Self {
        Self {
            source_id,
            relationships,
            created_at: Utc::now(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.relationships.is_empty()
    }

    pub fn len(&self) -> usize {
        self.relationships.len()
    }
}
