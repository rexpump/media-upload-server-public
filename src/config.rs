//! Configuration module for the media upload server.
//!
//! This module handles loading and validating configuration from TOML files.
//! Configuration can be loaded from a file path or from default locations.
//!
//! # Configuration Sources (in order of priority)
//! 1. `config.local.toml` - Local overrides (gitignored)
//! 2. `config.toml` - Main configuration file
//! 3. Default values
//!
//! # Example
//! ```rust,ignore
//! let config = Config::load("config.toml")?;
//! println!("Server will listen on {}:{}", config.server.host, config.server.port);
//! ```

use serde::Deserialize;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Configuration loading errors
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Failed to read configuration file: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Failed to parse configuration: {0}")]
    ParseError(#[from] toml::de::Error),

    #[error("Configuration validation failed: {0}")]
    ValidationError(String),
}

/// Root configuration structure
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub storage: StorageConfig,
    pub upload: UploadConfig,
    pub processing: ProcessingConfig,
    pub rate_limit: RateLimitConfig,
    pub logging: LoggingConfig,
    #[serde(default)]
    pub auth: AuthConfig,
}

/// Authentication configuration
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AuthConfig {
    /// Whether authentication is enabled for uploads
    #[serde(default)]
    pub enabled: bool,

    /// List of valid API keys
    #[serde(default)]
    pub api_keys: Vec<String>,

    /// Paths that require authentication (empty = all paths except public)
    #[serde(default)]
    pub protected_paths: Vec<String>,

    /// Paths that are always public (bypass auth)
    #[serde(default)]
    pub public_paths: Vec<String>,
}

/// Server configuration
#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    /// Host to bind the public API to
    pub host: String,
    /// Port for the public API
    pub port: u16,
    /// Host to bind the admin API to (should be localhost)
    pub admin_host: String,
    /// Port for the admin API
    pub admin_port: u16,
    /// Base URL for generating media URLs
    pub base_url: String,
    /// Request timeout in seconds
    pub request_timeout: u64,
    /// Maximum concurrent connections
    pub max_connections: usize,
    /// Cache-Control max-age in seconds (default: 31536000 = 1 year)
    pub cache_max_age: u64,
    /// Cleanup interval for expired sessions in seconds
    pub cleanup_interval_seconds: u64,
}

/// Storage configuration
#[derive(Debug, Clone, Deserialize)]
pub struct StorageConfig {
    /// Base directory for all data
    pub data_dir: PathBuf,
    /// Directory for original files (relative to data_dir)
    pub originals_dir: String,
    /// Directory for optimized files (relative to data_dir)
    pub optimized_dir: String,
    /// Directory for temporary uploads (relative to data_dir)
    pub temp_dir: String,
    /// Number of directory nesting levels for file storage (0-4)
    /// Each level uses 2 hex characters from UUID.
    /// - 0: files stored flat (originals/{uuid}.ext)
    /// - 1: one level (originals/ab/{uuid}.ext) - 256 subdirs
    /// - 2: two levels (originals/ab/cd/{uuid}.ext) - 65,536 subdirs (recommended)
    /// - 3: three levels - 16,777,216 subdirs
    /// - 4: four levels - 4,294,967,296 subdirs
    #[serde(default = "default_directory_levels")]
    pub directory_levels: u8,
    /// Database file (legacy, not used - RocksDB stores in data_dir/rocksdb)
    #[serde(default)]
    pub database_file: String,
}

fn default_directory_levels() -> u8 {
    2
}

impl StorageConfig {
    /// Get the full path to the originals directory
    pub fn originals_path(&self) -> PathBuf {
        self.data_dir.join(&self.originals_dir)
    }

    /// Get the full path to the optimized directory
    pub fn optimized_path(&self) -> PathBuf {
        self.data_dir.join(&self.optimized_dir)
    }

    /// Get the full path to the temp directory
    pub fn temp_path(&self) -> PathBuf {
        self.data_dir.join(&self.temp_dir)
    }

}

/// Upload configuration
#[derive(Debug, Clone, Deserialize)]
pub struct UploadConfig {
    /// Maximum file size for simple upload (bytes)
    pub max_simple_upload_size: u64,
    /// Maximum file size for chunked upload (bytes)
    pub max_chunked_upload_size: u64,
    /// Chunk size for chunked uploads (bytes)
    pub chunk_size: u64,
    /// Allowed MIME types for images
    pub allowed_image_types: Vec<String>,
    /// Allowed MIME types for videos
    pub allowed_video_types: Vec<String>,
    /// Upload session timeout in seconds
    pub upload_session_timeout: u64,
}

impl UploadConfig {
    /// Check if a MIME type is allowed for images
    pub fn is_allowed_image_type(&self, mime_type: &str) -> bool {
        self.allowed_image_types.iter().any(|t| t == mime_type)
    }

    /// Check if a MIME type is allowed for videos
    pub fn is_allowed_video_type(&self, mime_type: &str) -> bool {
        self.allowed_video_types.iter().any(|t| t == mime_type)
    }

    /// Check if a MIME type is allowed (image or video)
    pub fn is_allowed_type(&self, mime_type: &str) -> bool {
        self.is_allowed_image_type(mime_type) || self.is_allowed_video_type(mime_type)
    }
}

