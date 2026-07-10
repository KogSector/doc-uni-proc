//! Main orchestration logic for the unified processor
//!
//! gRPC-based processing pipeline:
//! 1. Receive from: data-connector via gRPC calls
//! 2. Process: clone repo / download doc → AST analysis → intelligent chunking
//! 3. Store: chunks in local database
//! 4. Cleanup: delete temp content after processing

use crate::core::{Config, Result, ProcessorError};
use crate::processors::documents::DocumentParser;
use crate::processors::web::{WebScraper, CrawlConfig, WebPageData};

use crate::infra::storage::{PostgresStorage, GraphSync};
use crate::graph::extractors::{SourceRelationshipRouter, SemanticExtractor};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, LazyLock};
use uuid::Uuid;

// ─── Data types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessedData {
    pub id: Uuid,
    pub source_id: String,
    pub filename: String,
    pub content_type: ContentType,
    pub data: ProcessingResult,
    pub metadata: ProcessingMetadata,
    pub chunks: Vec<ChunkData>,
    pub processing_stage: String,
}

/// Individual chunk produced by the chunking pipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkData {
    pub chunk_id: String,
    pub content: String,
    pub start_line: i32,
    pub end_line: i32,
    pub chunk_type: String,
    pub quality_score: f32,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentType {
    Document(DocumentData),
    Web(WebPageData),
}



