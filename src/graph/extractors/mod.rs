//! Source-agnostic relationship extraction system.
//!
//! # Architecture
//! Each source type (code, document, web, conversation, schema, transcript)
//! has a dedicated extractor that understands the structural relationships
//! within that domain.  The `SourceRelationshipRouter` dispatches chunks to
//! the correct extractor and then aggregates all results.
//!
//! Cross-source relationship detection (connecting chunks from different
//! sources/repositories) is handled by `fast-fetcher`, not here.
//!
//! # Adding a new source type
//! 1. Create `extractor/<source>.rs` implementing `SourceRelationshipExtractor`
//! 2. Add a field to `SourceRelationshipRouter`
//! 3. Call `.<source>.extract(chunks)` inside `extract_all()`

pub mod semantic;
mod hierarchical;

use crate::core::chunking::Chunk;
use crate::graph::models::ChunkRelationship;

pub use crate::processors::codebase::graph::CodeExtractor;
pub use crate::processors::documents::graph::DocumentExtractor;
pub use crate::processors::web::graph::WebExtractor;

pub use semantic::SemanticExtractor;
use hierarchical::HierarchicalExtractor;

// ─── Trait ─────────────────────────────────────────────────────────────────

/// Trait for source-type-specific structural relationship extraction.
///
/// Implementors filter incoming `chunks` to only those relevant to their
/// source type and return a `Vec<ChunkRelationship>` representing the
/// structural relationships found within that subset.
pub trait SourceRelationshipExtractor: Send + Sync {
    /// Short label identifying this extractor (used in logs and metadata).
    fn source_type(&self) -> &'static str;

    /// Extract relationships from the given chunk slice.
    ///
    /// Implementations MUST silently skip chunk types they don't handle.
    fn extract(&self, chunks: &[Chunk]) -> Vec<ChunkRelationship>;
}

// ─── Router ─────────────────────────────────────────────────────────────────

/// Routes chunks to the correct source-type extractor and aggregates results.
///
/// Only extracts relationships within a single source. Cross-source
/// relationship discovery is delegated to `fast-fetcher`.
pub struct SourceRelationshipRouter {
    hierarchical: HierarchicalExtractor,
    code: CodeExtractor,
    document: DocumentExtractor,
    web: WebExtractor,
}

impl Default for SourceRelationshipRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl SourceRelationshipRouter {
    pub fn new() -> Self {
        Self {
            hierarchical: HierarchicalExtractor::new(),
            code: CodeExtractor::new(),
            document: DocumentExtractor::new(),
            web: WebExtractor::new(),
        }
    }

    /// Extract structural relationships from all chunks within a single source.
    ///
    /// Runs:
    /// 1. `HierarchicalExtractor` for parent-child / sibling (all source types)
    /// 2. Each source-type extractor for domain-specific relationships
    pub fn extract_all(&self, chunks: &[Chunk]) -> Vec<ChunkRelationship> {
        let mut relationships = Vec::new();

        // Hierarchical relationships apply to every source type
        relationships.extend(self.hierarchical.extract(chunks));

        // Source-specific structural extraction (same-source only)
        relationships.extend(self.code.extract(chunks));
        relationships.extend(self.document.extract(chunks));
        relationships.extend(self.web.extract(chunks));

        relationships
    }
}
