//! Media entity model and related types.
//!
//! This module defines the core `Media` entity that represents uploaded files
//! stored in the system.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Media type classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaType {
    /// Image file (JPEG, PNG, GIF, WebP)
    Image,
    /// Video file (MP4, WebM, etc.)
    Video,
}

impl MediaType {
    /// Get media type from MIME type string
    pub fn from_mime(mime: &str) -> Option<Self> {
        if mime.starts_with("image/") {
            Some(Self::Image)
        } else if mime.starts_with("video/") {
            Some(Self::Video)
        } else {
            None
        }
    }

    /// Convert to database string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Video => "video",
        }
    }

    /// Parse from database string representation
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "image" => Some(Self::Image),
            "video" => Some(Self::Video),
            _ => None,
        }
    }
}

/// Media entity representing an uploaded file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Media {
    /// Unique identifier (UUID v4)
    pub id: Uuid,

    /// Original filename provided during upload
    pub original_filename: String,

    /// Original MIME type of the uploaded file
    pub original_mime_type: String,

    /// MIME type of the optimized version
    pub optimized_mime_type: String,

    /// Media type (image or video)
    pub media_type: MediaType,

    /// Original file size in bytes
    pub original_size: u64,

    /// Optimized file size in bytes
    pub optimized_size: u64,

    /// Image/video width in pixels
    pub width: u32,

    /// Image/video height in pixels
    pub height: u32,

    /// SHA-256 hash of the original file (for deduplication)
    pub content_hash: String,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// Last access timestamp (for potential cleanup)
    pub last_accessed_at: Option<DateTime<Utc>>,
}

impl Media {
    /// Create a new Media instance
    pub fn new(
        original_filename: String,
        original_mime_type: String,
        optimized_mime_type: String,
        original_size: u64,
        optimized_size: u64,
        width: u32,
        height: u32,
        content_hash: String,
    ) -> Self {
        let media_type = MediaType::from_mime(&original_mime_type).unwrap_or(MediaType::Image);

        Self {
            id: Uuid::new_v4(),
            original_filename,
            original_mime_type,
            optimized_mime_type,
            media_type,
            original_size,
            optimized_size,
            width,
            height,
            content_hash,
            created_at: Utc::now(),
            last_accessed_at: None,
        }
    }

    /// Get the filename for the original file in storage
    pub fn original_storage_filename(&self) -> String {
        let ext = self.get_extension_for_mime(&self.original_mime_type);
        format!("{}.{}", self.id, ext)
    }

    /// Get the filename for the optimized file in storage
    pub fn optimized_storage_filename(&self) -> String {
        let ext = self.get_extension_for_mime(&self.optimized_mime_type);
        format!("{}.{}", self.id, ext)
    }

    /// Get file extension for a MIME type
    fn get_extension_for_mime(&self, mime: &str) -> &'static str {
        match mime {
            "image/jpeg" => "jpg",
            "image/png" => "png",
            "image/gif" => "gif",
            "image/webp" => "webp",
            "video/mp4" => "mp4",
            "video/webm" => "webm",
            "video/quicktime" => "mov",
            _ => "bin",
        }
    }
}

/// Response DTO for successful upload
#[derive(Debug, Serialize)]
pub struct UploadResponse {
    /// Unique media ID
    pub id: Uuid,

    /// Public URL to access the media
    pub url: String,

    /// URL to access the original (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_url: Option<String>,

    /// Media type
    pub media_type: MediaType,

    /// MIME type of the served file
    pub mime_type: String,

    /// File size in bytes
    pub size: u64,

    /// Width in pixels
    pub width: u32,

    /// Height in pixels
    pub height: u32,
}

impl UploadResponse {
    /// Create upload response from Media entity and base URL
    pub fn from_media(media: &Media, base_url: &str, include_original: bool) -> Self {
        let url = format!("{}/m/{}", base_url, media.id);
        let original_url = if include_original {
            Some(format!("{}/m/{}/original", base_url, media.id))
        } else {
            None
        };

        Self {
            id: media.id,
            url,
            original_url,
            media_type: media.media_type,
            mime_type: media.optimized_mime_type.clone(),
            size: media.optimized_size,
            width: media.width,
            height: media.height,
        }
    }
}

/// Response DTO for media info (admin API)
#[derive(Debug, Serialize)]
pub struct MediaInfoResponse {
    /// Unique media ID
    pub id: Uuid,

    /// Original filename
    pub original_filename: String,

    /// Original MIME type
    pub original_mime_type: String,

    /// Optimized MIME type
    pub optimized_mime_type: String,

    /// Media type
    pub media_type: MediaType,

    /// Original file size
    pub original_size: u64,

    /// Optimized file size
    pub optimized_size: u64,

    /// Width in pixels
    pub width: u32,

    /// Height in pixels
    pub height: u32,

    /// Content hash
    pub content_hash: String,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// Last access timestamp
    pub last_accessed_at: Option<DateTime<Utc>>,

    /// Public URL
    pub url: String,
}

impl MediaInfoResponse {
    /// Create info response from Media entity and base URL
    pub fn from_media(media: &Media, base_url: &str) -> Self {
        Self {
            id: media.id,
            original_filename: media.original_filename.clone(),
            original_mime_type: media.original_mime_type.clone(),
            optimized_mime_type: media.optimized_mime_type.clone(),
            media_type: media.media_type,
            original_size: media.original_size,
            optimized_size: media.optimized_size,
            width: media.width,
            height: media.height,
            content_hash: media.content_hash.clone(),
            created_at: media.created_at,
            last_accessed_at: media.last_accessed_at,
            url: format!("{}/m/{}", base_url, media.id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_media_type_from_mime() {
        assert_eq!(MediaType::from_mime("image/jpeg"), Some(MediaType::Image));
        assert_eq!(MediaType::from_mime("image/png"), Some(MediaType::Image));
        assert_eq!(MediaType::from_mime("video/mp4"), Some(MediaType::Video));
        assert_eq!(MediaType::from_mime("text/plain"), None);
    }

    #[test]
    fn test_media_storage_filenames() {
        let media = Media::new(
            "test.jpg".to_string(),
            "image/jpeg".to_string(),
            "image/webp".to_string(),
            1000,
            500,
            100,
            100,
            "abc123".to_string(),
        );

        assert!(media.original_storage_filename().ends_with(".jpg"));
        assert!(media.optimized_storage_filename().ends_with(".webp"));
    }
}

