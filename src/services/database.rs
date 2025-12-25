//! Database service using RocksDB.
//!
//! RocksDB provides excellent crash safety through its LSM-tree architecture
//! and write-ahead log (WAL). All writes are atomic and durable.
//!
//! # Data Organization
//!
//! Uses column families to separate data types:
//! - `media`: Media records (key: UUID)
//! - `hash_index`: Content hash → UUID mapping (for deduplication)
//! - `sessions`: Upload sessions (key: UUID)
//! - `session_expires`: Expiration index (key: timestamp:uuid)

use crate::config::StorageConfig;
use crate::error::{AppError, Result};
use crate::models::{Media, MediaType, UploadSession, UploadSessionStatus};
use chrono::{DateTime, Utc};
use rocksdb::{ColumnFamilyDescriptor, DBWithThreadMode, MultiThreaded, Options, WriteBatch};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info};
use uuid::Uuid;

type DB = DBWithThreadMode<MultiThreaded>;

/// Column family names
const CF_MEDIA: &str = "media";
const CF_HASH_INDEX: &str = "hash_index";
const CF_SESSIONS: &str = "sessions";
const CF_SESSION_EXPIRES: &str = "session_expires";

/// Database service for managing media metadata
///
/// Uses RocksDB for high performance and crash safety.
#[derive(Clone)]
pub struct DatabaseService {
    db: Arc<DB>,
    #[allow(dead_code)]
    db_path: PathBuf,
}

impl std::fmt::Debug for DatabaseService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DatabaseService")
            .field("path", &self.db_path)
            .finish()
    }
}

impl DatabaseService {
    /// Create a new database service
    pub fn new(config: &StorageConfig) -> Result<Self> {
        let db_path = config.data_dir.join("rocksdb");

        // Ensure directory exists
        std::fs::create_dir_all(&db_path)?;

        // Configure RocksDB options
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        // Performance tuning
        opts.set_max_open_files(256);
        opts.set_keep_log_file_num(3);
        opts.set_max_total_wal_size(64 * 1024 * 1024); // 64MB
        opts.set_write_buffer_size(32 * 1024 * 1024); // 32MB
        opts.set_max_write_buffer_number(3);

        // Define column families
        let cf_names = [CF_MEDIA, CF_HASH_INDEX, CF_SESSIONS, CF_SESSION_EXPIRES];
        let cf_descriptors: Vec<_> = cf_names
            .iter()
            .map(|name| {
                let mut cf_opts = Options::default();
                cf_opts.set_compression_type(rocksdb::DBCompressionType::Lz4);
                ColumnFamilyDescriptor::new(*name, cf_opts)
            })
            .collect();

        // Open database with column families
        let db = DB::open_cf_descriptors(&opts, &db_path, cf_descriptors)
            .map_err(|e| AppError::internal(format!("Failed to open RocksDB: {}", e)))?;

        info!(path = %db_path.display(), "Database initialized (RocksDB)");

        Ok(Self {
            db: Arc::new(db),
            db_path,
        })
    }

