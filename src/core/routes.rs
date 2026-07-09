use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::core::orchestrator::{UnifiedProcessor, WebProcessingRequest, WebProcessingResult};
use crate::infra::middleware::AxumAuthLayer;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

pub fn build_app_router(
    processor: Arc<UnifiedProcessor>,
    auth_layer: AxumAuthLayer,
    rate_limit: crate::infra::middleware::AxumRateLimitConfig,
) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let protected_routes = Router::new()
        // Document processing endpoints
        .route("/api/v1/documents/process", post(process_document))
        .route("/api/v1/documents/batch", post(batch_process_documents))

        // Web scraping / crawling endpoints
        .route("/api/v1/web/scrape", post(scrape_url))
        .route("/api/v1/web/crawl", post(crawl_website))
        // Integration endpoints
        .route("/api/v1/graph/sync", post(trigger_graph_sync))
        .route("/api/v1/status/{source_id}", get(get_processing_status))
        // Legacy compatibility endpoints
        .route("/api/v1/process", post(process_document))
        .layer(axum::middleware::from_fn_with_state(rate_limit.clone(), crate::infra::middleware::axum_rate_limit_middleware))
        .layer(axum::middleware::from_fn_with_state(auth_layer, crate::infra::middleware::axum_auth_middleware))
        .layer(axum::extract::DefaultBodyLimit::disable());

    Router::new()
        // Health endpoints
        .route("/", get(health_check))
        .route("/health", get(health_check))
        // Internal service-to-service: process files already on the shared volume.
        // Bypasses auth middleware since data-connector (internal) doesn't need a user token.
        .route("/api/v1/process/local", post(process_local_directory))
        .merge(protected_routes)
        // Global Middleware stack
        .layer(axum::middleware::from_fn(crate::graph::correlation_middleware))
        .layer(axum::middleware::from_fn(crate::infra::middleware::security_headers_middleware))
        .layer(axum::middleware::from_fn(crate::infra::middleware::zero_trust_middleware))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(processor)
}


// ==========================================
// From documents.rs
// ==========================================

// Document processing API endpoints

#[derive(Debug, Deserialize)]
pub struct ProcessDocumentRequest {
    pub content: String,
    pub is_base64: Option<bool>,
    pub filename: String,
    pub source_id: String,
    pub user_id: String,
}

#[derive(Debug, Serialize)]
pub struct ProcessDocumentResponse {
    pub success: bool,
    pub data: Option<crate::core::orchestrator::ProcessedData>,
    pub error: Option<String>,
    pub processing_time_ms: u64,
}

pub async fn process_document(
    State(processor): State<Arc<UnifiedProcessor>>,
    Json(request): Json<ProcessDocumentRequest>,
) -> Result<Json<ProcessDocumentResponse>, StatusCode> {
    let start_time = std::time::Instant::now();
    
    // Spawn background task to prevent API gateway timeouts (e.g. Render 100s limit)
    tokio::spawn(async move {
        tracing::info!("Started background processing for document: {}", request.filename);
        let filename_for_log = request.filename.clone();
        let result = futures::FutureExt::catch_unwind(std::panic::AssertUnwindSafe(async {
            processor.process_file(
                &request.content, 
                request.is_base64.unwrap_or(false), 
                &request.filename, 
                &request.source_id, 
                "unknown/repo",
                &request.user_id
            ).await
        })).await;
        
        match result {
            Ok(Ok(_)) => tracing::info!("Successfully processed document in background: {}", filename_for_log),
            Ok(Err(e)) => tracing::error!("Failed to process document in background {}: {}", filename_for_log, e),
            Err(panic_info) => tracing::error!(
                "PANIC in background processing for {}: {:?}",
                filename_for_log, panic_info
            ),
        }
    });

    let processing_time = start_time.elapsed().as_millis() as u64;
    Ok(Json(ProcessDocumentResponse {
        success: true,
        data: None,
        error: None,
        processing_time_ms: processing_time,
    }))
}

