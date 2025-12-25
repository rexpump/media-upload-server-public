//! API Key authentication middleware.
//!
//! Provides simple API key authentication for upload endpoints.
//! Keys are configured in the config file.
//!
//! # Authentication Methods
//!
//! The middleware accepts API keys via:
//! 1. `Authorization: Bearer <api_key>` header
//! 2. `X-API-Key: <api_key>` header
//! 3. `?api_key=<api_key>` query parameter
//!
//! # Example
//!
//! ```rust,ignore
//! let auth = ApiKeyAuth::new(&config.auth);
//! let app = Router::new()
//!     .route("/api/upload", post(upload))
//!     .layer(auth.layer());
//! ```

use axum::{
    body::Body,
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
};
use std::{
    collections::HashSet,
    sync::Arc,
    task::{Context, Poll},
};
use tower::{Layer, Service};
use tracing::{debug, warn};

use crate::config::AuthConfig;

/// API Key authentication middleware
#[derive(Clone)]
pub struct ApiKeyAuth {
    /// Set of valid API keys for O(1) lookup
    valid_keys: Arc<HashSet<String>>,
    /// Whether auth is enabled
    enabled: bool,
    /// Protected path prefixes
    protected_paths: Arc<Vec<String>>,
    /// Public path prefixes (bypass auth)
    public_paths: Arc<Vec<String>>,
}

impl ApiKeyAuth {
    /// Create a new API key authenticator from configuration
    pub fn new(config: &AuthConfig) -> Self {
        let valid_keys: HashSet<String> = config.api_keys.iter().cloned().collect();

        Self {
            valid_keys: Arc::new(valid_keys),
            enabled: config.enabled,
            protected_paths: Arc::new(config.protected_paths.clone()),
            public_paths: Arc::new(config.public_paths.clone()),
        }
    }

    /// Create a Tower Layer for this authenticator
    pub fn layer(&self) -> ApiKeyAuthLayer {
        ApiKeyAuthLayer {
            auth: self.clone(),
        }
    }

    /// Check if a path requires authentication
    fn requires_auth(&self, path: &str) -> bool {
        if !self.enabled {
            return false;
        }

        // Check if path is explicitly public
        for public_path in self.public_paths.iter() {
            if path.starts_with(public_path) {
                return false;
            }
        }

        // If protected_paths is empty, protect all paths
        if self.protected_paths.is_empty() {
            return true;
        }

        // Check if path matches any protected path
        for protected_path in self.protected_paths.iter() {
            if path.starts_with(protected_path) {
                return true;
            }
        }

        false
    }

    /// Validate an API key
    fn validate_key(&self, key: &str) -> bool {
        self.valid_keys.contains(key)
    }
}

/// Tower Layer for API key authentication
#[derive(Clone)]
pub struct ApiKeyAuthLayer {
    auth: ApiKeyAuth,
}

impl<S> Layer<S> for ApiKeyAuthLayer {
    type Service = ApiKeyAuthMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        ApiKeyAuthMiddleware {
            inner,
            auth: self.auth.clone(),
        }
    }
}

/// API key authentication middleware service
#[derive(Clone)]
pub struct ApiKeyAuthMiddleware<S> {
    inner: S,
    auth: ApiKeyAuth,
}

impl<S> Service<Request<Body>> for ApiKeyAuthMiddleware<S>
where
    S: Service<Request<Body>, Response = Response> + Send + Clone + 'static,
    S::Future: Send + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let path = req.uri().path().to_string();

        // Check if this path requires auth
        if !self.auth.requires_auth(&path) {
            let mut inner = self.inner.clone();
            return Box::pin(async move { inner.call(req).await });
        }

        // Extract API key from request
        let api_key = extract_api_key(&req);

        let auth = self.auth.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            match api_key {
                Some(key) if auth.validate_key(&key) => {
                    debug!(path = %path, "API key authentication successful");
                    inner.call(req).await
                }
                Some(_) => {
                    warn!(path = %path, "Invalid API key");
                    Ok(unauthorized_response("Invalid API key"))
                }
                None => {
                    warn!(path = %path, "Missing API key");
                    Ok(unauthorized_response("API key required"))
                }
            }
        })
    }
}

/// Extract API key from request
fn extract_api_key<B>(req: &Request<B>) -> Option<String> {
    // Try Authorization: Bearer header
    if let Some(auth_header) = req.headers().get("authorization") {
        if let Ok(auth_str) = auth_header.to_str() {
            if let Some(token) = auth_str.strip_prefix("Bearer ") {
                return Some(token.to_string());
            }
        }
    }

    // Try X-API-Key header
    if let Some(api_key_header) = req.headers().get("x-api-key") {
        if let Ok(key) = api_key_header.to_str() {
            return Some(key.to_string());
        }
    }

    // Try query parameter
    if let Some(query) = req.uri().query() {
        for param in query.split('&') {
            if let Some(key) = param.strip_prefix("api_key=") {
                return Some(key.to_string());
            }
        }
    }

    None
}

/// Create unauthorized response
fn unauthorized_response(message: &str) -> Response {
    let body = serde_json::json!({
        "error": "unauthorized",
        "message": message,
        "status": 401
    });

    (
        StatusCode::UNAUTHORIZED,
        [
            ("content-type", "application/json"),
            ("www-authenticate", "Bearer"),
        ],
        body.to_string(),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_disabled() {
        let config = AuthConfig {
            enabled: false,
            api_keys: vec![],
            protected_paths: vec![],
            public_paths: vec![],
        };

        let auth = ApiKeyAuth::new(&config);
        assert!(!auth.requires_auth("/api/upload"));
    }

    #[test]
    fn test_auth_enabled_all_paths() {
        let config = AuthConfig {
            enabled: true,
            api_keys: vec!["secret123".to_string()],
            protected_paths: vec![], // Empty = all paths protected
            public_paths: vec!["/health".to_string()],
        };

        let auth = ApiKeyAuth::new(&config);

        // Protected paths
        assert!(auth.requires_auth("/api/upload"));
        assert!(auth.requires_auth("/api/upload/init"));

        // Public paths
        assert!(!auth.requires_auth("/health/live"));
        assert!(!auth.requires_auth("/health/ready"));

        // Key validation
        assert!(auth.validate_key("secret123"));
        assert!(!auth.validate_key("wrong_key"));
    }

    #[test]
    fn test_auth_specific_paths() {
        let config = AuthConfig {
            enabled: true,
            api_keys: vec!["key1".to_string()],
            protected_paths: vec!["/api/upload".to_string()],
            public_paths: vec![],
        };

        let auth = ApiKeyAuth::new(&config);

        // Only upload paths protected
        assert!(auth.requires_auth("/api/upload"));
        assert!(auth.requires_auth("/api/upload/init"));

        // Other paths not protected
        assert!(!auth.requires_auth("/m/123"));
        assert!(!auth.requires_auth("/health/live"));
    }
}

