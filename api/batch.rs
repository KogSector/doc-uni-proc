use serde::{Deserialize, Serialize};
use vercel_runtime::{run, Body, Error, Request, Response, StatusCode};
use unified_processor_lib::core::orchestrator::ProcessedData;
use unified_processor_lib::get_processor;

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
    pub data: Option<ProcessedData>,
    pub error: Option<String>,
    pub processing_time_ms: u64,
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

#[tokio::main]
async fn main() -> Result<(), Error> {
    run(handler).await
}

pub async fn handler(req: Request) -> Result<Response<Body>, Error> {
    let processor = get_processor().await.map_err(|e| {
        Error::from(format!("Failed to initialize processor: {}", e))
    })?;

    let body_bytes = req.body().as_ref();
    let request: BatchProcessRequest = serde_json::from_slice(body_bytes).map_err(|e| {
        Error::from(format!("Failed to parse request body: {}", e))
    })?;

    let start_time = std::time::Instant::now();
    let mut results = Vec::new();
    let mut processed_count = 0;
    let mut failed_count = 0;

    for file_request in request.files {
        let file_start_time = std::time::Instant::now();
        match processor
            .process_file(
                &file_request.content,
                file_request.is_base64.unwrap_or(false),
                &file_request.filename,
                &file_request.source_id,
                "unknown/repo",
                &file_request.user_id,
            )
            .await
        {
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

    let response = BatchProcessResponse {
        success: failed_count == 0,
        processed_files: processed_count,
        failed_files: failed_count,
        results,
        total_processing_time_ms: total_processing_time,
    };

    let resp_bytes = serde_json::to_vec(&response).map_err(|e| Error::from(e.to_string()))?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(resp_bytes))?)
}