#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentData {
    pub text_content: String,
    pub sections: Vec<DocumentSection>,
    pub tables: Vec<DocumentTable>,
    pub figures: Vec<DocumentFigure>,
    pub processor: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeData {
    pub language: String,
    pub functions: Vec<CodeFunction>,
    pub classes: Vec<CodeClass>,
    pub imports: Vec<String>,
    pub metrics: CodeMetrics,
    pub ast_summary: AstSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingResult {
    pub success: bool,
    pub processing_time_ms: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingMetadata {
    pub file_size: usize,
    pub line_count: usize,
    pub processed_at: String,
    pub embedding_model: Option<String>,
    pub embedding_generated: bool,
}

// Document-related types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentSection {
    pub title: String,
    pub content: String,
    pub level: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentTable {
    pub index: usize,
    pub content: String,
    pub caption: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentFigure {
    pub index: usize,
    pub caption: Option<String>,
    pub figure_type: String,
}

// Code-related types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeFunction {
    pub name: String,
    pub line_number: usize,
    pub parameters: Vec<String>,
    pub return_type: Option<String>,
    pub complexity: u32,
    pub docstring: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeClass {
    pub name: String,
    pub line_number: usize,
    pub methods: Vec<CodeFunction>,
    pub properties: Vec<String>,
    pub inheritance: Vec<String>,
    pub docstring: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeMetrics {
    pub lines_of_code: usize,
    pub lines_of_comments: usize,
    pub cyclomatic_complexity: u32,
    pub cognitive_complexity: u32,
    pub maintainability_index: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AstSummary {
    pub total_nodes: usize,
    pub max_depth: usize,
    pub node_types: std::collections::HashMap<String, usize>,
}

// ─── Kafka message types ────────────────────────────────────────────────────



/// Message consumed from `doc-processing.requests`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocProcessingRequest {
    pub request_id: String,
    pub user_id: String,
    pub document_id: String,
    pub filename: String,
    pub content: String, // base64-encoded document
    pub content_type: String, // MIME type
    pub metadata: HashMap<String, String>,
}

/// Message consumed from `repo-updates.requests`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoUpdateRequest {
    pub update_id: String,
    pub repo_id: String,
    pub repo_url: String,
    pub branch: String,
    pub credentials: Option<String>,
    pub from_commit: String,
    pub to_commit: String,
    pub provider: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebProcessingRequest {
    pub request_id: String,
    pub user_id: String,
    pub url: String,
    pub crawl: bool,
    pub max_pages: Option<usize>,
    pub max_depth: Option<usize>,
    pub crawl_delay_ms: Option<u64>,
    pub include_css: Option<bool>,
    pub include_js: Option<bool>,
    pub metadata: HashMap<String, String>,
}



// ─── Orchestrator ───────────────────────────────────────────────────────────



#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebProcessingResult {
    pub request_id: String,
    pub url: String,
    pub pages_processed: usize,
    pub total_chunks: usize,
    pub processing_time_ms: u64,
}

pub struct UnifiedProcessor {
    config: Config,
    pub document_parser: DocumentParser,
    pub web_scraper: WebScraper,
    pub postgres_storage: Arc<PostgresStorage>,
    graph_sync: Arc<GraphSync>,
    chunker: crate::core::chunking::HybridChunker,
    /// Source-agnostic structural relationship router (replaces UnifiedRelationshipExtractor)
    relationship_router: SourceRelationshipRouter,
    semantic_extractor: SemanticExtractor,


    pub falkordb_storage: Arc<crate::infra::storage::FalkordbStorage>,
    /// In-memory cache: chunk_id → content, populated before Kafka publish,
    /// consumed by the embedding consumer to avoid PostgreSQL dependency.
    pub chunk_content_cache: Arc<Mutex<HashMap<String, String>>>,
}

impl UnifiedProcessor {

    pub async fn new(
        config: Config,
        falkordb_storage: Arc<crate::infra::storage::FalkordbStorage>,
    ) -> Result<Self> {
        // Initialize components
        let document_parser = DocumentParser::new(true)?;
        let default_crawl_config = CrawlConfig::default();
        let web_scraper = WebScraper::new(&default_crawl_config)?;
        
        let postgres_storage = Arc::new(
            PostgresStorage::new(&config.database.postgres_url).await?
        );
        
        let graph_sync = Arc::new(GraphSync::new(falkordb_storage.clone()));
        
        // Initialize advanced chunking system
        // Initialize advanced chunking system
        let chunker = crate::core::chunking::HybridChunker::new();
        

        let relationship_router = SourceRelationshipRouter::new();
        let semantic_extractor = SemanticExtractor::new(config.llm.clone());




        Ok(Self {
            config,
            document_parser,
            web_scraper,
            postgres_storage,
            graph_sync,
            chunker,

            relationship_router,
            semantic_extractor,

            falkordb_storage,
            chunk_content_cache: Arc::new(Mutex::new(HashMap::new())),
        })
    }
    

    
    /// Handle a document processing request from gRPC
    /// Flow: download doc → structure analysis → chunk → store chunks → cleanup
    pub async fn handle_web_processing(&self, request: WebProcessingRequest) -> Result<WebProcessingResult> {
        let start = std::time::Instant::now();

        tracing::info!(
            request_id = %request.request_id,
            url = %request.url,
            crawl = request.crawl,
            "Starting web processing"
        );

        if !self.config.web.enabled {
            return Err(ProcessorError::InfraError("Web scraping is disabled".to_string()));
        }

        // Build CrawlConfig from request overrides + global defaults
        let crawl_config = CrawlConfig {
            max_pages: request.max_pages.unwrap_or(self.config.web.max_pages),
            max_depth: request.max_depth.unwrap_or(self.config.web.max_depth),
            crawl_delay_ms: request.crawl_delay_ms.unwrap_or(self.config.web.crawl_delay_ms),
            include_css: request.include_css.unwrap_or(true),
            include_js: request.include_js.unwrap_or(true),
            user_agent: self.config.web.user_agent.clone(),
            request_timeout_secs: self.config.web.request_timeout_secs,
            ..CrawlConfig::default()
        };

        // Scrape or crawl
        let pages: Vec<WebPageData> = if request.crawl {
            let site_ctx = self.web_scraper
                .crawl_website(&request.url, &crawl_config, &request.request_id)
                .await
                .map_err(|e| ProcessorError::InfraError(format!("Website crawl failed: {}", e)))?;
            site_ctx.pages
        } else {
            let page = self.web_scraper
                .scrape_url(&request.url, &crawl_config)
                .await
                .map_err(|e| ProcessorError::InfraError(format!("URL scrape failed: {}", e)))?;
            vec![page]
        };

        // Process each page through the chunking → embedding → storage pipeline
        let total_pages = pages.len();
        let mut total_chunks = 0usize;
        let source_id = format!("web:{}", request.url);

        for (page_idx, page) in pages.iter().enumerate() {
            match self.process_web_page(page, &source_id, &request.request_id, page_idx, &request.user_id).await {
                Ok(chunk_count) => {
                    total_chunks += chunk_count;
                    tracing::info!(
                        request_id = %request.request_id,
                        page_idx,
                        url = %page.url,
                        chunks = chunk_count,
                        "Web page processed"
                    );
                }
                Err(e) => {
                    tracing::error!(
                        request_id = %request.request_id,
                        url = %page.url,
                        error = %e,
                        "Failed to process web page, continuing"
                    );
                }
            }
        }

        // Trigger graph sync
        if let Err(e) = self.trigger_graph_sync(&source_id, &request.user_id).await {
            tracing::warn!("Failed to trigger graph sync for {}: {}", source_id, e);
        }

        let elapsed_ms = start.elapsed().as_millis() as u64;

        tracing::info!(
            request_id = %request.request_id,
            url = %request.url,
            total_pages,
            total_chunks,
            elapsed_ms,
            "Web processing completed"
        );

        Ok(WebProcessingResult {
            request_id: request.request_id.clone(),
            url: request.url.clone(),
            pages_processed: total_pages,
            total_chunks,
            processing_time_ms: elapsed_ms,
        })
    }

    /// Get chunk content by chunk ID.
    /// First checks the in-memory cache, then falls back to FalkorDB.
    pub async fn get_chunk_content(&self, chunk_id: &str, user_id: &str) -> Result<String> {
        // Check in-memory cache first
        {
            let cache = self.chunk_content_cache.lock().unwrap_or_else(|p| p.into_inner());
            if let Some(content) = cache.get(chunk_id) {
                return Ok(content.clone());
            }
        }
        
        tracing::warn!("Chunk not found in in-memory cache, falling back to FalkorDB: {}", chunk_id);
        let user_graph = self.falkordb_storage.with_user_graph(user_id);
        
        match user_graph.get_chunk_content(chunk_id).await {
            Ok(Some(content)) => Ok(content),
            Ok(None) => Err(ProcessorError::NotFound(format!("Chunk not found in FalkorDB: {}", chunk_id))),
            Err(e) => Err(ProcessorError::DatabaseError(format!("Failed to get chunk from FalkorDB: {}", e))),
        }
    }


    ///   - Table chunks (if present)
    async fn process_web_page(
        &self,
        page: &WebPageData,
        source_id: &str,
        _request_id: &str,
        _page_idx: usize,
        user_id: &str,
    ) -> Result<usize> {
        use crate::core::chunking::{Chunk, ChunkType, ChunkLevel, WebSemanticType};

        let mut all_chunks: Vec<Chunk> = Vec::new();

        // 1. Page overview chunk (title + description + heading outline)
        let heading_outline = page.headings.iter()
            .map(|h| format!("{} {}", "#".repeat(h.level as usize), h.text))
            .collect::<Vec<_>>()
            .join("\n");

        let overview = format!(
            "# {}\n\nURL: {}\nDomain: {}\n{}\n\n## Heading Structure\n{}",
            page.title,
            page.url,
            page.domain,
            page.description.as_deref().unwrap_or(""),
            heading_outline
        );

        all_chunks.push(Chunk::new(
            source_id.to_string(),
            page.url.clone(),
            overview,
            ChunkType::Web {
                url: page.url.clone(),
                semantic_type: WebSemanticType::PageOverview,
            },
            ChunkLevel::Overview,
        ).with_confidence(0.95));

        // 2. Metadata chunk (Open Graph, JSON-LD, keywords)
        if !page.metadata.og_properties.is_empty()
            || !page.structured_data.is_empty()
            || !page.metadata.keywords.is_empty()
        {
            let meta_content = serde_json::json!({
                "og": page.metadata.og_properties,
                "keywords": page.metadata.keywords,
                "structured_data": page.structured_data,
                "language": page.metadata.language,
                "author": page.metadata.author,
            }).to_string();

            all_chunks.push(Chunk::new(
                source_id.to_string(),
                page.url.clone(),
                meta_content,
                ChunkType::Web {
                    url: page.url.clone(),
                    semantic_type: WebSemanticType::Metadata,
                },
                ChunkLevel::Overview,
            ).with_confidence(0.9));
        }

        // 3. Main content — chunk via the standard pipeline (paragraph splitting)
        if !page.main_content.is_empty() {
            let content_chunks = self.generate_web_content_chunks(
                &page.main_content,
                &page.url,
                source_id,
            ).await?;
            all_chunks.extend(content_chunks);
        }

        // 4. Table chunks
        for table in &page.tables {
            let table_text = format_table_as_text(table);
            if !table_text.trim().is_empty() {
                all_chunks.push(Chunk::new(
                    source_id.to_string(),
                    page.url.clone(),
                    table_text,
                    ChunkType::Web {
                        url: page.url.clone(),
                        semantic_type: WebSemanticType::Table,
                    },
                    ChunkLevel::Micro,
                ).with_confidence(0.85));
            }
        }

        // 5. CSS chunks (only if content was captured)
        for css in &page.css_resources {
            if css.is_inline && !css.content.trim().is_empty() {
                all_chunks.push(Chunk::new(
                    source_id.to_string(),
                    css.source_url.clone().unwrap_or_else(|| page.url.clone()),
                    css.content.clone(),
                    ChunkType::Web {
                        url: page.url.clone(),
                        semantic_type: WebSemanticType::StyleSheet,
                    },
                    ChunkLevel::Semantic,
                ).with_confidence(0.7));
            }
        }

        let chunk_count = all_chunks.len();

        // Send through store + publish pipeline
        if let Err(e) = self.store_and_publish_chunks(all_chunks, source_id, None, user_id).await {
            tracing::error!(
                url = %page.url,
                error = %e,
                "Failed to store and publish web page chunks"
            );
        }

        Ok(chunk_count)
    }

    /// Generate content chunks from web page text.
    ///
    /// Uses the standard HybridChunker but with Web-typed fallback.
    async fn generate_web_content_chunks(
        &self,
        content: &str,
        url: &str,
        source_id: &str,
    ) -> Result<Vec<crate::core::chunking::Chunk>> {
        use crate::core::chunking::{ChunkType, WebSemanticType};
        use crate::core::chunking::ChunkingStrategy;

        let config = crate::core::chunking::ChunkingConfig::from_env();

        let mut result = self.chunker.process(content, url, source_id, &config)
            .await
            .map_err(|e| ProcessorError::InfraError(format!("Chunking failed: {}", e)))?;
            
        // Re-tag chunks as Web type (the HybridChunker may assign Document type)
        for chunk in &mut result.chunks {
            chunk.chunk_type = ChunkType::Web {
                url: url.to_string(),
                semantic_type: WebSemanticType::Paragraph,
            };
        }
        Ok(result.chunks)
    }

    // ─── Legacy file processing (kept for gRPC health checks & direct calls) ──
    
    /// Process a single file (used by gRPC endpoint for backward compatibility)
    pub async fn process_file(&self, content: &str, is_base64: bool, filename: &str, source_id: &str, repo_name: &str, user_id: &str) -> Result<ProcessedData> {
        let start_time = std::time::Instant::now();
        let file_id = Uuid::new_v4();
        
        let content_type = self.detect_content_type(filename);
        
        let (processing_result, mut chunks) = match content_type {
            ContentType::Document(_) => {
                let (res, doc_chunks) = self.process_document(content, is_base64, filename, source_id).await;
                tracing::info!("Finished process_document for {} with {} chunks", filename, doc_chunks.len());
                (res, doc_chunks)
            },
            ContentType::Web(_) => return Err(ProcessorError::UnsupportedFileType("Web processing not supported via process_file".to_string())),
        };
        
        // --- NEW LOGIC: Diff against snapshot ---
        let mut chunks_to_delete_from_db = Vec::new();
        if let Ok(snapshots) = self.postgres_storage.get_chunk_snapshots(source_id, filename).await {
            let mut snapshot_map = std::collections::HashMap::new();
            for s in snapshots {
                snapshot_map.insert(s.chunk_key.clone(), s);
            }
            
            let mut new_keys = std::collections::HashSet::new();
            for chunk in chunks.iter_mut() {
                new_keys.insert(chunk.chunk_key.clone());
                if let Some(snapshot) = snapshot_map.get(&chunk.chunk_key) {
                    if chunk.chunk_hash == snapshot.chunk_hash {
                        chunk.is_dirty = false;
                        tracing::debug!("Chunk {} is clean (hash match)", chunk.chunk_key);
                    } else {
                        tracing::debug!("Chunk {} is dirty (hash mismatch)", chunk.chunk_key);
                    }
                } else {
                    tracing::debug!("Chunk {} is new", chunk.chunk_key);
                }
            }
            
            // Find deleted chunks
            for (old_key, _) in snapshot_map {
                if !new_keys.contains(&old_key) {
                    tracing::debug!("Chunk {} was deleted", old_key);
                    chunks_to_delete_from_db.push(format!("{}|{}", old_key, filename));
                    // Mark as tombstone in postgres
                    let _ = self.postgres_storage.mark_chunks_deleted(source_id, &[old_key]).await;
                }
            }
        }
        
        if !chunks_to_delete_from_db.is_empty() {
            tracing::info!("Deleting {} orphaned chunks from FalkorDB", chunks_to_delete_from_db.len());
            let user_graph = self.falkordb_storage.with_user_graph(user_id);
            let keys_str = chunks_to_delete_from_db.iter()
                .map(|k| format!("'{}'", k))
                .collect::<Vec<_>>()
                .join(",");
            let query = format!(
                "MATCH (c:Vector_Chunk) WHERE c.id IN [{}] DETACH DELETE c",
                keys_str
            );
            if let Err(e) = user_graph.execute_query(&query).await {
                tracing::error!("Failed to delete orphaned chunks: {}", e);
            }
        }
        // ----------------------------------------
        
        // Send chunks to falkordb & publish to embeddings-service
        if let Err(e) = self.store_and_publish_chunks(chunks.clone(), source_id, Some(repo_name.to_string()), user_id).await {
            tracing::error!("Failed to store and publish chunks for {}: {}", filename, e);
        }

        let chunk_data: Vec<ChunkData> = chunks.iter().enumerate().map(|(_idx, chunk)| {
            let (start_line, end_line) = chunk.metadata.line_range.unwrap_or((0, 0));
            let chunk_type_str = match &chunk.chunk_type {
                crate::core::chunking::ChunkType::Code { language: _, semantic_type } => format!("code:{:?}", semantic_type),
                crate::core::chunking::ChunkType::Document { format: _, semantic_type } => format!("doc:{:?}", semantic_type),
                crate::core::chunking::ChunkType::Web { url: _, semantic_type } => format!("web:{:?}", semantic_type),
                crate::core::chunking::ChunkType::Mixed => "mixed".to_string(),
            };
            ChunkData {
                chunk_id: chunk.chunk_key.clone(),
                content: chunk.content.clone(),
                start_line: start_line as i32,
                end_line: end_line as i32,
                chunk_type: chunk_type_str,
                quality_score: chunk.confidence,
                metadata: HashMap::new(),
            }
        }).collect();
        
        // Store chunks locally and publish to Kafka
        let processing_stage = "chunks_generated".to_string();
        let processing_time = start_time.elapsed().as_millis() as u64;
        
        let metadata = ProcessingMetadata {
            file_size: content.len(),
            line_count: content.lines().count(),
            processed_at: chrono::Utc::now().to_rfc3339(),
            embedding_model: None,
            embedding_generated: false,
        };
        
        let processed_data = ProcessedData {
            id: file_id,
            source_id: source_id.to_string(),
            filename: filename.to_string(),
            content_type: content_type.clone(),
            data: ProcessingResult {
                success: processing_result.success,
                processing_time_ms: processing_time,
                error: processing_result.error,
            },
            metadata,
            chunks: chunk_data,
            processing_stage,
        };
        
        // Store file metadata in PostgreSQL
        self.store_file_metadata(&processed_data, content, user_id).await?;
        
        Ok(processed_data)
    }
    
    
    // ─── Internal helpers ───────────────────────────────────────────────
    
    /// Generate chunks from file content
    pub(crate) async fn generate_chunks(
        &self,
        content: &str,
        filename: &str,
        source_id: &str,
    ) -> Result<Vec<crate::core::chunking::Chunk>> {
        use crate::core::chunking::ChunkingStrategy;
        
        let config = crate::core::chunking::ChunkingConfig::from_env();
        
        // Use the HybridChunker (which implements ChunkingStrategy)
        let result = self.chunker.process(content, filename, source_id, &config)
            .await
            .map_err(|e| ProcessorError::InfraError(format!("Chunking failed: {}", e)))?;
            
        Ok(result.chunks)
    }

    async fn process_document(&self, content: &str, is_base64: bool, filename: &str, source_id: &str) -> (ProcessingResult, Vec<crate::core::chunking::Chunk>) {
        // Write content to a temp file since process_document_file requires a file path
        let temp_dir = std::env::temp_dir();
        // Use a UUID to avoid path traversal or missing directory errors if filename contains slashes
        let ext = std::path::Path::new(filename).extension().and_then(|e| e.to_str()).unwrap_or("tmp");
        let safe_filename = format!("{}.{}", uuid::Uuid::new_v4(), ext);
        let file_path = temp_dir.join(safe_filename);
        
        let write_result = if is_base64 {
            use base64::{Engine as _, engine::general_purpose::STANDARD};
            match STANDARD.decode(content) {
                Ok(bytes) => std::fs::write(&file_path, bytes),
                Err(e) => {
                    return (ProcessingResult {
                        success: false,
                        processing_time_ms: 0,
                        error: Some(format!("Failed to decode base64 content: {}", e)),
                    }, vec![]);
                }
            }
        } else {
            std::fs::write(&file_path, content)
        };
        
        if let Err(e) = write_result {
            return (ProcessingResult {
                success: false,
                processing_time_ms: 0,
                error: Some(format!("Failed to write temp file: {}", e)),
            }, vec![]);
        }

        let result = match self.document_parser.process_document_file(file_path.to_str().unwrap()).await {
            Ok(document_data) => {
                let chunks = crate::processors::documents::parser::build_document_chunks(&document_data, filename, source_id);
                (ProcessingResult {
                    success: true,
                    processing_time_ms: 0,
                    error: None,
                }, chunks)
            }
            Err(e) => {
                tracing::error!("process_document_file failed: {}", e);
                (ProcessingResult {
                    success: false,
                    processing_time_ms: 0,
                    error: Some(e.to_string()),
                }, vec![])
            },
        };

        // Clean up temp file
        let _ = std::fs::remove_file(file_path);

        result
    }



    async fn store_file_metadata(&self, data: &ProcessedData, _content: &str, user_id: &str) -> Result<()> {
        let file_type = match &data.content_type {
            ContentType::Document(_) => "document",
            ContentType::Web(_) => "web",
        };
        
        let language = match &data.content_type {
            ContentType::Document(_) => None,
            ContentType::Web(_) => None,
        };
        
        let pg_metadata = crate::infra::storage::FileMetadata {
            id: data.id,
            source_id: data.source_id.clone(),
            filename: data.filename.clone(),
            file_type: file_type.to_string(),
            language,
            size_bytes: Some(data.metadata.file_size as i32),
            line_count: Some(data.metadata.line_count as i32),
            processed_at: chrono::DateTime::parse_from_rfc3339(&data.metadata.processed_at)
                .map_err(|e| ProcessorError::SerializationError(e.to_string()))?
                .with_timezone(&chrono::Utc),
            user_id: user_id.to_string(),
        };
        self.postgres_storage.store_file_metadata(pg_metadata, user_id).await?;
        
        Ok(())
    }

    /// Delete a file's chunks from FalkorDB when a deletion event is received.
    pub async fn delete_file(&self, file_path: &str, source_id: &str, user_id: &str) -> Result<()> {
        let start_time = std::time::Instant::now();
        tracing::info!(
            "Deleting file from FalkorDB: filename={}, source_id={}, user_graph=graph-{}",
            file_path, source_id, user_id
        );
        
        let user_graph = self.falkordb_storage.with_user_graph(user_id);
        let query = format!(
            "MATCH (c:Vector_Chunk) WHERE c.source_id = '{}' AND c.id ENDS WITH '|{}' DETACH DELETE c",
            source_id, file_path
        );
        
        match user_graph.execute_query(&query).await {
            Ok(_) => {
                tracing::info!(
                    "Successfully deleted file chunks in {}ms",
                    start_time.elapsed().as_millis()
                );
            },
            Err(e) => {
                tracing::error!("Failed to delete file chunks: {}", e);
                return Err(ProcessorError::DatabaseError(format!("Failed to delete chunks: {}", e)));
            }
        }
        
        // Also delete file metadata from PostgreSQL
        if let Err(e) = self.postgres_storage.delete_file_metadata(source_id, file_path).await {
            tracing::error!("Failed to delete file metadata: {}", e);
        }
        
        Ok(())
    }

    pub async fn trigger_graph_sync(&self, source_id: &str, user_id: &str) -> Result<()> {
        self.graph_sync.trigger_relationship_building(source_id, user_id).await?;
        Ok(())
    }

    fn detect_content_type(&self, filename: &str) -> ContentType {
        // DSA: O(1) HashSet lookup replaces O(n) linear array scan.
        // LazyLock ensures the set is built exactly once across all calls.
        static CODE_EXTENSIONS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
            [
                "rs", "py", "js", "jsx", "ts", "tsx", "go", "java",
                "c", "cpp", "cxx", "cc", "h", "hpp", "cs", "rb",
                "kt", "swift", "scala", "lua", "dart", "r",
                "html", "htm", "xhtml", "css", "scss", "less", "vue", "svelte",
                "yaml", "yml", "json", "toml", "xml",
            ]
            .into_iter()
            .collect()
        });

        let extension = std::path::Path::new(filename)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_lowercase();

        static DOCUMENT_EXTENSIONS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
            [
                "pdf", "docx", "doc", "pptx", "ppt", "rtf", "epub", "md", "markdown", "txt"
            ]
            .into_iter()
            .collect()
        });

        if DOCUMENT_EXTENSIONS.contains(extension.as_str()) {
            return ContentType::Document(DocumentData {
                text_content: String::new(),
                sections: Vec::new(),
                tables: Vec::new(),
                figures: Vec::new(),
                processor: "unknown".to_string(),
            });
        }

        ContentType::Document(DocumentData {
            text_content: String::new(),
            sections: Vec::new(),
            tables: Vec::new(),
            figures: Vec::new(),
            processor: "unknown".to_string(),
        })
    }

    pub async fn get_processing_status(&self, source_id: &str, user_id: &str) -> Result<ProcessingStatus> {
        let files = self.postgres_storage.list_files_by_source(source_id, user_id).await?;
        let graph_status = self.graph_sync.get_graph_status(source_id).await.ok();
        
        Ok(ProcessingStatus {
            source_id: source_id.to_string(),
            total_files: files.len(),
            processed_files: files.len(),
            graph_built: graph_status.map(|s| s.processed).unwrap_or(false),
            last_updated: files.first()
                .map(|f| f.processed_at.to_rfc3339())
                .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
        })
    }
    
    /// Get processor capabilities for health check
    pub fn get_capabilities(&self) -> ProcessorCapabilities {
        ProcessorCapabilities {
            tree_sitter_enabled: true,
            docling_enabled: true,
            kafka_connected: true, // Kafka enabled for embeddings pipeline
        }
    }

    /// Store chunk nodes in FalkorDB (without embeddings) and publish to Kafka for the embeddings-service.
    ///
    /// Pipeline:
    ///   1. Cache chunks in memory
    ///   2. Publish chunks to Kafka for embeddings-service processing
    ///   3. Store chunks directly in FalkorDB (embeddings callback will update them later)
    pub async fn store_and_publish_chunks(&self, chunks: Vec<crate::core::chunking::Chunk>, source_id: &str, _repo_name: Option<String>, user_id: &str) -> Result<()> {

        use std::collections::HashMap;

        if chunks.is_empty() {
            tracing::warn!("store_and_publish_chunks called with empty chunks array for source_id: {}", source_id);
            return Ok(());
        }

        // Resolve the per-user graph for all FalkorDB operations in this function
        let user_graph = self.falkordb_storage.with_user_graph(user_id);
        // Ensure the user's graph has indexes (lazy init)
        if let Err(e) = user_graph.ensure_user_graph().await {
            tracing::warn!("Failed to ensure user graph indexes for user {}: {}", user_id, e);
        }

        let chunk_count = chunks.len();
        tracing::info!(
            "store_and_publish_chunks starting: source_id={}, chunk_count={}, user_id={}, graph={}",
            source_id, chunk_count, user_id, user_graph.graph_name
        );
        
        let filename = &chunks[0].file_path;
        
        // --- Incremental Diffing Logic ---
        let existing_snapshots = self.postgres_storage.get_chunk_snapshots(source_id, filename).await.unwrap_or_default();
        
        let mut to_insert_chunks = Vec::new();
        let mut to_delete_uuids = Vec::new();
        let mut unchanged_count = 0;
        
        let mut existing_map: HashMap<String, crate::infra::storage::ChunkSnapshot> = existing_snapshots.into_iter()
            .filter(|s| !s.tombstone)
            .map(|s| (s.chunk_hash.clone(), s))
            .collect();
            
        let mut chunks_to_process = Vec::new();
        
        for chunk in &chunks {
            // Check if chunk with identical hash exists
            if existing_map.remove(&chunk.chunk_hash).is_some() {
                unchanged_count += 1;
            } else {
                to_insert_chunks.push(chunk.clone());
                chunks_to_process.push(chunk.clone());
            }
        }
        
        // Remaining chunks in existing_map are missing from the new parse, meaning they were deleted
        for (_, snapshot) in existing_map {
            to_delete_uuids.push(snapshot.id);
        }
        
        tracing::info!(
            "Diff results for {}: {} unchanged, {} new/modified, {} deleted",
            filename, unchanged_count, chunks_to_process.len(), to_delete_uuids.len()
        );
        
        if chunks_to_process.is_empty() && to_delete_uuids.is_empty() {
            tracing::info!("No changes detected for {}, skipping processing.", filename);
            return Ok(());
        }
        
        let mut snapshots_to_insert = Vec::new();
        for chunk in &chunks_to_process {
            let (start_line, end_line) = chunk.metadata.line_range.unwrap_or((0, 0));
            snapshots_to_insert.push(crate::infra::storage::ChunkSnapshot {
                id: chunk.id,
                source_id: source_id.to_string(),
                filename: chunk.file_path.clone(),
                start_byte: start_line as i32,
                end_byte: end_line as i32,
                chunk_key: chunk.chunk_key.clone(),
                chunk_hash: chunk.chunk_hash.clone(),
                commit_id: None,
                embedding_model: None,
                last_indexed_at: chrono::Utc::now(),
                tombstone: false,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            });
        }
        
        if let Err(e) = self.postgres_storage.atomic_replace_file_chunks(source_id, filename, snapshots_to_insert, to_delete_uuids.clone()).await {
            tracing::warn!("Failed to atomic_replace_file_chunks: {}", e);
        }
        
        if !to_delete_uuids.is_empty() {
            if let Err(e) = user_graph.delete_chunks_by_ids(&to_delete_uuids).await {
                tracing::warn!("Failed to delete chunks from FalkorDB: {}", e);
            }
        }

        // Only process the new/modified chunks further down
        let chunks = chunks_to_process;
        let chunk_count = chunks.len();

        // ── Step 1: Cache chunk content in-memory so the embedding consumer can
        //           look it up by chunk_id without needing a PostgreSQL `chunks` table.
        {
            let mut cache = self.chunk_content_cache.lock().unwrap_or_else(|p| p.into_inner());
            for c in chunks.iter() {
                let composite_id = format!("{}|{}", c.chunk_key, c.file_path);
                cache.insert(composite_id, c.content.clone());
            }
            tracing::info!("Cached {} new chunks in-memory for source: {}", chunk_count, source_id);
        }



        // ── Step 1.5: Store chunks DIRECTLY in FalkorDB (without embeddings) ──
        // This ensures the knowledge graph is always populated, regardless of whether
        // the embeddings-service processes them. Uses MERGE so the embeddings callback
        // can update the node with the actual embedding vector later.
        {
            let mut chunks_stored = 0;
            tracing::info!(
                "[FalkorDB-Direct] Storing {} chunks directly in FalkorDB for source: {}",
                chunk_count, source_id
            );

            for c in chunks.iter() {
                let composite_id = format!("{}|{}", c.chunk_key, c.file_path);
                let repo_file_path = format!("{}/{}", source_id, c.file_path);
                let chunk_type_str = format!("{:?}", c.chunk_type);
                let metadata = serde_json::json!({
                    "confidence": c.confidence,
                    "level": format!("{:?}", c.level),
                    "user_id": user_id,
                });
                // Empty embedding — will be filled by the embeddings callback if it arrives
                let empty_embedding: Vec<f32> = vec![];
                let language_str = match &c.chunk_type {
                    crate::core::chunking::types::ChunkType::Code { language, .. } => language.clone(),
                    _ => "unknown".to_string(),
                };

                if let Err(e) = user_graph.store_chunk_with_embedding(
                    &composite_id,
                    source_id,
                    &c.content,
                    &empty_embedding,
                    &chunk_type_str,
                    &metadata,
                    "none",  // model = "none" until embeddings arrive
                    user_id,
                    &repo_file_path,
                    &language_str,
                ).await {
                    tracing::warn!(
                        "[FalkorDB-Direct] Failed to store chunk {} in FalkorDB: {}",
                        composite_id, e
                    );
                } else {
                    chunks_stored += 1;
                }
            }

            tracing::info!(
                "[FalkorDB-Direct] Stored {}/{} chunks directly in FalkorDB for source: {}",
                chunks_stored, chunk_count, source_id
            );
        }

        tracing::info!("store_and_publish_chunks completed successfully for source_id={}", source_id);
        Ok(())
    }

    /// Extract and store relationships across all chunks in a source.
    pub async fn extract_and_store_relationships(&self, chunks: &[crate::core::chunking::Chunk], source_id: &str, user_id: &str) -> Result<()> {
        let chunk_count = chunks.len();
        if chunk_count == 0 {
            return Ok(());
        }

        let user_graph = self.falkordb_storage.with_user_graph(user_id);
        
        // ── Step 2: Structural relationship extraction (source-agnostic router) ──
        tracing::info!("Starting structural relationship extraction for {} chunks", chunk_count);
        let mut relationships = self.relationship_router.extract_all(chunks);



        // ── Step 2.75: Semantic LLM relationship extraction ──
        // Only run LLM if the file is complex (e.g. >= 1500 characters) and structural extraction found 0 relationships.
        let is_complex_fallback = chunks.iter().any(|c| {
            if let crate::core::chunking::ChunkType::Code { .. } = c.chunk_type {
                c.content.chars().count() >= 1500 && relationships.is_empty()
            } else {
                false
            }
        });
        let semantic_chunks: Vec<_> = chunks.iter().filter(|c| {
            match &c.chunk_type {
                crate::core::chunking::ChunkType::Code { .. } => {
                    is_complex_fallback
                }
                _ => true
            }
        }).cloned().collect();
        
        tracing::info!("Filtered {} chunks down to {} for semantic LLM extraction", chunk_count, semantic_chunks.len());
        
        let semantic_rels = self.semantic_extractor.extract_semantic(&semantic_chunks).await;
        relationships.extend(semantic_rels);

        // ── Step 3: Ensure minimum connectivity with SameSource edges ──
        // Link every chunk to the first chunk of the source to guarantee they are connected to the same source.
        let first_chunk_id = chunks[0].id;
        for c in chunks.iter().skip(1) {
            relationships.push(crate::graph::models::ChunkRelationship::new(
                c.id,
                first_chunk_id,
                crate::graph::models::ChunkRelationType::SameSource,
                1.0,
            ));
        }

        let mut uuid_to_composite = std::collections::HashMap::new();
        for c in chunks {
            uuid_to_composite.insert(c.id, format!("{}|{}", c.chunk_key, c.file_path));
        }

        let mut rels_stored = 0;
        for rel in &relationships {
            if let (Some(source_composite), Some(target_composite)) = (
                uuid_to_composite.get(&rel.source_chunk_id),
                uuid_to_composite.get(&rel.target_chunk_id)
            ) {
                let metadata_val = serde_json::to_value(&rel.metadata).unwrap_or(serde_json::json!({}));
                if let Err(e) = user_graph.store_relationship(
                    source_composite,
                    target_composite,
                    rel.relationship_type.label(),
                    rel.confidence as f64,
                    &metadata_val
                ).await {
                    tracing::warn!("Failed to store relationship in FalkorDB: {}", e);
                } else {
                    rels_stored += 1;
                }
            } else {
                tracing::warn!("Could not find composite ID for relationship chunks");
            }
        }
        tracing::info!("Stored {} relationships in FalkorDB", rels_stored);

        tracing::info!("extract_and_store_relationships completed successfully for source_id={}", source_id);
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingStatus {
    pub source_id: String,
    pub total_files: usize,
    pub processed_files: usize,
    pub graph_built: bool,
    pub last_updated: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProcessorCapabilities {
    pub tree_sitter_enabled: bool,
    pub docling_enabled: bool,
    pub kafka_connected: bool,
}


/// Format a table as readable text for chunking.
fn format_table_as_text(table: &crate::processors::web::TableData) -> String {
    let mut out = String::new();
    if let Some(caption) = &table.caption {
        out.push_str(&format!("Table: {}\n", caption));
    }
    if !table.headers.is_empty() {
        out.push_str(&table.headers.join(" | "));
        out.push('\n');
        out.push_str(&table.headers.iter().map(|h| "-".repeat(h.len().max(3))).collect::<Vec<_>>().join("-|-"));
        out.push('\n');
    }
    for row in &table.rows {
        out.push_str(&row.join(" | "));
        out.push('\n');
    }
    out
}
/// Helper: detect language from file path
/// DSA: O(1) static HashMap lookup, zero per-call allocation.
#[allow(dead_code)]
fn detect_language(file_path: &str) -> Option<String> {
    static EXT_TO_LANG: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
        [
            ("py", "Python"),
            ("rs", "Rust"),
            ("js", "JavaScript"),
            ("mjs", "JavaScript"),
            ("cjs", "JavaScript"),
            ("ts", "TypeScript"),
            ("tsx", "TypeScript"),
            ("go", "Go"),
            ("java", "Java"),
            ("c", "C"),
            ("h", "C"),
            ("cpp", "C++"),
            ("cc", "C++"),
            ("cxx", "C++"),
            ("hpp", "C++"),
        ]
        .into_iter()
        .collect()
    });

    let ext = file_path.rsplit('.').next()?.to_lowercase();
    EXT_TO_LANG.get(ext.as_str()).map(|s| s.to_string())
}

