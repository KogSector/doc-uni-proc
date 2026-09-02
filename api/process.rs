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

#[tokio::main]
async fn main() -> Result<(), Error> {
    run(handler).await
}

pub async fn handler(req: Request) -> Result<Response<Body>, Error> {
    let processor = get_processor().await.map_err(|e| {
        Error::from(format!("Failed to initialize processor: {}", e))
    })?;

    let body_bytes = req.body().as_ref();
    let request: ProcessDocumentRequest = serde_json::from_slice(body_bytes).map_err(|e| {
        Error::from(format!("Failed to parse request body: {}", e))
    })?;

    let start_time = std::time::Instant::now();

    let result = processor
        .process_file(
            &request.content,
            request.is_base64.unwrap_or(false),
            &request.filename,
            &request.source_id,
            "unknown/repo",
            &request.user_id,
        )
        .await;

    let processing_time = start_time.elapsed().as_millis() as u64;

    let (status, response) = match result {
        Ok(data) => (
            StatusCode::OK,
            ProcessDocumentResponse {
                success: true,
                data: Some(data),
                error: None,
                processing_time_ms: processing_time,
            },
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            ProcessDocumentResponse {
                success: false,
                data: None,
                error: Some(e.to_string()),
                processing_time_ms: processing_time,
            },
        ),
    };

    let resp_bytes = serde_json::to_vec(&response).map_err(|e| Error::from(e.to_string()))?;

    Ok(Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .body(Body::from(resp_bytes))?)
}
