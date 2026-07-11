/// Kafka topic constants for the platform.
pub struct Topics;
impl Topics {
    pub const EMBEDDING_GENERATED: &'static str = "embedding.generated";
    pub const CHUNKS_RAW: &'static str = "chunks.raw";
    pub const REPO_EVENTS: &'static str = "repo.events";
}

/// Kafka topic constants for the new event types.
pub mod semantic_os_topics {
    pub const SLACK_MESSAGES: &str = "slack.messages.received";
    pub const MEETING_TRANSCRIPTS: &str = "meetings.transcripts.received";
    pub const API_SCHEMAS: &str = "api.schemas.updated";
    pub const CHUNK_DEPRECATED: &str = "chunks.deprecated";
    pub const ENTITY_MERGED: &str = "entities.merged";
    pub const RELATIONSHIP_INFERRED: &str = "relationships.inferred";
    pub const DOCUMENT_UPDATED: &str = "documents.updated";
}
