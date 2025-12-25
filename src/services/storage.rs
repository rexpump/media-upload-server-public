//! Storage service for file operations.
//!
//! This module handles all file system operations including:
//! - Saving original and optimized files
//! - Managing temporary upload files
//! - Cleaning up expired sessions
//!
//! # File Organization
//!
//! Files are organized in a hierarchical structure using the first 4 characters
//! of the UUID (2 levels of 2 characters each) to avoid having too many files
//! in a single directory. This prevents filesystem performance degradation
//! with large numbers of files.
//!
//! ```text
//! data/
//! ├── originals/           # Original uploaded files
//! │   └── ab/cd/           # First 2 chars / next 2 chars of UUID
//! │       └── abcd1234-...-5678.jpg
//! ├── optimized/           # Optimized/converted files
//! │   └── ab/cd/
//! │       └── abcd1234-...-5678.webp
//! └── temp/                # Temporary chunked upload files
//!     └── {session_id}/
//!         ├── chunk_0
//!         ├── chunk_1
//!         └── ...
//! ```
//!
//! With 2 levels of 2 hex characters, we get 256 × 256 = 65,536 possible
//! subdirectory combinations, ensuring even distribution of millions of files.

use crate::config::StorageConfig;
use crate::error::{AppError, Result};
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Service for managing file storage operations
#[derive(Debug, Clone)]
pub struct StorageService {
    /// Path to originals directory
    originals_dir: PathBuf,
    /// Path to optimized files directory
    optimized_dir: PathBuf,
    /// Path to temporary files directory
    temp_dir: PathBuf,
    /// Number of directory nesting levels (0-4)
    directory_levels: u8,
}

impl StorageService {
    /// Create a new storage service and initialize directories
    ///
    /// # Arguments
    /// * `config` - Storage configuration
    ///
    /// # Errors
    /// Returns error if directories cannot be created
    pub async fn new(config: &StorageConfig) -> Result<Self> {
        let service = Self {
            originals_dir: config.originals_path(),
            optimized_dir: config.optimized_path(),
            temp_dir: config.temp_path(),
            directory_levels: config.directory_levels,
        };

        // Ensure all directories exist
        service.init_directories().await?;

        info!(
            originals = %service.originals_dir.display(),
            optimized = %service.optimized_dir.display(),
            temp = %service.temp_dir.display(),
            directory_levels = service.directory_levels,
            "Storage service initialized"
        );

        Ok(service)
    }

    /// Initialize storage directories
    async fn init_directories(&self) -> Result<()> {
        for dir in [&self.originals_dir, &self.optimized_dir, &self.temp_dir] {
            if !dir.exists() {
                fs::create_dir_all(dir).await?;
                debug!(path = %dir.display(), "Created storage directory");
            }
        }
        Ok(())
    }

    /// Generate subdirectory path based on UUID and configured nesting levels
    ///
    /// For UUID "550e8400-e29b-41d4-a716-446655440000":
    /// - level 0: ""
    /// - level 1: "55"
    /// - level 2: "55/0e"
    /// - level 3: "55/0e/84"
    /// - level 4: "55/0e/84/00"
    fn subdir_path(&self, id: Uuid) -> PathBuf {
        if self.directory_levels == 0 {
            return PathBuf::new();
        }

        let hex = id.as_simple().to_string(); // 32 hex characters without dashes
        let mut path = PathBuf::new();

        for level in 0..self.directory_levels.min(4) {
            let start = (level as usize) * 2;
            let end = start + 2;
            if end <= hex.len() {
                path.push(&hex[start..end]);
            }
        }

        path
    }

    /// Build full file path with subdirectories
    fn build_file_path(&self, base_dir: &Path, id: Uuid, extension: &str) -> PathBuf {
        let subdir = self.subdir_path(id);
        let filename = format!("{}.{}", id, extension);
        base_dir.join(subdir).join(filename)
    }

