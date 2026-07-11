use crate::core::chunking::Chunk;
use crate::graph::models::{ChunkRelationship, ChunkRelationType};
use crate::core::config::LlmConfig;
use std::collections::HashMap;

/// LLM-based semantic extractor for conceptual relationships
pub struct SemanticExtractor {
    config: LlmConfig,
    client: reqwest::Client,
}

#[derive(serde::Serialize)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    generation_config: GeminiGenerationConfig,
}

#[derive(serde::Serialize)]
struct GeminiGenerationConfig {
    temperature: f32,
    response_mime_type: String,
}

#[derive(serde::Serialize)]
struct GeminiContent {
    parts: Vec<GeminiPart>,
}

#[derive(serde::Serialize)]
struct GeminiPart {
    text: String,
}

#[derive(serde::Deserialize)]
struct GeminiResponse {
    candidates: Option<Vec<GeminiCandidate>>,
}

#[derive(serde::Deserialize)]
struct GeminiCandidate {
    content: GeminiMessageContent,
}

#[derive(serde::Deserialize)]
struct GeminiMessageContent {
    parts: Vec<GeminiResponsePart>,
}

#[derive(serde::Deserialize)]
struct GeminiResponsePart {
    text: String,
}

#[derive(serde::Deserialize, Debug)]
struct ExtractedRelationship {
    source_id: String,
    target_id: String,
    relation: String,
    confidence: f32,
    reasoning: String,
}

impl SemanticExtractor {
    pub fn new(config: LlmConfig) -> Self {
        Self { 
            config,
            client: reqwest::Client::new(),
        }
    }

    /// Extracts semantic relationships across chunks using the configured LLM
    pub async fn extract_semantic(&self, chunks: &[Chunk]) -> Vec<ChunkRelationship> {
        if chunks.is_empty() || chunks.len() == 1 {
            return Vec::new();
        }

        // Limit chunks per request to avoid exceeding context window or JSON parsing complexity
        let mut all_relationships = Vec::new();
        
        // Chunk into groups of up to 10
        for chunk_group in chunks.chunks(10) {
            let mut prompt = String::from("Analyze the following document chunks and identify semantic relationships between them. Valid relationship types are: SIMILAR_TO, CONTINUATION_OF, ELABORATES_ON, CONTRADICTS. Only output a JSON array of objects with fields: 'source_id', 'target_id', 'relation', 'confidence' (0.0 to 1.0), and 'reasoning'. If none, output [].\n\n");
            
            let mut id_map = HashMap::new();
            
            for c in chunk_group {
                let short_id = c.id.to_string()[0..8].to_string();
                id_map.insert(short_id.clone(), c.id);
                prompt.push_str(&format!("--- CHUNK ID: {} ---\n{}\n\n", short_id, c.content.chars().take(2000).collect::<String>()));
            }

            let request_body = GeminiRequest {
                contents: vec![GeminiContent {
                    parts: vec![GeminiPart { text: prompt }],
                }],
                generation_config: GeminiGenerationConfig {
                    temperature: 0.1,
                    response_mime_type: "application/json".to_string(),
                }
            };

            let url = format!("{}/v1beta/models/{}:generateContent?key={}", 
                self.config.base_url.trim_end_matches('/'),
                self.config.model,
                self.config.api_key
            );

            let res = match self.client.post(&url).json(&request_body).send().await {
                Ok(r) => r,
                Err(e) => {
                    tracing::error!("LLM request failed: {}", e);
                    continue;
                }
            };

            if let Ok(gemini_res) = res.json::<GeminiResponse>().await {
                if let Some(candidates) = gemini_res.candidates {
                    if let Some(candidate) = candidates.first() {
                        if let Some(part) = candidate.content.parts.first() {
                            let text = &part.text;
                            // Attempt to parse the JSON array
                            if let Ok(extracted) = serde_json::from_str::<Vec<ExtractedRelationship>>(text) {
                                for rel in extracted {
                                    if let (Some(&src_uuid), Some(&tgt_uuid)) = (id_map.get(&rel.source_id), id_map.get(&rel.target_id)) {
                                        if src_uuid != tgt_uuid {
                                            all_relationships.push(
                                                ChunkRelationship::new(
                                                    src_uuid,
                                                    tgt_uuid,
                                                    ChunkRelationType::Semantic(rel.relation),
                                                    rel.confidence,
                                                ).with_fact(rel.reasoning)
                                            );
                                        }
                                    }
                                }
                            } else {
                                tracing::warn!("Failed to parse LLM JSON output: {}", text);
                            }
                        }
                    }
                }
            } else {
                tracing::error!("Failed to parse Gemini response");
            }
        }
        
        all_relationships
    }
}
