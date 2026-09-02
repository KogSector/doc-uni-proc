use serde_json::json;
use vercel_runtime::{run, Body, Error, Request, Response, StatusCode};
use unified_processor_lib::get_processor;

#[tokio::main]
async fn main() -> Result<(), Error> {
    run(handler).await
}

pub async fn handler(_req: Request) -> Result<Response<Body>, Error> {
    let _processor = get_processor().await.map_err(|e| {
        Error::from(format!("Failed to initialize processor: {}", e))
    })?;

    let response = json!({
        "status": "healthy",
        "service": "doc-uni-proc",
        "version": env!("CARGO_PKG_VERSION"),
    });

    let resp_bytes = serde_json::to_vec(&response).map_err(|e| Error::from(e.to_string()))?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(resp_bytes))?)
}
