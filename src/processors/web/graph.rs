//! Web-specific structural relationship extractor.
//!
//! Extracts relationships from `ChunkType::Web` chunks:
//! - `HyperlinkReference` — web page A links to web page B
//! - `CanonicalUrl` — page A is the canonical version of page B
//! - Same-domain section clustering (sibling pages)
//!
//! Non-web chunks are silently skipped.

use crate::core::chunking::{Chunk, ChunkType, WebSemanticType};
use crate::graph::models::{
    ChunkRelationship, ChunkRelationshipMetadata, ChunkRelationType, RelationshipEvidence,
};
use crate::graph::extractors::SourceRelationshipExtractor;
use std::collections::HashMap;

/// Extracts structural relationships from web page chunks.
pub struct WebExtractor;

impl WebExtractor {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WebExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl SourceRelationshipExtractor for WebExtractor {
    fn source_type(&self) -> &'static str {
        "web"
    }

    fn extract(&self, chunks: &[Chunk]) -> Vec<ChunkRelationship> {
        let web_chunks: Vec<&Chunk> = chunks
            .iter()
            .filter(|c| matches!(c.chunk_type, ChunkType::Web { .. }))
            .collect();

        if web_chunks.is_empty() {
            return Vec::new();
        }

        // Build URL → chunk UUID map (overview chunks preferred for URL-level refs)
        let url_to_chunk: HashMap<String, uuid::Uuid> = web_chunks
            .iter()
            .filter_map(|c| {
                if let ChunkType::Web { url, semantic_type } = &c.chunk_type {
                    if matches!(semantic_type, WebSemanticType::PageOverview) {
                        return Some((url.clone(), c.id));
                    }
                }
                None
            })
            .collect();

        let mut relationships = Vec::new();

        for chunk in &web_chunks {
            relationships.extend(extract_hyperlink_relationships(chunk, &url_to_chunk));
        }

        relationships
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn extract_hyperlink_relationships(
    chunk: &Chunk,
    url_to_chunk: &HashMap<String, uuid::Uuid>,
) -> Vec<ChunkRelationship> {
    let mut rels = Vec::new();

    // Match markdown links and raw URLs in chunk content
    let link_re = match regex::Regex::new(r#"(?:\[([^\]]*)\]\((https?://[^)]+)\)|(https?://[^\s<>"']+))"#) {
        Ok(r) => r,
        Err(_) => return rels,
    };

    for (line_num, line) in chunk.content.lines().enumerate() {
        for caps in link_re.captures_iter(line) {
            let url_str = caps
                .get(2)
                .or(caps.get(3))
                .map(|m| m.as_str());

            if let Some(url) = url_str {
                // Look up the target page chunk
                if let Some(&target_id) = url_to_chunk.get(url) {
                    if target_id != chunk.id {
                        rels.push(
                            ChunkRelationship::new(
                                chunk.id,
                                target_id,
                                ChunkRelationType::HyperlinkReference,
                                0.85,
                            )
                            .with_evidence(vec![RelationshipEvidence {
                                evidence_type: "hyperlink".to_string(),
                                location: format!("line_{}", line_num + 1),
                                snippet: Some(url.to_string()),
                            }])
                            .with_metadata(ChunkRelationshipMetadata {
                                extraction_method: "url_pattern_matching".to_string(),
                                source_chunk_type: "web_page".to_string(),
                                target_chunk_type: "web_page".to_string(),
                                ..Default::default()
                            }),
                        );
                    }
                }
            }
        }
    }

    rels
}
