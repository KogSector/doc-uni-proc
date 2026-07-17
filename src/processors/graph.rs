//! Document-specific structural relationship extractor.
//!
//! Extracts relationships from `ChunkType::Document` chunks:
//! - Markdown / HTML links between document chunks
//! - Code block references (documentation → code)
//! - Citation / footnote references
//!
//! Non-document chunks are silently skipped.

use crate::core::chunking::{Chunk, ChunkType};
use crate::graph::models::{
    ChunkRelationship, ChunkRelationshipMetadata, ChunkRelationType, RelationshipEvidence,
};
use crate::graph::extractors::SourceRelationshipExtractor;
use std::collections::HashMap;

/// Extracts structural relationships from document chunks.
pub struct DocumentExtractor;

impl DocumentExtractor {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DocumentExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl SourceRelationshipExtractor for DocumentExtractor {
    fn source_type(&self) -> &'static str {
        "document"
    }

    fn extract(&self, chunks: &[Chunk]) -> Vec<ChunkRelationship> {
        let doc_chunks: Vec<&Chunk> = chunks
            .iter()
            .filter(|c| matches!(c.chunk_type, ChunkType::Document { .. }))
            .collect();

        if doc_chunks.is_empty() {
            return Vec::new();
        }

        // Index document chunks by file path for link resolution
        let path_to_chunk: HashMap<&str, uuid::Uuid> = doc_chunks
            .iter()
            .map(|c| (c.file_path.as_str(), c.id))
            .collect();

        let mut relationships = Vec::new();

        for chunk in &doc_chunks {
            // Markdown / HTML links
            relationships.extend(
                extract_link_relationships(chunk, &path_to_chunk)
            );

            // Citation references
            relationships.extend(
                extract_citation_relationships(chunk, &path_to_chunk)
            );
        }

        relationships
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn extract_link_relationships(
    chunk: &Chunk,
    path_to_chunk: &HashMap<&str, uuid::Uuid>,
) -> Vec<ChunkRelationship> {
    let mut rels = Vec::new();

    let md_link = match regex::Regex::new(r"\[([^\]]+)\]\(([^)]+)\)") {
        Ok(r) => r,
        Err(_) => return rels,
    };

    for (line_num, line) in chunk.content.lines().enumerate() {
        for caps in md_link.captures_iter(line) {
            if let Some(url) = caps.get(2) {
                let url_str = url.as_str();

                // Skip external links
                if url_str.starts_with("http://") || url_str.starts_with("https://") {
                    continue;
                }

                // Try to resolve the link to a chunk
                if let Some(&target_id) = path_to_chunk.get(url_str) {
                    if target_id != chunk.id {
                        rels.push(
                            ChunkRelationship::new(
                                chunk.id,
                                target_id,
                                ChunkRelationType::DocumentReferencesDoc,
                                0.90,
                            )
                            .with_evidence(vec![RelationshipEvidence {
                                evidence_type: "markdown_link".to_string(),
                                location: format!("line_{}", line_num + 1),
                                snippet: Some(format!(
                                    "[{}]({})",
                                    caps.get(1).map_or("", |m| m.as_str()),
                                    url_str
                                )),
                            }])
                            .with_metadata(ChunkRelationshipMetadata {
                                extraction_method: "markdown_link_parsing".to_string(),
                                source_chunk_type: "document".to_string(),
                                target_chunk_type: "document".to_string(),
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

fn extract_citation_relationships(
    chunk: &Chunk,
    _path_to_chunk: &HashMap<&str, uuid::Uuid>,
) -> Vec<ChunkRelationship> {
    let mut rels = Vec::new();

    let citation_re = match regex::Regex::new(r"(?:@cite\{([^}]+)\}|\[@([^\]]+)\])") {
        Ok(r) => r,
        Err(_) => return rels,
    };

    for (line_num, line) in chunk.content.lines().enumerate() {
        for caps in citation_re.captures_iter(line) {
            let cited_key = caps.get(1).or(caps.get(2));
            if let Some(key) = cited_key {
                // We emit a "self-referencing" citation entry — the target
                // chunk UUID cannot be resolved without a bibliography index.
                // Log the citation but skip creating a dangling edge.
                tracing::debug!(
                    chunk_id = %chunk.id,
                    line = line_num + 1,
                    citation_key = key.as_str(),
                    "Found citation reference (target chunk not resolvable without bibliography index)"
                );
                let _ = rels; // keep reference to avoid dead_code warnings
            }
        }
    }

    // Code-block language references — connect documentation to
    // code sections that reference the same file path.
    let code_block_re =
        match regex::Regex::new(r"```(\w+)(?:\s+([^\n`]+))?") {
            Ok(r) => r,
            Err(_) => return rels,
        };

    for (line_num, line) in chunk.content.lines().enumerate() {
        if let Some(caps) = code_block_re.captures(line) {
            if let Some(filename) = caps.get(2) {
                let fname = filename.as_str().trim();
                if let Some(&target_id) = _path_to_chunk.get(fname) {
                    if target_id != chunk.id {
                        rels.push(
                            ChunkRelationship::new(
                                chunk.id,
                                target_id,
                                ChunkRelationType::ReferencedIn,
                                0.85,
                            )
                            .with_evidence(vec![RelationshipEvidence {
                                evidence_type: "code_block_reference".to_string(),
                                location: format!("line_{}", line_num + 1),
                                snippet: Some(format!("```{}", caps.get(1).map_or("", |m| m.as_str()))),
                            }])
                            .with_metadata(ChunkRelationshipMetadata {
                                extraction_method: "code_block_reference".to_string(),
                                source_chunk_type: "document".to_string(),
                                target_chunk_type: "code".to_string(),
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