    /// Ensure subdirectory exists before saving a file
    async fn ensure_subdir(&self, file_path: &Path) -> Result<()> {
        if let Some(parent) = file_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).await?;
                debug!(path = %parent.display(), "Created subdirectory");
            }
        }
        Ok(())
    }

    // =========================================================================
    // Original files
    // =========================================================================

    /// Save an original file
    ///
    /// # Arguments
    /// * `id` - Media UUID
    /// * `extension` - File extension (e.g., "jpg", "png")
    /// * `data` - File contents
    ///
    /// # Returns
    /// Path to the saved file
    pub async fn save_original(
        &self,
        id: Uuid,
        extension: &str,
        data: &[u8],
    ) -> Result<PathBuf> {
        let path = self.build_file_path(&self.originals_dir, id, extension);

        // Ensure subdirectory exists
        self.ensure_subdir(&path).await?;

        fs::write(&path, data).await?;

        debug!(
            id = %id,
            path = %path.display(),
            size = data.len(),
            "Saved original file"
        );

        Ok(path)
    }

    /// Get path to an original file
    pub fn original_path(&self, id: Uuid, extension: &str) -> PathBuf {
        self.build_file_path(&self.originals_dir, id, extension)
    }

    /// Read an original file
    pub async fn read_original(&self, id: Uuid, extension: &str) -> Result<Vec<u8>> {
        let path = self.original_path(id, extension);

        if !path.exists() {
            return Err(AppError::not_found(format!("Original file not found: {}", id)));
        }

        Ok(fs::read(&path).await?)
    }

    /// Delete an original file
    pub async fn delete_original(&self, id: Uuid, extension: &str) -> Result<()> {
        let path = self.original_path(id, extension);

        if path.exists() {
            fs::remove_file(&path).await?;
            debug!(id = %id, path = %path.display(), "Deleted original file");
        }

        Ok(())
    }

    // =========================================================================
    // Optimized files
    // =========================================================================

    /// Save an optimized file
    ///
    /// # Arguments
    /// * `id` - Media UUID
    /// * `extension` - File extension (e.g., "webp")
    /// * `data` - File contents
    ///
    /// # Returns
    /// Path to the saved file
    pub async fn save_optimized(
        &self,
        id: Uuid,
        extension: &str,
        data: &[u8],
    ) -> Result<PathBuf> {
        let path = self.build_file_path(&self.optimized_dir, id, extension);

        // Ensure subdirectory exists
        self.ensure_subdir(&path).await?;

        fs::write(&path, data).await?;

        debug!(
            id = %id,
            path = %path.display(),
            size = data.len(),
            "Saved optimized file"
        );

        Ok(path)
    }

    /// Get path to an optimized file
    pub fn optimized_path(&self, id: Uuid, extension: &str) -> PathBuf {
        self.build_file_path(&self.optimized_dir, id, extension)
    }

    /// Read an optimized file
    pub async fn read_optimized(&self, id: Uuid, extension: &str) -> Result<Vec<u8>> {
        let path = self.optimized_path(id, extension);

        if !path.exists() {
            return Err(AppError::not_found(format!("Optimized file not found: {}", id)));
        }

        Ok(fs::read(&path).await?)
    }

    /// Delete an optimized file
    pub async fn delete_optimized(&self, id: Uuid, extension: &str) -> Result<()> {
        let path = self.optimized_path(id, extension);

        if path.exists() {
            fs::remove_file(&path).await?;
            debug!(id = %id, path = %path.display(), "Deleted optimized file");
        }

        Ok(())
    }

    // =========================================================================
    // Temporary files (chunked uploads)
    // =========================================================================

    /// Create a temporary directory for a chunked upload session
    pub async fn create_temp_session_dir(&self, session_id: Uuid) -> Result<PathBuf> {
        let path = self.temp_dir.join(session_id.to_string());
        fs::create_dir_all(&path).await?;

        debug!(session_id = %session_id, path = %path.display(), "Created temp session directory");

        Ok(path)
    }

    /// Get the path to a temp session directory
    pub fn temp_session_path(&self, session_id: Uuid) -> PathBuf {
        self.temp_dir.join(session_id.to_string())
    }

    /// Save a chunk to a temp session
    ///
    /// # Arguments
    /// * `session_id` - Upload session UUID
    /// * `chunk_index` - Chunk index (0-based)
    /// * `data` - Chunk data
    pub async fn save_chunk(
        &self,
        session_id: Uuid,
        chunk_index: u64,
        data: &[u8],
    ) -> Result<PathBuf> {
        let session_dir = self.temp_session_path(session_id);

        // Ensure session directory exists
        if !session_dir.exists() {
            fs::create_dir_all(&session_dir).await?;
        }

        let chunk_path = session_dir.join(format!("chunk_{}", chunk_index));
        fs::write(&chunk_path, data).await?;

        debug!(
            session_id = %session_id,
            chunk_index = chunk_index,
            size = data.len(),
            "Saved chunk"
        );

        Ok(chunk_path)
    }

    /// Append data to a temp file (for streaming chunks)
    pub async fn append_to_temp_file(
        &self,
        session_id: Uuid,
        data: &[u8],
    ) -> Result<()> {
        let session_dir = self.temp_session_path(session_id);

        // Ensure session directory exists
        if !session_dir.exists() {
            fs::create_dir_all(&session_dir).await?;
        }

        let file_path = session_dir.join("upload");

        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)
            .await?;

        file.write_all(data).await?;
        file.flush().await?;

        Ok(())
    }

    /// Get the current size of the temp upload file
    pub async fn get_temp_file_size(&self, session_id: Uuid) -> Result<u64> {
        let file_path = self.temp_session_path(session_id).join("upload");

        if !file_path.exists() {
            return Ok(0);
        }

        let metadata = fs::metadata(&file_path).await?;
        Ok(metadata.len())
    }

    /// Read the assembled temp file
    pub async fn read_temp_file(&self, session_id: Uuid) -> Result<Vec<u8>> {
        let file_path = self.temp_session_path(session_id).join("upload");

        if !file_path.exists() {
            return Err(AppError::not_found(format!(
                "Temp file not found for session: {}",
                session_id
            )));
        }

        Ok(fs::read(&file_path).await?)
    }

    /// Assemble chunks into a single file
    ///
    /// Reads all chunk files in order and combines them into a single buffer.
    ///
    /// # Arguments
    /// * `session_id` - Upload session UUID
    /// * `num_chunks` - Expected number of chunks
    ///
    /// # Returns
    /// Combined file data
    pub async fn assemble_chunks(&self, session_id: Uuid, num_chunks: u64) -> Result<Vec<u8>> {
        let session_dir = self.temp_session_path(session_id);
        let mut data = Vec::new();

        for i in 0..num_chunks {
            let chunk_path = session_dir.join(format!("chunk_{}", i));

            if !chunk_path.exists() {
                return Err(AppError::upload_session(format!(
                    "Missing chunk {} for session {}",
                    i, session_id
                )));
            }

            let chunk_data = fs::read(&chunk_path).await?;
            data.extend_from_slice(&chunk_data);
        }

        debug!(
            session_id = %session_id,
            num_chunks = num_chunks,
            total_size = data.len(),
            "Assembled chunks"
        );

        Ok(data)
    }

    /// Delete a temp session directory and all its contents
    pub async fn delete_temp_session(&self, session_id: Uuid) -> Result<()> {
        let session_dir = self.temp_session_path(session_id);

        if session_dir.exists() {
            fs::remove_dir_all(&session_dir).await?;
            debug!(session_id = %session_id, "Deleted temp session directory");
        }

        Ok(())
    }

    /// Clean up expired temp sessions
    ///
    /// # Arguments
    /// * `max_age_secs` - Maximum age in seconds for temp directories
    ///
    /// # Returns
    /// Number of sessions cleaned up
    pub async fn cleanup_expired_sessions(&self, max_age_secs: u64) -> Result<usize> {
        let mut cleaned = 0;
        let now = std::time::SystemTime::now();
        let max_age = std::time::Duration::from_secs(max_age_secs);

        let mut entries = fs::read_dir(&self.temp_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            if !path.is_dir() {
                continue;
            }

            // Check directory age based on modification time
            if let Ok(metadata) = fs::metadata(&path).await {
                if let Ok(modified) = metadata.modified() {
                    if let Ok(age) = now.duration_since(modified) {
                        if age > max_age {
                            if let Err(e) = fs::remove_dir_all(&path).await {
                                warn!(
                                    path = %path.display(),
                                    error = %e,
                                    "Failed to cleanup expired session"
                                );
                            } else {
                                info!(path = %path.display(), "Cleaned up expired session");
                                cleaned += 1;
                            }
                        }
                    }
                }
            }
        }

        Ok(cleaned)
    }

    // =========================================================================
    // Utility methods
    // =========================================================================

    /// Check if a file exists in the originals directory
    pub async fn original_exists(&self, id: Uuid, extension: &str) -> bool {
        self.original_path(id, extension).exists()
    }

    /// Check if a file exists in the optimized directory
    pub async fn optimized_exists(&self, id: Uuid, extension: &str) -> bool {
        self.optimized_path(id, extension).exists()
    }

    /// Get the file path for serving (prefers optimized)
    pub fn get_serve_path(&self, id: Uuid, optimized_ext: &str, original_ext: &str) -> PathBuf {
        let optimized = self.optimized_path(id, optimized_ext);
        if optimized.exists() {
            optimized
        } else {
            self.original_path(id, original_ext)
        }
    }

    /// Delete all files associated with a media ID
    pub async fn delete_media_files(
        &self,
        id: Uuid,
        original_ext: &str,
        optimized_ext: &str,
    ) -> Result<()> {
        self.delete_original(id, original_ext).await?;
        self.delete_optimized(id, optimized_ext).await?;
        Ok(())
    }

    /// Get storage statistics
    pub async fn get_stats(&self) -> Result<StorageStats> {
        let originals_size = Self::dir_size(&self.originals_dir).await?;
        let optimized_size = Self::dir_size(&self.optimized_dir).await?;
        let temp_size = Self::dir_size(&self.temp_dir).await?;

        let originals_count = Self::file_count(&self.originals_dir).await?;
        let optimized_count = Self::file_count(&self.optimized_dir).await?;

        Ok(StorageStats {
            originals_size,
            optimized_size,
            temp_size,
            total_size: originals_size + optimized_size + temp_size,
            originals_count,
            optimized_count,
        })
    }

    /// Calculate total size of a directory
    async fn dir_size(path: &Path) -> Result<u64> {
        let mut total = 0;

        if !path.exists() {
            return Ok(0);
        }

        let mut entries = fs::read_dir(path).await?;

        while let Some(entry) = entries.next_entry().await? {
            let metadata = entry.metadata().await?;
            if metadata.is_file() {
                total += metadata.len();
            } else if metadata.is_dir() {
                total += Box::pin(Self::dir_size(&entry.path())).await?;
            }
        }

        Ok(total)
    }

    /// Count files in a directory
    async fn file_count(path: &Path) -> Result<usize> {
        let mut count = 0;

        if !path.exists() {
            return Ok(0);
        }

        let mut entries = fs::read_dir(path).await?;

        while let Some(entry) = entries.next_entry().await? {
            let metadata = entry.metadata().await?;
            if metadata.is_file() {
                count += 1;
            }
        }

        Ok(count)
    }
}