    fn cf_media(&self) -> Arc<rocksdb::BoundColumnFamily<'_>> {
        self.db.cf_handle(CF_MEDIA).expect("CF media must exist")
    }

    fn cf_hash_index(&self) -> Arc<rocksdb::BoundColumnFamily<'_>> {
        self.db
            .cf_handle(CF_HASH_INDEX)
            .expect("CF hash_index must exist")
    }

    fn cf_sessions(&self) -> Arc<rocksdb::BoundColumnFamily<'_>> {
        self.db
            .cf_handle(CF_SESSIONS)
            .expect("CF sessions must exist")
    }

    fn cf_session_expires(&self) -> Arc<rocksdb::BoundColumnFamily<'_>> {
        self.db
            .cf_handle(CF_SESSION_EXPIRES)
            .expect("CF session_expires must exist")
    }

    // =========================================================================
    // Media operations
    // =========================================================================

    /// Insert a new media record
    pub fn insert_media(&self, media: &Media) -> Result<()> {
        let record = MediaRecord::from(media);
        let data = serde_json::to_vec(&record)?;

        // Atomic batch write: media record + hash index
        let mut batch = WriteBatch::default();
        batch.put_cf(&self.cf_media(), media.id.to_string().as_bytes(), &data);
        batch.put_cf(
            &self.cf_hash_index(),
            media.content_hash.as_bytes(),
            media.id.to_string().as_bytes(),
        );

        self.db
            .write(batch)
            .map_err(|e| AppError::internal(format!("RocksDB write failed: {}", e)))?;

        debug!(id = %media.id, "Inserted media record");
        Ok(())
    }

    /// Get a media record by ID
    pub fn get_media(&self, id: Uuid) -> Result<Option<Media>> {
        let key = id.to_string();
        match self
            .db
            .get_cf(&self.cf_media(), key.as_bytes())
            .map_err(|e| AppError::internal(format!("RocksDB read failed: {}", e)))?
        {
            Some(data) => {
                let record: MediaRecord = serde_json::from_slice(&data)?;
                Ok(Some(record.into_media()?))
            }
            None => Ok(None),
        }
    }

    /// Delete a media record by ID
    pub fn delete_media(&self, id: Uuid) -> Result<bool> {
        // First get the media to find its hash
        let media = match self.get_media(id)? {
            Some(m) => m,
            None => return Ok(false),
        };

        // Atomic delete of both record and hash index
        let mut batch = WriteBatch::default();
        batch.delete_cf(&self.cf_media(), id.to_string().as_bytes());
        batch.delete_cf(&self.cf_hash_index(), media.content_hash.as_bytes());

        self.db
            .write(batch)
            .map_err(|e| AppError::internal(format!("RocksDB delete failed: {}", e)))?;

        debug!(id = %id, "Deleted media record");
        Ok(true)
    }

    /// Update last_accessed_at timestamp
    pub fn update_last_accessed(&self, id: Uuid) -> Result<()> {
        let mut media = match self.get_media(id)? {
            Some(m) => m,
            None => return Ok(()),
        };

        media.last_accessed_at = Some(Utc::now());

        let record = MediaRecord::from(&media);
        let data = serde_json::to_vec(&record)?;

        self.db
            .put_cf(&self.cf_media(), id.to_string().as_bytes(), data)
            .map_err(|e| AppError::internal(format!("RocksDB write failed: {}", e)))?;

        Ok(())
    }

    /// Find media by content hash (for deduplication)
    pub fn find_by_hash(&self, hash: &str) -> Result<Option<Media>> {
        match self
            .db
            .get_cf(&self.cf_hash_index(), hash.as_bytes())
            .map_err(|e| AppError::internal(format!("RocksDB read failed: {}", e)))?
        {
            Some(id_bytes) => {
                let id_str = String::from_utf8_lossy(&id_bytes);
                let id = Uuid::parse_str(&id_str)?;
                self.get_media(id)
            }
            None => Ok(None),
        }
    }

    /// Get total media count
    pub fn get_media_count(&self) -> Result<u64> {
        let mut count = 0u64;
        let iter = self.db.iterator_cf(&self.cf_media(), rocksdb::IteratorMode::Start);

        for item in iter {
            if item.is_ok() {
                count += 1;
            }
        }

        Ok(count)
    }

    // =========================================================================
    // Upload session operations
    // =========================================================================

    /// Insert a new upload session
    pub fn insert_session(&self, session: &UploadSession) -> Result<()> {
        let record = SessionRecord::from(session);
        let data = serde_json::to_vec(&record)?;

        // Create expiration index key: "timestamp:uuid"
        let expires_key = format!("{}:{}", session.expires_at.to_rfc3339(), session.id);

        let mut batch = WriteBatch::default();
        batch.put_cf(
            &self.cf_sessions(),
            session.id.to_string().as_bytes(),
            &data,
        );
        batch.put_cf(
            &self.cf_session_expires(),
            expires_key.as_bytes(),
            session.id.to_string().as_bytes(),
        );

        self.db
            .write(batch)
            .map_err(|e| AppError::internal(format!("RocksDB write failed: {}", e)))?;

        debug!(id = %session.id, "Inserted upload session");
        Ok(())
    }

    /// Get an upload session by ID
    pub fn get_session(&self, id: Uuid) -> Result<Option<UploadSession>> {
        let key = id.to_string();
        match self
            .db
            .get_cf(&self.cf_sessions(), key.as_bytes())
            .map_err(|e| AppError::internal(format!("RocksDB read failed: {}", e)))?
        {
            Some(data) => {
                let record: SessionRecord = serde_json::from_slice(&data)?;
                Ok(Some(record.into_session()?))
            }
            None => Ok(None),
        }
    }

    /// Update an upload session
    pub fn update_session(&self, session: &UploadSession) -> Result<()> {
        // Get old session to remove old expiration index
        let old_session = self.get_session(session.id)?;

        let record = SessionRecord::from(session);
        let data = serde_json::to_vec(&record)?;

        let mut batch = WriteBatch::default();
        batch.put_cf(
            &self.cf_sessions(),
            session.id.to_string().as_bytes(),
            &data,
        );

        // Update expiration index if it changed
        if let Some(old) = old_session {
            if old.expires_at != session.expires_at {
                let old_expires_key = format!("{}:{}", old.expires_at.to_rfc3339(), session.id);
                batch.delete_cf(&self.cf_session_expires(), old_expires_key.as_bytes());

                let new_expires_key =
                    format!("{}:{}", session.expires_at.to_rfc3339(), session.id);
                batch.put_cf(
                    &self.cf_session_expires(),
                    new_expires_key.as_bytes(),
                    session.id.to_string().as_bytes(),
                );
            }
        }

        self.db
            .write(batch)
            .map_err(|e| AppError::internal(format!("RocksDB write failed: {}", e)))?;

        debug!(id = %session.id, status = ?session.status, "Updated upload session");
        Ok(())
    }

    /// Delete an upload session
    pub fn delete_session(&self, id: Uuid) -> Result<bool> {
        // Get session to find expiration key
        let session = match self.get_session(id)? {
            Some(s) => s,
            None => return Ok(false),
        };

        let expires_key = format!("{}:{}", session.expires_at.to_rfc3339(), id);

        let mut batch = WriteBatch::default();
        batch.delete_cf(&self.cf_sessions(), id.to_string().as_bytes());
        batch.delete_cf(&self.cf_session_expires(), expires_key.as_bytes());

        self.db
            .write(batch)
            .map_err(|e| AppError::internal(format!("RocksDB delete failed: {}", e)))?;

        Ok(true)
    }

    /// Clean up expired sessions, returns IDs of deleted sessions
    pub fn cleanup_expired_sessions(&self) -> Result<Vec<Uuid>> {
        let now = Utc::now().to_rfc3339();
        let mut expired_ids = Vec::new();

        // Scan expiration index for expired sessions
        let iter = self
            .db
            .iterator_cf(&self.cf_session_expires(), rocksdb::IteratorMode::Start);

        for item in iter {
            let (key, value) =
                item.map_err(|e| AppError::internal(format!("RocksDB iterator error: {}", e)))?;

            let key_str = String::from_utf8_lossy(&key);

            // Key format: "timestamp:uuid"
            // If key is lexically greater than now, we've passed all expired sessions
            if key_str.as_ref() > now.as_str() {
                break;
            }

            // Check if session is still in_progress
            let id_str = String::from_utf8_lossy(&value);
            if let Ok(id) = Uuid::parse_str(&id_str) {
                if let Ok(Some(session)) = self.get_session(id) {
                    if session.status == UploadSessionStatus::InProgress {
                        expired_ids.push(id);
                    }
                }
            }
        }

        // Delete expired sessions
        if !expired_ids.is_empty() {
            let mut batch = WriteBatch::default();

            for id in &expired_ids {
                if let Ok(Some(session)) = self.get_session(*id) {
                    let expires_key = format!("{}:{}", session.expires_at.to_rfc3339(), id);
                    batch.delete_cf(&self.cf_sessions(), id.to_string().as_bytes());
                    batch.delete_cf(&self.cf_session_expires(), expires_key.as_bytes());
                }
            }

            self.db
                .write(batch)
                .map_err(|e| AppError::internal(format!("RocksDB cleanup failed: {}", e)))?;

            info!(count = expired_ids.len(), "Cleaned up expired upload sessions");
        }

        Ok(expired_ids)
    }
}