#[derive(Debug, Deserialize)]
pub struct BatchProcessRequest {
    pub files: Vec<ProcessDocumentRequest>,
}

#[derive(Debug, Serialize)]
pub struct BatchProcessResponse {
    pub success: bool,
    pub processed_files: usize,
    pub failed_files: usize,
    pub results: Vec<ProcessDocumentResponse>,
    pub total_processing_time_ms: u64,
}

pub async fn batch_process_documents(
    State(processor): State<Arc<UnifiedProcessor>>,
    Json(request): Json<BatchProcessRequest>,
) -> Result<Json<BatchProcessResponse>, StatusCode> {
    let start_time = std::time::Instant::now();
    let mut results = Vec::new();
    let mut processed_count = 0;
    let mut failed_count = 0;

    for file_request in request.files {
        let file_start_time = std::time::Instant::now();
        
        match processor.process_file(&file_request.content, file_request.is_base64.unwrap_or(false), &file_request.filename, &file_request.source_id, "unknown/repo", &file_request.user_id).await {
            Ok(data) => {
                let processing_time = file_start_time.elapsed().as_millis() as u64;
                results.push(ProcessDocumentResponse {
                    success: true,
                    data: Some(data),
                    error: None,
                    processing_time_ms: processing_time,
                });
                processed_count += 1;
            }
            Err(e) => {
                let processing_time = file_start_time.elapsed().as_millis() as u64;
                results.push(ProcessDocumentResponse {
                    success: false,
                    data: None,
                    error: Some(e.to_string()),
                    processing_time_ms: processing_time,
                });
                failed_count += 1;
            }
        }
    }

    let total_processing_time = start_time.elapsed().as_millis() as u64;

    Ok(Json(BatchProcessResponse {
        success: failed_count == 0,
        processed_files: processed_count,
        failed_files: failed_count,
        results,
        total_processing_time_ms: total_processing_time,
    }))
}