/// Image/video processing configuration
#[derive(Debug, Clone, Deserialize)]
pub struct ProcessingConfig {
    /// Output format for optimized images (webp, jpeg, png)
    pub output_format: String,
    /// Quality for lossy formats (0-100)
    pub output_quality: u8,
    /// Maximum image dimension (width or height)
    pub max_image_dimension: u32,
    /// Whether to keep original files
    pub keep_originals: bool,
    /// Whether to strip EXIF data
    pub strip_exif: bool,
}

impl ProcessingConfig {
    /// Get the output MIME type based on format
    pub fn output_mime_type(&self) -> &'static str {
        match self.output_format.as_str() {
            "webp" => "image/webp",
            "jpeg" | "jpg" => "image/jpeg",
            "png" => "image/png",
            _ => "image/webp",
        }
    }

    /// Get the file extension for output format
    pub fn output_extension(&self) -> &'static str {
        match self.output_format.as_str() {
            "webp" => "webp",
            "jpeg" | "jpg" => "jpg",
            "png" => "png",
            _ => "webp",
        }
    }
}

/// Rate limiting configuration
#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitConfig {
    /// Enable rate limiting
    pub enabled: bool,
    /// Maximum requests per window
    pub requests_per_window: u32,
    /// Window duration in seconds
    pub window_seconds: u64,
    /// Maximum uploads per IP per window
    pub uploads_per_window: u32,
}

/// Logging configuration
#[derive(Debug, Clone, Deserialize)]
pub struct LoggingConfig {
    /// Log level: trace, debug, info, warn, error
    pub level: String,
    /// Log format: "pretty" or "json"
    pub format: String,
    /// Log to file (optional)
    pub file: String,
}

impl Config {
    /// Load configuration from a file path
    ///
    /// # Arguments
    /// * `path` - Path to the TOML configuration file
    ///
    /// # Errors
    /// Returns `ConfigError` if the file cannot be read or parsed
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let contents = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&contents)?;
        config.validate()?;
        Ok(config)
    }

    /// Load configuration from default locations
    ///
    /// Tries to load from:
    /// 1. `config.local.toml` (if exists)
    /// 2. `config.toml`
    ///
    /// # Errors
    /// Returns `ConfigError` if no configuration file is found
    pub fn load_default() -> Result<Self, ConfigError> {
        // Try local config first
        if Path::new("config.local.toml").exists() {
            return Self::load("config.local.toml");
        }

        // Fall back to main config
        if Path::new("config.toml").exists() {
            return Self::load("config.toml");
        }

        Err(ConfigError::ValidationError(
            "No configuration file found. Expected config.toml or config.local.toml".to_string(),
        ))
    }

    /// Validate the configuration
    fn validate(&self) -> Result<(), ConfigError> {
        // Validate output quality
        if self.processing.output_quality > 100 {
            return Err(ConfigError::ValidationError(
                "output_quality must be between 0 and 100".to_string(),
            ));
        }

        // Validate output format
        let valid_formats = ["webp", "jpeg", "jpg", "png"];
        if !valid_formats.contains(&self.processing.output_format.as_str()) {
            return Err(ConfigError::ValidationError(format!(
                "output_format must be one of: {:?}",
                valid_formats
            )));
        }

        // Validate chunk size
        if self.upload.chunk_size < 1024 {
            return Err(ConfigError::ValidationError(
                "chunk_size must be at least 1024 bytes".to_string(),
            ));
        }

        // Validate that max_chunked_upload_size >= max_simple_upload_size
        if self.upload.max_chunked_upload_size < self.upload.max_simple_upload_size {
            return Err(ConfigError::ValidationError(
                "max_chunked_upload_size must be >= max_simple_upload_size".to_string(),
            ));
        }

        // Validate base_url doesn't have trailing slash
        if self.server.base_url.ends_with('/') {
            return Err(ConfigError::ValidationError(
                "base_url should not have a trailing slash".to_string(),
            ));
        }

        // Validate directory_levels (0-4)
        if self.storage.directory_levels > 4 {
            return Err(ConfigError::ValidationError(
                "directory_levels must be between 0 and 4".to_string(),
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_paths() {
        let storage = StorageConfig {
            data_dir: PathBuf::from("/data"),
            originals_dir: "originals".to_string(),
            optimized_dir: "optimized".to_string(),
            temp_dir: "temp".to_string(),
            directory_levels: 2,
            database_file: String::new(),
        };

        assert_eq!(storage.originals_path(), PathBuf::from("/data/originals"));
        assert_eq!(storage.optimized_path(), PathBuf::from("/data/optimized"));
        assert_eq!(storage.temp_path(), PathBuf::from("/data/temp"));
    }

    #[test]
    fn test_allowed_types() {
        let upload = UploadConfig {
            max_simple_upload_size: 1024,
            max_chunked_upload_size: 2048,
            chunk_size: 512,
            allowed_image_types: vec!["image/jpeg".to_string(), "image/png".to_string()],
            allowed_video_types: vec!["video/mp4".to_string()],
            upload_session_timeout: 3600,
        };

        assert!(upload.is_allowed_image_type("image/jpeg"));
        assert!(!upload.is_allowed_image_type("image/gif"));
        assert!(upload.is_allowed_video_type("video/mp4"));
        assert!(upload.is_allowed_type("image/jpeg"));
        assert!(upload.is_allowed_type("video/mp4"));
        assert!(!upload.is_allowed_type("text/plain"));
    }
}

