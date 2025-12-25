//! Application state management.
//!
//! This module defines the shared application state that is accessible
//! from all request handlers via Axum's State extractor.
//!
//! # Usage
//!
//! ```rust,ignore
//! async fn handler(State(state): State<AppState>) -> impl IntoResponse {
//!     let media = state.db.get_media(id)?;
//!     // ...
//! }
//! ```

use crate::config::Config;
use crate::error::Result;
use crate::services::{DatabaseService, ImageProcessor, StorageService};
use std::sync::Arc;

/// Shared application state
///
/// This struct holds all shared resources that handlers need access to.
/// It's wrapped in `Arc` and cloned into each request handler.
#[derive(Clone)]
pub struct AppState {
    /// Application configuration
    pub config: Arc<Config>,

    /// Database service for metadata operations
    pub db: Arc<DatabaseService>,

    /// Storage service for file operations
    pub storage: Arc<StorageService>,

    /// Image processor for format conversion
    pub image_processor: Arc<ImageProcessor>,
}

impl AppState {
    /// Create a new application state
    ///
    /// # Arguments
    /// * `config` - Application configuration
    ///
    /// # Errors
    /// Returns error if services cannot be initialized
    pub async fn new(config: Config) -> Result<Self> {
        // Initialize services
        let db = DatabaseService::new(&config.storage)?;
        let storage = StorageService::new(&config.storage).await?;
        let image_processor = ImageProcessor::new(&config.processing);

        Ok(Self {
            config: Arc::new(config),
            db: Arc::new(db),
            storage: Arc::new(storage),
            image_processor: Arc::new(image_processor),
        })
    }

    /// Get the base URL for media URLs
    pub fn base_url(&self) -> &str {
        &self.config.server.base_url
    }

    /// Check if originals should be kept
    pub fn keep_originals(&self) -> bool {
        self.config.processing.keep_originals
    }

    /// Get the maximum simple upload size
    pub fn max_upload_size(&self) -> u64 {
        self.config.upload.max_simple_upload_size
    }

    /// Get the chunk size for chunked uploads
    pub fn chunk_size(&self) -> u64 {
        self.config.upload.chunk_size
    }

    /// Get upload session timeout
    pub fn upload_session_timeout(&self) -> u64 {
        self.config.upload.upload_session_timeout
    }

    /// Get cache max age in seconds
    pub fn cache_max_age(&self) -> u64 {
        self.config.server.cache_max_age
    }

    /// Get cleanup interval in seconds
    pub fn cleanup_interval(&self) -> u64 {
        self.config.server.cleanup_interval_seconds
    }

    /// Get output format extension (e.g., "webp")
    pub fn output_extension(&self) -> &'static str {
        self.config.processing.output_extension()
    }

    /// Get output MIME type (e.g., "image/webp")
    pub fn output_mime_type(&self) -> &'static str {
        self.config.processing.output_mime_type()
    }

    /// Check if MIME type is allowed
    pub fn is_allowed_mime_type(&self, mime_type: &str) -> bool {
        self.config.upload.is_allowed_type(mime_type)
    }
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("config", &"<Config>")
            .field("db", &"<DatabaseService>")
            .field("storage", &"<StorageService>")
            .field("image_processor", &"<ImageProcessor>")
            .finish()
    }
}

