//! Error types for the media upload server.
//!
//! This module defines a unified error handling system using `thiserror`.
//! All errors are converted to appropriate HTTP responses automatically.
//!
//! # Error Categories
//!
//! - **Client errors (4xx)**: Invalid input, validation failures, not found
//! - **Server errors (5xx)**: Internal failures, I/O errors, processing errors
//!
//! # Example
//!
//! ```rust,ignore
//! use crate::error::{AppError, Result};
//!
//! fn process_upload(data: &[u8]) -> Result<String> {
//!     if data.is_empty() {
//!         return Err(AppError::validation("Upload data is empty"));
//!     }
//!     Ok("processed".to_string())
//! }
//! ```

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use thiserror::Error;

/// Result type alias using AppError
pub type Result<T> = std::result::Result<T, AppError>;

/// Application error type
///
/// This enum represents all possible errors that can occur in the application.
/// Each variant is mapped to an appropriate HTTP status code.
#[derive(Debug, Error)]
pub enum AppError {
    // -------------------------------------------------------------------------
    // Client Errors (4xx)
    // -------------------------------------------------------------------------
    /// Invalid request or validation error
    #[error("Validation error: {0}")]
    Validation(String),

    /// Resource not found
    #[error("Not found: {0}")]
    NotFound(String),

    /// Unsupported media type
    #[error("Unsupported media type: {0}")]
    UnsupportedMediaType(String),

    /// Request payload too large
    #[error("Payload too large: {0}")]
    PayloadTooLarge(String),

    /// Rate limit exceeded
    #[error("Rate limit exceeded: {0}")]
    RateLimitExceeded(String),

    /// Upload session expired or invalid
    #[error("Upload session error: {0}")]
    UploadSessionError(String),

    /// Authentication required
    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    /// Token is locked by admin (RexPump)
    #[error("Token locked: {0}")]
    TokenLocked(String),

    /// Rate limit for token updates exceeded (RexPump)
    #[error("Update cooldown: {0}")]
    UpdateCooldown(String),

    /// Signature verification failed (RexPump)
    #[error("Invalid signature: {0}")]
    InvalidSignature(String),

    /// Not authorized to perform action (RexPump)
    #[error("Not authorized: {0}")]
    NotAuthorized(String),

    // -------------------------------------------------------------------------
    // Server Errors (5xx)
    // -------------------------------------------------------------------------
    /// Internal server error
    #[error("Internal error: {0}")]
    Internal(String),

    /// I/O error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Database error
    #[error("Database error: {0}")]
    Database(#[from] rocksdb::Error),

    /// Image processing error
    #[error("Image processing error: {0}")]
    ImageProcessing(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),
}

impl AppError {
    // -------------------------------------------------------------------------
    // Convenience constructors
    // -------------------------------------------------------------------------

    /// Create a validation error
    pub fn validation<S: Into<String>>(msg: S) -> Self {
        Self::Validation(msg.into())
    }

    /// Create a not found error
    pub fn not_found<S: Into<String>>(msg: S) -> Self {
        Self::NotFound(msg.into())
    }

    /// Create an unsupported media type error
    pub fn unsupported_media_type<S: Into<String>>(msg: S) -> Self {
        Self::UnsupportedMediaType(msg.into())
    }

    /// Create a payload too large error
    pub fn payload_too_large<S: Into<String>>(msg: S) -> Self {
        Self::PayloadTooLarge(msg.into())
    }

    /// Create a rate limit exceeded error
    pub fn rate_limit_exceeded<S: Into<String>>(msg: S) -> Self {
        Self::RateLimitExceeded(msg.into())
    }

    /// Create an upload session error
    pub fn upload_session<S: Into<String>>(msg: S) -> Self {
        Self::UploadSessionError(msg.into())
    }

    /// Create an internal error
    pub fn internal<S: Into<String>>(msg: S) -> Self {
        Self::Internal(msg.into())
    }

    /// Create an image processing error
    pub fn image_processing<S: Into<String>>(msg: S) -> Self {
        Self::ImageProcessing(msg.into())
    }

