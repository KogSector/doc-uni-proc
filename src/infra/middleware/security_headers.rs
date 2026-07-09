//! Security headers middleware for Axum
//!
//! Adds standard security headers to all responses.


use axum::{
    extract::Request,
    http::HeaderValue,
    middleware::Next,
    response::Response,
};

/// Security headers middleware for Axum services

pub async fn security_headers_middleware(
    request: Request,
    next: Next,
) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();

    headers.insert("X-Content-Type-Options", HeaderValue::from_static("nosniff"));
    headers.insert("X-Frame-Options", HeaderValue::from_static("DENY"));
    headers.insert(
        "X-XSS-Protection",
        HeaderValue::from_static("1; mode=block"),
    );
    headers.insert(
        "Strict-Transport-Security",
        HeaderValue::from_static("max-age=31536000; includeSubDomains; preload"),
    );
    headers.insert(
        "Referrer-Policy",
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    headers.insert(
        "Permissions-Policy",
        HeaderValue::from_static("camera=(), microphone=(), geolocation=(), payment=()"),
    );
    headers.insert(
        "Content-Security-Policy",
        HeaderValue::from_static("default-src 'none'; frame-ancestors 'none'"),
    );
    headers.insert(
        "Cache-Control",
        HeaderValue::from_static("no-store, no-cache, must-revalidate"),
    );
    headers.insert(
        "Cross-Origin-Opener-Policy",
        HeaderValue::from_static("same-origin"),
    );
    headers.insert(
        "Cross-Origin-Resource-Policy",
        HeaderValue::from_static("same-origin"),
    );
    headers.remove("Server");

    response
}
