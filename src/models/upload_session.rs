//! Upload session model for chunked uploads.
//!
//! This module defines the `UploadSession` entity that tracks the state
//! of chunked/resumable uploads.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Status of an upload session
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UploadSessionStatus {
    /// Upload in progress, accepting chunks
    InProgress,
    /// All chunks received, processing
    Processing,
    /// Upload completed successfully
    Completed,
    /// Upload failed
    Failed,
    /// Upload expired/timed out
    Expired,
    /// Upload cancelled by client
    Cancelled,
}

impl UploadSessionStatus {
    /// Convert to database string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InProgress => "in_progress",
            Self::Processing => "processing",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Expired => "expired",
            Self::Cancelled => "cancelled",
        }
    }

    /// Parse from database string representation
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "in_progress" => Some(Self::InProgress),
            "processing" => Some(Self::Processing),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            "expired" => Some(Self::Expired),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }

    /// Check if the session can accept more chunks
    pub fn can_accept_chunks(&self) -> bool {
        matches!(self, Self::InProgress)
    }

    /// Check if the session is in a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Expired | Self::Cancelled
        )
    }
}

/// Upload session for tracking chunked uploads
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadSession {
    /// Unique session identifier
    pub id: Uuid,

    /// Original filename
    pub filename: String,

    /// MIME type of the file being uploaded
    pub mime_type: String,

    /// Total expected file size in bytes
    pub total_size: u64,

    /// Number of bytes received so far
    pub received_bytes: u64,

    /// Expected chunk size
    pub chunk_size: u64,

    /// Current session status
    pub status: UploadSessionStatus,

    /// Error message if failed
    pub error_message: Option<String>,

    /// Associated media ID (set when completed)
    pub media_id: Option<Uuid>,

    /// Session creation timestamp
    pub created_at: DateTime<Utc>,

    /// Last activity timestamp
    pub updated_at: DateTime<Utc>,

    /// Expiration timestamp
    pub expires_at: DateTime<Utc>,
}

impl UploadSession {
    /// Create a new upload session
    pub fn new(
        filename: String,
        mime_type: String,
        total_size: u64,
        chunk_size: u64,
        timeout_seconds: u64,
    ) -> Self {
        let now = Utc::now();
        let expires_at = now + chrono::Duration::seconds(timeout_seconds as i64);

        Self {
            id: Uuid::new_v4(),
            filename,
            mime_type,
            total_size,
            received_bytes: 0,
            chunk_size,
            status: UploadSessionStatus::InProgress,
            error_message: None,
            media_id: None,
            created_at: now,
            updated_at: now,
            expires_at,
        }
    }

    /// Calculate the expected number of chunks
    pub fn total_chunks(&self) -> u64 {
        (self.total_size + self.chunk_size - 1) / self.chunk_size
    }

    /// Calculate the number of chunks received
    pub fn received_chunks(&self) -> u64 {
        (self.received_bytes + self.chunk_size - 1) / self.chunk_size
    }

    /// Calculate upload progress as a percentage
    pub fn progress_percent(&self) -> f64 {
        if self.total_size == 0 {
            return 100.0;
        }
        (self.received_bytes as f64 / self.total_size as f64) * 100.0
    }

    /// Check if all chunks have been received
    pub fn is_complete(&self) -> bool {
        self.received_bytes >= self.total_size
    }

    /// Check if the session has expired
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    /// Add received bytes and update timestamp
    pub fn add_received_bytes(&mut self, bytes: u64) {
        self.received_bytes += bytes;
        self.updated_at = Utc::now();
    }

    /// Mark session as processing
    pub fn mark_processing(&mut self) {
        self.status = UploadSessionStatus::Processing;
        self.updated_at = Utc::now();
    }

    /// Mark session as completed with media ID
    pub fn mark_completed(&mut self, media_id: Uuid) {
        self.status = UploadSessionStatus::Completed;
        self.media_id = Some(media_id);
        self.updated_at = Utc::now();
    }

    /// Mark session as failed with error message
    pub fn mark_failed(&mut self, error: impl Into<String>) {
        self.status = UploadSessionStatus::Failed;
        self.error_message = Some(error.into());
        self.updated_at = Utc::now();
    }

    /// Mark session as expired
    pub fn mark_expired(&mut self) {
        self.status = UploadSessionStatus::Expired;
        self.updated_at = Utc::now();
    }

    /// Mark session as cancelled
    pub fn mark_cancelled(&mut self) {
        self.status = UploadSessionStatus::Cancelled;
        self.updated_at = Utc::now();
    }
}

/// Request DTO for initiating a chunked upload
#[derive(Debug, Deserialize)]
pub struct InitUploadRequest {
    /// Original filename
    pub filename: String,

    /// MIME type of the file
    pub mime_type: String,

    /// Total file size in bytes
    pub total_size: u64,
}

/// Response DTO for upload session status
#[derive(Debug, Serialize)]
pub struct UploadSessionResponse {
    /// Session ID
    pub id: Uuid,

    /// Session status
    pub status: UploadSessionStatus,

    /// Bytes received so far
    pub received_bytes: u64,

    /// Total expected bytes
    pub total_size: u64,

    /// Progress percentage
    pub progress: f64,

    /// Expected chunk size
    pub chunk_size: u64,

    /// Next expected byte offset
    pub next_offset: u64,

    /// Expiration timestamp
    pub expires_at: DateTime<Utc>,

    /// Error message if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    /// Media ID if completed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_id: Option<Uuid>,

    /// Media URL if completed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_url: Option<String>,
}

impl UploadSessionResponse {
    /// Create response from upload session
    pub fn from_session(session: &UploadSession, base_url: Option<&str>) -> Self {
        let media_url = session.media_id.and_then(|id| {
            base_url.map(|url| format!("{}/m/{}", url, id))
        });

        Self {
            id: session.id,
            status: session.status,
            received_bytes: session.received_bytes,
            total_size: session.total_size,
            progress: session.progress_percent(),
            chunk_size: session.chunk_size,
            next_offset: session.received_bytes,
            expires_at: session.expires_at,
            error: session.error_message.clone(),
            media_id: session.media_id,
            media_url,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_progress() {
        let mut session = UploadSession::new(
            "test.jpg".to_string(),
            "image/jpeg".to_string(),
            1000,
            100,
            3600,
        );

        assert_eq!(session.progress_percent(), 0.0);
        assert_eq!(session.total_chunks(), 10);
        assert_eq!(session.received_chunks(), 0);

        session.add_received_bytes(500);
        assert_eq!(session.progress_percent(), 50.0);
        assert_eq!(session.received_chunks(), 5);

        session.add_received_bytes(500);
        assert!(session.is_complete());
        assert_eq!(session.progress_percent(), 100.0);
    }

    #[test]
    fn test_session_status_transitions() {
        let mut session = UploadSession::new(
            "test.jpg".to_string(),
            "image/jpeg".to_string(),
            1000,
            100,
            3600,
        );

        assert!(session.status.can_accept_chunks());
        assert!(!session.status.is_terminal());

        session.mark_processing();
        assert!(!session.status.can_accept_chunks());
        assert!(!session.status.is_terminal());

        let media_id = Uuid::new_v4();
        session.mark_completed(media_id);
        assert!(session.status.is_terminal());
        assert_eq!(session.media_id, Some(media_id));
    }
}

