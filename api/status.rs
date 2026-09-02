use serde::Serialize;
use vercel_runtime::{run, Body, Error, Request, Response, StatusCode};
use unified_processor_lib::get_processor;

#[derive(Debug, Serialize)]
pub struct ProcessingStatusResponse {
    pub source_id: String,
    pub total_files: usize,
    pub processed_files: usize,
    pub graph_built: bool,
    pub last_updated: String,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    run(handler).await
}

pub async fn handler(req: Request) -> Result<Response<Body>, Error> {
    let processor = get_processor().await.map_err(|e| {
        Error::from(format!("Failed to initialize processor: {}", e))
    })?;

    let user_id = req
        .headers()
        .get("x-user-id")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("system");

    // Extract source_id from path or query
    let path = req.uri().path();
    let source_id = if let Some(stripped) = path.strip_prefix("/api/v1/status/") {
        stripped.trim_matches('/').to_string()
    } else if let Some(stripped) = path.strip_prefix("/status/") {
        stripped.trim_matches('/').to_string()
    } else if let Some(query) = req.uri().query() {
        query
            .split('&')
            .find_map(|pair| {
                let mut parts = pair.split('=');
                if parts.next()? == "source_id" {
                    parts.next().map(|s| s.to_string())
                } else {
                    None
                }
            })
            .unwrap_or_default()
    } else {
        req.headers()
            .get("x-source-id")
            .and_then(|h| h.to_str().ok())
            .unwrap_or_default()
            .to_string()
    };

    if source_id.is_empty() {
        let err_json = serde_json::json!({
            "error": "Missing source_id in path or query parameters"
        });
        return Ok(Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .header("Content-Type", "application/json")
            .body(Body::from(serde_json::to_vec(&err_json).unwrap()))?);
    }

    match processor.get_processing_status(&source_id, user_id).await {
        Ok(status) => {
            let response = ProcessingStatusResponse {
                source_id: status.source_id,
                total_files: status.total_files,
                processed_files: status.processed_files,
                graph_built: status.graph_built,
                last_updated: status.last_updated,
            };
            let resp_bytes = serde_json::to_vec(&response).map_err(|e| Error::from(e.to_string()))?;
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .body(Body::from(resp_bytes))?)
        }
        Err(e) => {
            let err_json = serde_json::json!({
                "error": format!("Failed to retrieve status: {}", e)
            });
            Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_vec(&err_json).unwrap()))?)
        }
    }
}