/// Storage statistics
#[derive(Debug, Clone, serde::Serialize)]
pub struct StorageStats {
    /// Size of originals directory in bytes
    pub originals_size: u64,
    /// Size of optimized directory in bytes
    pub optimized_size: u64,
    /// Size of temp directory in bytes
    pub temp_size: u64,
    /// Total storage size in bytes
    pub total_size: u64,
    /// Number of original files
    pub originals_count: usize,
    /// Number of optimized files
    pub optimized_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn create_test_service() -> (StorageService, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = crate::config::StorageConfig {
            data_dir: temp_dir.path().to_path_buf(),
            originals_dir: "originals".to_string(),
            optimized_dir: "optimized".to_string(),
            temp_dir: "temp".to_string(),
            directory_levels: 2,
            database_file: "test.db".to_string(),
        };

        let service = StorageService::new(&config).await.unwrap();
        (service, temp_dir)
    }

    #[tokio::test]
    async fn test_save_and_read_original() {
        let (service, _temp) = create_test_service().await;
        let id = Uuid::new_v4();
        let data = b"test image data";

        service.save_original(id, "jpg", data).await.unwrap();

        let read_data = service.read_original(id, "jpg").await.unwrap();
        assert_eq!(read_data, data);
    }

    #[tokio::test]
    async fn test_delete_original() {
        let (service, _temp) = create_test_service().await;
        let id = Uuid::new_v4();
        let data = b"test image data";

        service.save_original(id, "jpg", data).await.unwrap();
        assert!(service.original_exists(id, "jpg").await);

        service.delete_original(id, "jpg").await.unwrap();
        assert!(!service.original_exists(id, "jpg").await);
    }
}

