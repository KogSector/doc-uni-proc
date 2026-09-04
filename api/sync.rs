use serde::{Deserialize, Serialize};
use vercel_runtime::{run, Body, Error, Request, Response, StatusCode};
use unified_processor_lib::get_processor;

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
        .unwrap_or("system")
        .to_string();

    let body_bytes = req.body().as_ref();
    let request: TriggerGraphSyncRequest = serde_json::from_slice(body_bytes).map_err(|e| {
        Error::from(format!("Failed to parse request body: {}", e))
    })?;

    let response = match processor.trigger_graph_sync(&request.source_id, &user_id).await {
        Ok(_) => TriggerGraphSyncResponse {
            success: true,
            message: "Graph sync triggered successfully".to_string(),
            error: None,
        },
        Err(e) => TriggerGraphSyncResponse {
            success: false,
            message: "Failed to trigger graph sync".to_string(),
            error: Some(e.to_string()),
        },
    };

    let resp_bytes = serde_json::to_vec(&response).map_err(|e| Error::from(e.to_string()))?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(resp_bytes))?)
}
