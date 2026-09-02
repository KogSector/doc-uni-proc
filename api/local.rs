use serde::{Deserialize, Serialize};
use vercel_runtime::{run, Body, Error, Request, Response, StatusCode};
use unified_processor_lib::get_processor;

#[derive(Debug, Deserialize)]
pub struct LocalProcessRequest {
    pub source_id: String,
    pub directory_path: String,
    pub user_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LocalProcessResponse {
    pub accepted: bool,
    pub source_id: String,
    pub message: String,
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
    let request: LocalProcessRequest = serde_json::from_slice(body_bytes).map_err(|e| {
        Error::from(format!("Failed to parse request body: {}", e))
    })?;

    let source_id = request.source_id.clone();
    let directory_path = request.directory_path.clone();
    let user_id = request.user_id.clone().unwrap_or_else(|| "system".to_string());

    tracing::info!(
        source_id = %source_id,
        directory_path = %directory_path,
        user_id = %user_id,
        "Accepted local directory processing request in serverless function"
    );

    let processor_clone = processor.clone();
    let dir_clone = directory_path.clone();
    let src_clone = source_id.clone();

    // Process directory
    tokio::spawn(async move {
        let result = processor_clone
            .process_local_directory(&src_clone, &dir_clone, &user_id)
            .await;

        tracing::info!(directory = %dir_clone, "Cleaning up local directory after processing attempt");
        let _ = std::fs::remove_dir_all(&dir_clone);

        if let Err(e) = result {
            tracing::error!(
                source_id = %src_clone,
                directory_path = %dir_clone,
                error = %e,
                "Local directory processing failed"
            );
        }
    });

    let response = LocalProcessResponse {
        accepted: true,
        source_id: request.source_id,
        message: "Directory processing started".to_string(),
    };

    let resp_bytes = serde_json::to_vec(&response).map_err(|e| Error::from(e.to_string()))?;

    Ok(Response::builder()
        .status(StatusCode::ACCEPTED)
        .header("Content-Type", "application/json")
        .body(Body::from(resp_bytes))?)
}