// =============================================================================
// Serialization structs
// =============================================================================

#[derive(Serialize, Deserialize)]
struct MediaRecord {
    id: String,
    original_filename: String,
    original_mime_type: String,
    optimized_mime_type: String,
    media_type: String,
    original_size: u64,
    optimized_size: u64,
    width: u32,
    height: u32,
    content_hash: String,
    created_at: String,
    last_accessed_at: Option<String>,
}

impl From<&Media> for MediaRecord {
    fn from(media: &Media) -> Self {
        Self {
            id: media.id.to_string(),
            original_filename: media.original_filename.clone(),
            original_mime_type: media.original_mime_type.clone(),
            optimized_mime_type: media.optimized_mime_type.clone(),
            media_type: media.media_type.as_str().to_string(),
            original_size: media.original_size,
            optimized_size: media.optimized_size,
            width: media.width,
            height: media.height,
            content_hash: media.content_hash.clone(),
            created_at: media.created_at.to_rfc3339(),
            last_accessed_at: media.last_accessed_at.map(|dt| dt.to_rfc3339()),
        }
    }
}

impl MediaRecord {
    fn into_media(self) -> Result<Media> {
        Ok(Media {
            id: Uuid::parse_str(&self.id)?,
            original_filename: self.original_filename,
            original_mime_type: self.original_mime_type,
            optimized_mime_type: self.optimized_mime_type,
            media_type: MediaType::from_str(&self.media_type).unwrap_or(MediaType::Image),
            original_size: self.original_size,
            optimized_size: self.optimized_size,
            width: self.width,
            height: self.height,
            content_hash: self.content_hash,
            created_at: DateTime::parse_from_rfc3339(&self.created_at)
                .map_err(|e| AppError::internal(format!("Invalid date: {}", e)))?
                .with_timezone(&Utc),
            last_accessed_at: self
                .last_accessed_at
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|dt| dt.with_timezone(&Utc)),
        })
    }
}