#[derive(Debug, Deserialize)]
pub struct TriggerGraphSyncRequest {
    pub source_id: String,
    pub force_rebuild: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct TriggerGraphSyncResponse {
    pub success: bool,
    pub message: String,
    pub error: Option<String>,
}

pub async fn trigger_graph_sync(
    headers: axum::http::HeaderMap,
    State(processor): State<Arc<UnifiedProcessor>>,
    Json(request): Json<TriggerGraphSyncRequest>,
) -> Result<Json<TriggerGraphSyncResponse>, StatusCode> {
    let user_id = headers.get("x-user-id").and_then(|h| h.to_str().ok()).unwrap_or("system");
    match processor.trigger_graph_sync(&request.source_id, user_id).await {
        Ok(_) => Ok(Json(TriggerGraphSyncResponse {
            success: true,
            message: "Graph sync triggered successfully".to_string(),
            error: None,
        })),
        Err(e) => Ok(Json(TriggerGraphSyncResponse {
            success: false,
            message: "Failed to trigger graph sync".to_string(),
            error: Some(e.to_string()),
        })),
    }
}

#[derive(Debug, Serialize)]
pub struct ProcessingStatusResponse {
    pub source_id: String,
    pub total_files: usize,
    pub processed_files: usize,
    pub graph_built: bool,
    pub last_updated: String,
}

pub async fn get_processing_status(
    headers: axum::http::HeaderMap,
    State(processor): State<Arc<UnifiedProcessor>>,
    Path(source_id): Path<String>,
) -> Result<Json<ProcessingStatusResponse>, StatusCode> {
    let user_id = match headers.get("x-user-id").and_then(|h| h.to_str().ok()) {
        Some(uid) => uid,
        None => return Err(StatusCode::UNAUTHORIZED),
    };
    match processor.get_processing_status(&source_id, user_id).await {
        Ok(status) => Ok(Json(ProcessingStatusResponse {
            source_id: status.source_id,
            total_files: status.total_files,
            processed_files: status.processed_files,
            graph_built: status.graph_built,
            last_updated: status.last_updated,
        })),
        Err(_e) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

// ==========================================
// From health.rs
// ==========================================

// Health check endpoints

/// Health check response
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub service: String,
    pub version: String,
}

/// Detailed status response
#[derive(Debug, Serialize, Deserialize)]
pub struct StatusResponse {
    pub status: String,
    pub service: String,
    pub version: String,
    pub components: ComponentStatus,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ComponentStatus {
    pub tree_sitter: String,
    pub docling: String,
    pub embedding_model: String,
    pub postgres: String,
}

/// Health check endpoint
pub async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy".to_string(),
        service: "unified-processor".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// Detailed status endpoint
pub async fn get_status(State(state): State<Arc<AppState>>) -> Json<StatusResponse> {
    let capabilities = state.processor.get_capabilities();

    Json(StatusResponse {
        status: "running".to_string(),
        service: "unified-processor".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        components: ComponentStatus {
            tree_sitter: if capabilities.tree_sitter_enabled {
                "enabled".to_string()
            } else {
                "disabled".to_string()
            },
            docling: if capabilities.docling_enabled {
                "enabled".to_string()
            } else {
                "disabled".to_string()
            },
            embedding_model: if capabilities.docling_enabled { "active".to_string() } else { "inactive".to_string() },
            postgres: "connected".to_string(),
        },
    })
}

// ============================================================================
// Local shared-volume processing (internal, service-to-service)
// ============================================================================

/// Posted by data-connector after downloading a source to the shared volume.
///
/// # AKS Portability
/// `directory_path` is always inside the shared volume (`DOWNLOADS_BASE_PATH`).
/// Locally: Docker named volume.  AKS: Azure Files PVC — no code change needed.
#[derive(Debug, Deserialize)]
pub struct LocalProcessRequest {
    /// Source ID used for logging and metadata tagging.
    pub source_id: String,
    /// Absolute path to the directory to scan inside the shared volume,
    /// e.g. `/shared/downloads/repos/<source_id>`.
    pub directory_path: String,
    pub user_id: Option<String>,
}

/// Immediate acknowledgement returned with 202 Accepted.
#[derive(Debug, Serialize)]
pub struct LocalProcessResponse {
    pub accepted: bool,
    pub source_id: String,
    pub message: String,
}

/// Process files from a locally mounted shared volume directory.
///
/// Called internally by `data-connector` after it finishes downloading a source.
/// Returns **202 Accepted** immediately; processing runs in a background tokio task
/// so large repos do not cause HTTP timeouts.
///
/// `POST /api/v1/process/local`
pub async fn process_local_directory(
    State(processor): State<Arc<UnifiedProcessor>>,
    Json(request): Json<LocalProcessRequest>,
) -> impl IntoResponse {
    let source_id = request.source_id.clone();
    let directory_path = request.directory_path.clone();
    let user_id = request.user_id.clone().unwrap_or_else(|| "system".to_string());

    tracing::info!(
        source_id = %source_id,
        directory_path = %directory_path,
        user_id = %user_id,
        "Accepted local directory processing request"
    );

    // Clone the Arc<UnifiedProcessor> (cheap refcount bump) so the background
    // task holds its own reference without needing UnifiedProcessor to be Clone.
    let processor_clone = processor.clone();
    tokio::spawn(async move {
        let result = processor_clone.process_local_directory(&source_id, &directory_path, &user_id).await;
        
        // Always clean up the directory after processing, regardless of success or failure
        tracing::info!(directory = %directory_path, "Cleaning up local directory after processing attempt");
        if let Err(e) = std::fs::remove_dir_all(&directory_path) {
            tracing::warn!(
                directory = %directory_path,
                error = %e,
                "Failed to clean up local directory (it may have been already deleted or does not exist)"
            );
        }

        if let Err(e) = result {
            tracing::error!(
                source_id = %source_id,
                directory_path = %directory_path,
                error = %e,
                "Local directory processing failed"
            );
        }
    });

    (
        StatusCode::ACCEPTED,
        Json(LocalProcessResponse {
            accepted: true,
            source_id: request.source_id,
            message: "Directory processing started in background".to_string(),
        }),
    )
}



// ==========================================
// From web.rs
// ==========================================

// REST API endpoints for web scraping / crawling.
//
// Endpoints:
//   POST /api/v1/web/scrape  — scrape a single URL
//   POST /api/v1/web/crawl   — BFS-crawl an entire website

// ─── Request / Response DTOs ─────────────────────────────────────────────────

/// Scrape a single URL.
#[derive(Debug, Deserialize)]
pub struct ScrapeRequest {
    pub url: String,
    #[serde(default)]
    pub include_css: Option<bool>,
    #[serde(default)]
    pub include_js: Option<bool>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// Crawl an entire website.
#[derive(Debug, Deserialize)]
pub struct CrawlRequest {
    pub url: String,
    #[serde(default)]
    pub max_pages: Option<usize>,
    #[serde(default)]
    pub max_depth: Option<usize>,
    #[serde(default)]
    pub crawl_delay_ms: Option<u64>,
    #[serde(default)]
    pub include_css: Option<bool>,
    #[serde(default)]
    pub include_js: Option<bool>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// Unified response for both scrape and crawl.
#[derive(Debug, Serialize)]
pub struct WebResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<WebProcessingResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// ─── Handlers ────────────────────────────────────────────────────────────────

/// POST /api/v1/web/scrape
///
/// Scrape a single URL: fetch HTML, parse, chunk, embed, store.
pub async fn scrape_url(
    State(processor): State<Arc<UnifiedProcessor>>,
    Json(body): Json<ScrapeRequest>,
) -> Json<WebResponse> {
    let request_id = uuid::Uuid::new_v4().to_string();

    tracing::info!(request_id = %request_id, url = %body.url, "Received scrape request");

    let req = WebProcessingRequest {
        request_id: request_id.clone(),
        user_id: "api".to_string(),
        url: body.url.clone(),
        crawl: false,
        max_pages: Some(1),
        max_depth: Some(0),
        crawl_delay_ms: None,
        include_css: body.include_css,
        include_js: body.include_js,
        metadata: body.metadata,
    };

    match processor.handle_web_processing(req).await {
        Ok(result) => Json(WebResponse {
            success: true,
            data: Some(result),
            error: None,
        }),
        Err(e) => {
            tracing::error!(request_id = %request_id, error = %e, "Scrape failed");
            Json(WebResponse {
                success: false,
                data: None,
                error: Some(e.to_string()),
            })
        }
    }
}

/// POST /api/v1/web/crawl
///
/// BFS-crawl an entire website: discover pages, scrape each, chunk, embed, store.
pub async fn crawl_website(
    State(processor): State<Arc<UnifiedProcessor>>,
    Json(body): Json<CrawlRequest>,
) -> Json<WebResponse> {
    let request_id = uuid::Uuid::new_v4().to_string();

    tracing::info!(
        request_id = %request_id,
        url = %body.url,
        max_pages = ?body.max_pages,
        max_depth = ?body.max_depth,
        "Received crawl request"
    );

    let req = WebProcessingRequest {
        request_id: request_id.clone(),
        user_id: "api".to_string(),
        url: body.url.clone(),
        crawl: true,
        max_pages: body.max_pages,
        max_depth: body.max_depth,
        crawl_delay_ms: body.crawl_delay_ms,
        include_css: body.include_css,
        include_js: body.include_js,
        metadata: body.metadata,
    };

    match processor.handle_web_processing(req).await {
        Ok(result) => Json(WebResponse {
            success: true,
            data: Some(result),
            error: None,
        }),
        Err(e) => {
            tracing::error!(request_id = %request_id, error = %e, "Crawl failed");
            Json(WebResponse {
                success: false,
                data: None,
                error: Some(e.to_string()),
            })
        }
    }
}