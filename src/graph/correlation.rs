use axum::{
    extract::Request,
    middleware::Next,
    response::Response,
};
use tracing::Instrument;
use uuid::Uuid;

pub async fn correlation_middleware(mut request: Request, next: Next) -> Response {
    let correlation_id = request
        .headers()
        .get("x-correlation-id")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    // Inject back into request headers so downstream can see it if needed
    if let Ok(val) = correlation_id.parse() {
        request.headers_mut().insert("x-correlation-id", val);
    }

    let span = tracing::info_span!("request", correlation_id = %correlation_id);
    
    let mut response = next.run(request).instrument(span).await;
    
    if let Ok(val) = correlation_id.parse() {
        response.headers_mut().insert("x-correlation-id", val);
    }
    
    response
}
