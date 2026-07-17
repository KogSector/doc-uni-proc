//! Advanced document parser (invokes python pipeline)

use crate::core::Result;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineOutput {
    pub title: String,
    #[serde(default)]
    pub sections: Vec<PipelineSection>,
    #[serde(default)]
    pub tables: Vec<PipelineTable>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineSection {
    #[serde(default)]
    pub heading: String,
    #[serde(default = "default_level")]
    pub level: u8,
    #[serde(default)]
    pub content: String,
}

fn default_level() -> u8 { 1 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineTable {
    #[serde(default)]
    pub caption: String,
    #[serde(default)]
    pub markdown: String,
}

pub struct DocumentParser {
    _python_processor_enabled: bool,
}

impl DocumentParser {
    pub fn new(python_processor_enabled: bool) -> Result<Self> {
        Ok(Self {
            _python_processor_enabled: python_processor_enabled,
        })
    }



    /// Process a document file through the python pipeline and return structured data
    pub async fn process_document_file(&self, file_path: &str) -> Result<PipelineOutput> {
        let content = tokio::fs::read_to_string(file_path).await.unwrap_or_else(|_| "Binary or unreadable file".to_string());
        let title = std::path::Path::new(file_path).file_stem().and_then(|s| s.to_str()).unwrap_or("Document").to_string();

        let parsed = PipelineOutput {
            title,
            sections: vec![PipelineSection {
                heading: "Content".to_string(),
                level: 1,
                content,
            }],
            tables: vec![],
            metadata: std::collections::HashMap::new(),
        };

        Ok(parsed)
    }

    /// Detect document type by extension
    pub fn detect_document_type(&self, filename: &str) -> String {
        let extension = Path::new(filename)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_lowercase();

        match extension.as_str() {
            "pdf" => "pdf".to_string(),
            "docx" => "docx".to_string(),
            "doc" => "doc".to_string(),
            "html" | "htm" => "html".to_string(),
            "md" | "markdown" => "markdown".to_string(),
            "txt" => "text".to_string(),
            "rtf" => "rtf".to_string(),
            _ => "unknown".to_string(),
        }
    }
}

const MAX_SECTION_CHUNK_CHARS: usize = 3000;
const MAX_FULL_DOC_CHARS: usize = 100_000;

pub fn build_document_chunks(
    parsed: &PipelineOutput,
    filename: &str,
    source_id: &str,
) -> Vec<crate::core::chunking::Chunk> {
    use crate::core::chunking::{Chunk, ChunkType, ChunkLevel, DocumentSemanticType};

    let ext = std::path::Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let mut chunks: Vec<Chunk> = Vec::new();

    // 1. Document overview chunk
    let heading_outline: Vec<String> = parsed.sections.iter()
        .filter(|s| !s.heading.is_empty())
        .map(|s| format!("{} {}", "#".repeat(s.level as usize), s.heading))
        .collect();

    let _page_count = parsed.metadata.get("page_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let _word_count = parsed.metadata.get("word_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let overview_text = format!(
        "# {}\n\n## Document Structure\n{}\n\n---\n\n",
        parsed.title,
        heading_outline.join("\n"),
    );

    // Track whether we've prepended the overview metadata to the first chunk
    let mut overview_prepended = false;

    // 1. Section chunks
    for section in &parsed.sections {
        if section.content.trim().is_empty() {
            continue;
        }

        let mut section_header = if section.heading.is_empty() {
            String::new()
        } else {
            format!("{} {}\n\n", "#".repeat(section.level as usize), section.heading)
        };
        
        if !overview_prepended {
            section_header = format!("{}{}", overview_text, section_header);
            overview_prepended = true;
        }

        // If the section is small enough, keep it as one chunk
        if section.content.len() <= MAX_SECTION_CHUNK_CHARS {
            let content = format!("{}{}", section_header, section.content);
            let chunk = Chunk::new(
                source_id.to_string(),
                filename.to_string(),
                content,
                ChunkType::Document {
                    format: ext.clone(),
                    semantic_type: DocumentSemanticType::Section,
                },
                ChunkLevel::Structural,
            ).with_confidence(0.95);
            chunks.push(chunk);
        } else {
            // Split long sections with an algorithm that balances chunk sizes
            fn chunk_text(text: &str, max_len: usize) -> Vec<String> {
                let text_len = text.len();
                if text_len <= max_len {
                    return vec![text.to_string()];
                }
                
                // Calculate ideal chunk size to ensure chunks are evenly sized
                let num_chunks = std::cmp::max(1, (text_len + max_len - 1) / max_len);
                let target_len = text_len / num_chunks;
                
                // Add a small tolerance so we don't split unnaturally
                let split_threshold = std::cmp::min(target_len + (target_len as f64 * 0.15) as usize, max_len);
                
                let mut results = Vec::new();
                let mut current = String::new();
                
                for p in text.split("\n\n") {
                    if p.len() <= max_len {
                        let expected_len = current.len() + p.len() + 2;
                        if expected_len > split_threshold && !current.is_empty() {
                            results.push(current.clone());
                            current = p.to_string();
                        } else {
                            if !current.is_empty() { current.push_str("\n\n"); }
                            current.push_str(p);
                        }
                    } else {
                        // For paragraphs larger than max_len, recursively split by '\n' and then chars
                        for l in p.split('\n') {
                            if l.len() <= max_len {
                                let expected_len = current.len() + l.len() + 1;
                                if expected_len > split_threshold && !current.is_empty() {
                                    results.push(current.clone());
                                    current = l.to_string();
                                } else {
                                    if !current.is_empty() { current.push('\n'); }
                                    current.push_str(l);
                                }
                            } else {
                                let mut chars = l.chars().peekable();
                                while chars.peek().is_some() {
                                    let chunk_part: String = chars.by_ref().take(max_len).collect();
                                    if current.len() + chunk_part.len() > split_threshold && !current.is_empty() {
                                        results.push(current.clone());
                                        current = chunk_part;
                                    } else {
                                        current.push_str(&chunk_part);
                                    }
                                }
                            }
                        }
                    }
                }
                if !current.is_empty() {
                    results.push(current);
                }
                results
            }

            let text_chunks = chunk_text(&section.content, MAX_SECTION_CHUNK_CHARS);
            
            for (i, t_chunk) in text_chunks.iter().enumerate() {
                let final_text = if i == 0 {
                    format!("{}{}", section_header, t_chunk)
                } else if !section.heading.is_empty() {
                    format!("[cont. {}]\n\n{}", section.heading, t_chunk)
                } else {
                    t_chunk.to_string()
                };
                
                let chunk = Chunk::new(
                    source_id.to_string(),
                    filename.to_string(),
                    final_text.trim().to_string(),
                    ChunkType::Document {
                        format: ext.clone(),
                        semantic_type: if i == 0 {
                            DocumentSemanticType::Section
                        } else {
                            DocumentSemanticType::Paragraph
                        },
                    },
                    ChunkLevel::Structural,
                ).with_confidence(0.90);
                chunks.push(chunk);
            }
        }
    }

    // 3. Table chunks
    for table in &parsed.tables {
        if table.markdown.trim().is_empty() {
            continue;
        }

        let content = if !table.caption.is_empty() {
            format!("Table: {}\n\n{}", table.caption, table.markdown)
        } else {
            table.markdown.clone()
        };

        let chunk = Chunk::new(
            source_id.to_string(),
            filename.to_string(),
            content,
            ChunkType::Document {
                format: ext.clone(),
                semantic_type: DocumentSemanticType::Table,
            },
            ChunkLevel::Micro,
        ).with_confidence(0.95);
        
        chunks.push(chunk);
    }

    // 4. Full-document chunk (store as large-context fallback)
    // Build a large contiguous representation of the document to provide
    // richer retrieval context. Truncate to MAX_FULL_DOC_CHARS to avoid
    // creating excessively large nodes.
    let mut full_text = String::new();
    for section in &parsed.sections {
        if !section.heading.is_empty() {
            full_text.push_str(&format!("{} {}\n\n", "#".repeat(section.level as usize), section.heading));
        }
        if !section.content.trim().is_empty() {
            full_text.push_str(&section.content);
            full_text.push_str("\n\n");
        }
    }

    // Append tables as text as well
    for table in &parsed.tables {
        if !table.markdown.trim().is_empty() {
            if !table.caption.is_empty() {
                full_text.push_str(&format!("Table: {}\n\n", table.caption));
            }
            full_text.push_str(&table.markdown);
            full_text.push_str("\n\n");
        }
    }

    if !full_text.trim().is_empty() {
        let truncated = if full_text.len() > MAX_FULL_DOC_CHARS {
            // Attempt to truncate at a paragraph boundary near the limit
            let mut t = full_text[..MAX_FULL_DOC_CHARS].to_string();
            if let Some(idx) = t.rfind("\n\n") {
                t.truncate(idx);
            }
            t
        } else {
            full_text
        };

        let mut full_chunk = Chunk::new(
            source_id.to_string(),
            filename.to_string(),
            truncated,
            ChunkType::Document {
                format: ext.clone(),
                semantic_type: DocumentSemanticType::DocumentOverview,
            },
            ChunkLevel::Overview,
        ).with_confidence(0.85);

        // Mark this chunk as the full document fallback
        full_chunk.metadata.custom.insert("full_document".to_string(), json!(true));
        chunks.push(full_chunk);
    }

    chunks
}
