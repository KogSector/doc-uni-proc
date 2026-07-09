use crate::core::chunking::Chunk;
use crate::graph::models::{ChunkRelationship, ChunkRelationType};
use crate::core::config::LlmConfig;
use regex::Regex;
use std::collections::{HashMap, HashSet};

/// Fast local mathematical extractor for conceptual and semantic relationships
pub struct SemanticExtractor {
    _config: LlmConfig, // Kept for compatibility, though unused now
}

impl SemanticExtractor {
    pub fn new(config: LlmConfig) -> Self {
        Self { _config: config }
    }

    /// Extracts semantic relationships across chunks using local TF-IDF and Lexical Overlap
    pub async fn extract_semantic(&self, chunks: &[Chunk]) -> Vec<ChunkRelationship> {
        if chunks.is_empty() {
            return Vec::new();
        }

        // We run the CPU-intensive math in a blocking task so we don't block the async executor
        let chunks_clone = chunks.to_vec();
        
        tokio::task::spawn_blocking(move || {
            let mut relationships = Vec::new();
            
            // 1. Entity (Identifier) Overlap
            let entity_relations = Self::extract_entity_overlap(&chunks_clone);
            relationships.extend(entity_relations);
            
            // 2. TF-IDF Cosine Similarity
            let semantic_relations = Self::extract_tfidf_similarity(&chunks_clone);
            relationships.extend(semantic_relations);
            
            relationships
        })
        .await
        .unwrap_or_default()
    }

    fn extract_entity_overlap(chunks: &[Chunk]) -> Vec<ChunkRelationship> {
        let mut relationships = Vec::new();
        
        // Regex for CamelCase, PascalCase, or snake_case identifiers (length >= 4)
        // This heuristic finds function names, class names, constants, etc.
        let re = Regex::new(r"\b([A-Z][a-zA-Z0-9]{3,}|[a-z]{2,}(?:_[a-z0-9]+)+|[a-z]+(?:[A-Z][a-z0-9]+)+)\b").unwrap();
        
        let mut chunk_entities: HashMap<uuid::Uuid, HashSet<String>> = HashMap::new();
        
        for chunk in chunks {
            let mut entities = HashSet::new();
            for cap in re.captures_iter(&chunk.content) {
                if let Some(m) = cap.get(1) {
                    entities.insert(m.as_str().to_string());
                }
            }
            chunk_entities.insert(chunk.id, entities);
        }
        
        // Compare all pairs
        for i in 0..chunks.len() {
            for j in (i + 1)..chunks.len() {
                let c1 = &chunks[i];
                let c2 = &chunks[j];
                
                let e1 = chunk_entities.get(&c1.id).unwrap();
                let e2 = chunk_entities.get(&c2.id).unwrap();
                
                let intersection_count = e1.intersection(e2).count();
                
                if intersection_count >= 2 { // Heuristic: sharing at least 2 complex identifiers
                    // Determine directionality based on who defines vs who uses, but a simple SHARES_CONCEPT is safer
                    relationships.push(
                        ChunkRelationship::new(
                            c1.id,
                            c2.id,
                            ChunkRelationType::Semantic("SHARES_CONCEPT".to_string()),
                            0.7 + (0.01 * intersection_count.min(30) as f32), // Confidence bounded by 1.0
                        ).with_fact(format!("Shares {} complex identifiers", intersection_count))
                    );
                }
            }
        }
        
        relationships
    }

    fn extract_tfidf_similarity(chunks: &[Chunk]) -> Vec<ChunkRelationship> {
        let mut relationships = Vec::new();
        let num_docs = chunks.len() as f32;
        
        // Basic word tokenizer (alphanumeric >= 3 chars)
        let token_re = Regex::new(r"\b[a-zA-Z0-9]{3,}\b").unwrap();
        
        let mut doc_tokens = Vec::with_capacity(chunks.len());
        let mut df: HashMap<String, f32> = HashMap::new();
        
        for chunk in chunks {
            let mut tf: HashMap<String, f32> = HashMap::new();
            let mut unique_in_doc = HashSet::new();
            
            for cap in token_re.captures_iter(&chunk.content.to_lowercase()) {
                if let Some(m) = cap.get(0) {
                    let word = m.as_str().to_string();
                    *tf.entry(word.clone()).or_insert(0.0) += 1.0;
                    unique_in_doc.insert(word);
                }
            }
            
            for word in unique_in_doc {
                *df.entry(word).or_insert(0.0) += 1.0;
            }
            
            doc_tokens.push(tf);
        }
        
        // Calculate TF-IDF vectors
        let mut tfidf_vectors = Vec::with_capacity(chunks.len());
        for tf in doc_tokens {
            let mut vec: HashMap<String, f32> = HashMap::new();
            let mut norm_sq = 0.0;
            
            for (word, count) in tf {
                let doc_freq = df.get(&word).unwrap_or(&1.0);
                let idf = (num_docs / doc_freq).ln() + 1.0;
                let val = count * idf;
                vec.insert(word, val);
                norm_sq += val * val;
            }
            
            let norm = norm_sq.sqrt().max(1e-10);
            
            // Normalize
            for val in vec.values_mut() {
                *val /= norm;
            }
            
            tfidf_vectors.push(vec);
        }
        
        // Calculate pairwise cosine similarity
        for i in 0..chunks.len() {
            for j in (i + 1)..chunks.len() {
                let mut dot_product = 0.0;
                let v1 = &tfidf_vectors[i];
                let v2 = &tfidf_vectors[j];
                
                // Iterate over the smaller vector for speed
                let (smaller, larger) = if v1.len() < v2.len() { (v1, v2) } else { (v2, v1) };
                
                for (word, val1) in smaller {
                    if let Some(val2) = larger.get(word) {
                        dot_product += val1 * val2;
                    }
                }
                
                // Threshold for semantic similarity
                if dot_product > 0.35 {
                    relationships.push(
                        ChunkRelationship::new(
                            chunks[i].id,
                            chunks[j].id,
                            ChunkRelationType::Semantic("SIMILAR_TO".to_string()),
                            dot_product,
                        ).with_fact(format!("TF-IDF Cosine Similarity of {:.2}", dot_product))
                    );
                }
            }
        }
        
        relationships
    }
}

