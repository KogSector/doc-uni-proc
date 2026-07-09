//! Hierarchical relationship extractor — applies to ALL source types.
//!
//! Extracts `NextChunk` relationships connecting sequential chunks within
//! the same file. This replaces the dense O(N^2) parent-child and sibling graphs
//! to guarantee perfect reading order with linear connections.

use crate::core::chunking::Chunk;
use crate::graph::models::{
    ChunkRelationship, ChunkRelationType, RelationshipEvidence, ChunkRelationshipMetadata,
};
use crate::graph::extractors::SourceRelationshipExtractor;
use fnv::FnvHashMap;

/// Extracts sequential reading order relationships from chunks.
pub struct HierarchicalExtractor;

impl HierarchicalExtractor {
    pub fn new() -> Self {
        Self
    }
}

impl SourceRelationshipExtractor for HierarchicalExtractor {
    fn source_type(&self) -> &'static str {
        "hierarchical"
    }

    fn extract(&self, chunks: &[Chunk]) -> Vec<ChunkRelationship> {
        let mut relationships = Vec::new();

        // Group chunks by file_path to ensure we only link chunks within the same document
        let mut chunks_by_file: FnvHashMap<&str, Vec<&Chunk>> = FnvHashMap::default();
        for chunk in chunks {
            chunks_by_file
                .entry(&chunk.file_path)
                .or_default()
                .push(chunk);
        }

        for (_, mut file_chunks) in chunks_by_file {
            if file_chunks.len() < 2 {
                continue;
            }

            // Sort chunks by their start position to ensure correct sequential order
            file_chunks.sort_by(|a, b| {
                let a_pos = a.metadata.byte_range.map(|r| r.0).or(a.metadata.line_range.map(|r| r.0)).unwrap_or(0);
                let b_pos = b.metadata.byte_range.map(|r| r.0).or(b.metadata.line_range.map(|r| r.0)).unwrap_or(0);
                a_pos.cmp(&b_pos)
            });

            // Create NextChunk relationships sequentially
            for i in 0..(file_chunks.len() - 1) {
                let current = file_chunks[i];
                let next = file_chunks[i + 1];

                // Prevent self-referential NEXT_CHUNK loops
                if current.id == next.id {
                    continue;
                }

                relationships.push(
                    ChunkRelationship::new(
                        current.id,
                        next.id,
                        ChunkRelationType::NextChunk,
                        1.0,
                    )
                    .with_evidence(vec![RelationshipEvidence {
                        evidence_type: "sequential_order".to_string(),
                        location: "document_structure".to_string(),
                        snippet: None,
                    }])
                    .with_metadata(ChunkRelationshipMetadata {
                        extraction_method: "structural".to_string(),
                        source_chunk_type: "sequential".to_string(),
                        target_chunk_type: "sequential".to_string(),
                        ..Default::default()
                    }),
                );
            }
        }

        relationships
    }
}