#[derive(Serialize, Deserialize)]
struct SessionRecord {
    id: String,
    filename: String,
    mime_type: String,
    total_size: u64,
    received_bytes: u64,
    chunk_size: u64,
    status: String,
    error_message: Option<String>,
    media_id: Option<String>,
    created_at: String,
    updated_at: String,
    expires_at: String,
}

impl From<&UploadSession> for SessionRecord {
    fn from(session: &UploadSession) -> Self {
        Self {
            id: session.id.to_string(),
            filename: session.filename.clone(),
            mime_type: session.mime_type.clone(),
            total_size: session.total_size,
            received_bytes: session.received_bytes,
            chunk_size: session.chunk_size,
            status: session.status.as_str().to_string(),
            error_message: session.error_message.clone(),
            media_id: session.media_id.map(|id| id.to_string()),
            created_at: session.created_at.to_rfc3339(),
            updated_at: session.updated_at.to_rfc3339(),
            expires_at: session.expires_at.to_rfc3339(),
        }
    }
}

impl SessionRecord {
    fn into_session(self) -> Result<UploadSession> {
        Ok(UploadSession {
            id: Uuid::parse_str(&self.id)?,
            filename: self.filename,
            mime_type: self.mime_type,
            total_size: self.total_size,
            received_bytes: self.received_bytes,
            chunk_size: self.chunk_size,
            status: UploadSessionStatus::from_str(&self.status)
                .unwrap_or(UploadSessionStatus::InProgress),
            error_message: self.error_message,
            media_id: self.media_id.and_then(|s| Uuid::parse_str(&s).ok()),
            created_at: DateTime::parse_from_rfc3339(&self.created_at)
                .map_err(|e| AppError::internal(format!("Invalid date: {}", e)))?
                .with_timezone(&Utc),
            updated_at: DateTime::parse_from_rfc3339(&self.updated_at)
                .map_err(|e| AppError::internal(format!("Invalid date: {}", e)))?
                .with_timezone(&Utc),
            expires_at: DateTime::parse_from_rfc3339(&self.expires_at)
                .map_err(|e| AppError::internal(format!("Invalid date: {}", e)))?
                .with_timezone(&Utc),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_db() -> (DatabaseService, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = StorageConfig {
            data_dir: temp_dir.path().to_path_buf(),
            originals_dir: "originals".to_string(),
            optimized_dir: "optimized".to_string(),
            temp_dir: "temp".to_string(),
            directory_levels: 2,
            database_file: "unused".to_string(),
        };

        let db = DatabaseService::new(&config).unwrap();
        (db, temp_dir)
    }

    #[test]
    fn test_media_crud() {
        let (db, _temp) = create_test_db();

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

        // Insert
        db.insert_media(&media).unwrap();

        // Get
        let retrieved = db.get_media(media.id).unwrap().unwrap();
        assert_eq!(retrieved.id, media.id);
        assert_eq!(retrieved.original_filename, "test.jpg");

        // Find by hash
        let found = db.find_by_hash("abc123").unwrap().unwrap();
        assert_eq!(found.id, media.id);

        // Delete
        assert!(db.delete_media(media.id).unwrap());
        assert!(db.get_media(media.id).unwrap().is_none());
        assert!(db.find_by_hash("abc123").unwrap().is_none());
    }

    #[test]
    fn test_session_crud() {
        let (db, _temp) = create_test_db();

        let session = UploadSession::new(
            "test.png".to_string(),
            "image/png".to_string(),
            1000,
            512,
            3600,
        );

        // Insert
        db.insert_session(&session).unwrap();

        // Get
        let retrieved = db.get_session(session.id).unwrap().unwrap();
        assert_eq!(retrieved.id, session.id);
        assert_eq!(retrieved.filename, "test.png");

        // Delete
        assert!(db.delete_session(session.id).unwrap());
        assert!(db.get_session(session.id).unwrap().is_none());
    }
}