    /// Create a config error
    pub fn config<S: Into<String>>(msg: S) -> Self {
        Self::Config(msg.into())
    }

    /// Get the HTTP status code for this error
    pub fn status_code(&self) -> StatusCode {
        match self {
            // 4xx Client Errors
            Self::Validation(_) => StatusCode::BAD_REQUEST,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::UnsupportedMediaType(_) => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Self::PayloadTooLarge(_) => StatusCode::PAYLOAD_TOO_LARGE,
            Self::RateLimitExceeded(_) => StatusCode::TOO_MANY_REQUESTS,
            Self::UploadSessionError(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::TokenLocked(_) => StatusCode::FORBIDDEN,
            Self::UpdateCooldown(_) => StatusCode::TOO_MANY_REQUESTS,
            Self::InvalidSignature(_) => StatusCode::BAD_REQUEST,
            Self::NotAuthorized(_) => StatusCode::FORBIDDEN,

            // 5xx Server Errors
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::ImageProcessing(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Config(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Check if this is a client error (4xx)
    pub fn is_client_error(&self) -> bool {
        self.status_code().is_client_error()
    }

    /// Check if this is a server error (5xx)
    pub fn is_server_error(&self) -> bool {
        self.status_code().is_server_error()
    }
}

/// Error response body sent to clients
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    /// Error type/code
    pub error: String,
    /// Human-readable error message
    pub message: String,
    /// HTTP status code
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
}

impl ErrorResponse {
    /// Create a new error response
    pub fn new(error: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            message: message.into(),
            status: None,
        }
    }

    /// Add status code to the response
    pub fn with_status(mut self, status: StatusCode) -> Self {
        self.status = Some(status.as_u16());
        self
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status_code();

        // Log server errors
        if self.is_server_error() {
            tracing::error!(error = %self, "Server error occurred");
        } else {
            tracing::debug!(error = %self, "Client error occurred");
        }

        // Create error response
        let error_type = match &self {
            Self::Validation(_) => "validation_error",
            Self::NotFound(_) => "not_found",
            Self::UnsupportedMediaType(_) => "unsupported_media_type",
            Self::PayloadTooLarge(_) => "payload_too_large",
            Self::RateLimitExceeded(_) => "rate_limit_exceeded",
            Self::UploadSessionError(_) => "upload_session_error",
            Self::Unauthorized(_) => "unauthorized",
            Self::TokenLocked(_) => "token_locked",
            Self::UpdateCooldown(_) => "update_cooldown",
            Self::InvalidSignature(_) => "invalid_signature",
            Self::NotAuthorized(_) => "not_authorized",
            Self::Internal(_) => "internal_error",
            Self::Io(_) => "io_error",
            Self::Database(_) => "database_error",
            Self::ImageProcessing(_) => "image_processing_error",
            Self::Config(_) => "config_error",
        };

        // For server errors, don't expose internal details to clients
        let message = if self.is_server_error() {
            "An internal error occurred. Please try again later.".to_string()
        } else {
            self.to_string()
        };

        let body = ErrorResponse::new(error_type, message).with_status(status);

        (status, Json(body)).into_response()
    }
}

// -------------------------------------------------------------------------
// Error conversions from external crates
// -------------------------------------------------------------------------

impl From<image::ImageError> for AppError {
    fn from(err: image::ImageError) -> Self {
        Self::ImageProcessing(err.to_string())
    }
}

impl From<uuid::Error> for AppError {
    fn from(err: uuid::Error) -> Self {
        Self::Validation(format!("Invalid UUID: {}", err))
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        Self::Internal(format!("Serialization error: {}", err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_status_codes() {
        assert_eq!(
            AppError::validation("test").status_code(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            AppError::not_found("test").status_code(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            AppError::internal("test").status_code(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn test_error_categories() {
        assert!(AppError::validation("test").is_client_error());
        assert!(!AppError::validation("test").is_server_error());
        assert!(AppError::internal("test").is_server_error());
        assert!(!AppError::internal("test").is_client_error());
    }
}

