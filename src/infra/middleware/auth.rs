//! Axum authentication middleware
//!
//! JWT Bearer token and API key authentication for Axum services.
//! Validates tokens by calling the auth-middleware service.


use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use crate::infra::proto::confuse_auth_v1::auth_client::AuthClient;
use crate::infra::proto::confuse_auth_v1::ValidateTokenRequest;
use tonic::transport::Channel;

/// Authenticated user extracted from JWT/API key
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthenticatedUser {
    pub id: String,
    pub email: String,
    pub name: Option<String>,
    pub picture: Option<String>,
    pub roles: Vec<String>,
    pub workspace_id: Option<String>,
}

/// Authentication layer configuration for Axum
#[derive(Clone)]
pub struct AxumAuthLayer {
    pub auth_service_url: String,
    pub auth_grpc_url: Option<String>,
    http_client: reqwest::Client,
    grpc_client: Option<AuthClient<Channel>>,
}

impl AxumAuthLayer {
    pub fn new(auth_service_url: String) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap_or_default();

        Self {
            auth_service_url,
            auth_grpc_url: None,
            http_client,
            grpc_client: None,
        }
    }

    /// Initialize with gRPC support
    pub async fn with_grpc(
        auth_service_url: String,
        auth_grpc_url: String,
    ) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap_or_default();

        let grpc_client = if !auth_grpc_url.is_empty() {
            match AuthClient::connect(auth_grpc_url.clone()).await {
                Ok(client) => Some(client),
                Err(e) => {
                    tracing::error!("Failed to connect to auth gRPC service at {}: {}", auth_grpc_url, e);
                    None
                }
            }
        } else {
            None
        };

        Self {
            auth_service_url,
            auth_grpc_url: Some(auth_grpc_url),
            http_client,
            grpc_client,
        }
    }

    /// Validate a Bearer token against auth-middleware
    pub async fn verify_token(&self, token: &str) -> Result<AuthenticatedUser, String> {
        // Try gRPC first if available
        if let Some(mut client) = self.grpc_client.clone() {
            let request = tonic::Request::new(ValidateTokenRequest {
                token: token.to_string(),
            });

            match client.validate_token(request).await {
                Ok(response) => {
                    let resp = response.into_inner();
                    if resp.valid {
                        return Ok(AuthenticatedUser {
                            id: resp.user_id.unwrap_or_default(),
                            email: resp.email, 
                            name: None,
                            picture: None,
                            roles: resp.roles,
                            workspace_id: None,
                        });
                    } else {
                        return Err(resp.error.unwrap_or_else(|| "Invalid token".to_string()));
                    }
                }
                Err(e) => {
                    tracing::warn!("Auth gRPC call failed, falling back to HTTP: {}", e);
                }
            }
        }

        // Fallback to HTTP
        let url = format!("{}/auth/validate", self.auth_service_url);

        let res = self
            .http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .map_err(|e| format!("Auth service request failed: {}", e))?;

        if !res.status().is_success() {
            return Err(format!(
                "Auth service rejected token: {}",
                res.status()
            ));
        }

        let user: AuthenticatedUser = res
            .json()
            .await
            .map_err(|e| format!("Failed to parse auth response: {}", e))?;

        Ok(user)
    }

    /// Validate an API key against auth-middleware
    pub async fn validate_api_key(&self, key: &str) -> Result<AuthenticatedUser, String> {
        // Try gRPC first if available
        if let Some(mut client) = self.grpc_client.clone() {
            let request = tonic::Request::new(crate::infra::proto::confuse_auth_v1::ValidateApiKeyRequest {
                api_key: key.to_string(),
            });

            match client.validate_api_key(request).await {
                Ok(response) => {
                    let resp = response.into_inner();
                    if resp.valid {
                        return Ok(AuthenticatedUser {
                            id: resp.user_id,
                            email: resp.email,
                            name: None,
                            picture: None,
                            roles: resp.roles,
                            workspace_id: None,
                        });
                    } else {
                        return Err(resp.error);
                    }
                }
                Err(e) => {
                    tracing::warn!("Auth gRPC call failed for API key, falling back to HTTP: {}", e);
                }
            }
        }

        // Fallback to HTTP
        let url = format!("{}/auth/validate-api-key", self.auth_service_url);

        let res = self
            .http_client
            .post(&url)
            .header("X-API-Key", key)
            .send()
            .await
            .map_err(|e| format!("Auth service request failed: {}", e))?;

        if !res.status().is_success() {
            return Err(format!(
                "Auth service rejected API key: {}",
                res.status()
            ));
        }

        let user: AuthenticatedUser = res
            .json()
            .await
            .map_err(|e| format!("Failed to parse auth response: {}", e))?;

        Ok(user)
    }
}


/// Authentication middleware function for Axum

pub async fn axum_auth_middleware(
    State(auth_layer): State<AxumAuthLayer>,
    mut request: Request,
    next: Next,
) -> Result<Response, Response> {
    // Try to extract authorization
    let auth_header = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let api_key = request
        .headers()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // Extract workspace ID from headers (optional)
    let workspace_id = request
        .headers()
        .get("X-Workspace-Id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let mut user = if let Some(auth_value) = auth_header {
        if let Some(token) = auth_value.strip_prefix("Bearer ") {
            auth_layer.verify_token(token).await.map_err(|e| {
                tracing::warn!("Token verification failed: {}", e);
                (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({
                        "error": { "code": "UNAUTHORIZED", "message": e }
                    })),
                )
                    .into_response()
            })?
        } else {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": { "code": "UNAUTHORIZED", "message": "Invalid authorization header format" }
                })),
            )
                .into_response());
        }
    } else if let Some(key) = api_key {
        auth_layer.validate_api_key(&key).await.map_err(|e| {
            tracing::warn!("API key validation failed: {}", e);
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": { "code": "UNAUTHORIZED", "message": e }
                })),
            )
                .into_response()
        })?
    } else {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": { "code": "UNAUTHORIZED", "message": "No authentication provided" }
            })),
        )
            .into_response());
    };

    // Set workspace_id if provided in headers
    if workspace_id.is_some() {
        user.workspace_id = workspace_id;
    }

    // Attach user to request extensions
    request.extensions_mut().insert(user);

    Ok(next.run(request).await)
}

/// Optional authentication — doesn't fail if no auth provided

pub async fn axum_optional_auth_middleware(
    State(auth_layer): State<AxumAuthLayer>,
    mut request: Request,
    next: Next,
) -> Response {
    // Try to extract and validate authorization
    let auth_header = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    if let Some(auth_value) = auth_header {
        if let Some(token) = auth_value.strip_prefix("Bearer ") {
            if let Ok(user) = auth_layer.verify_token(token).await {
                request.extensions_mut().insert(user);
            }
        }
    }

    next.run(request).await
}
