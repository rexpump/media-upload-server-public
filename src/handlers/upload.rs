//! Upload handlers for receiving media files.
//!
//! This module provides two upload methods:
//!
//! ## Simple Upload
//! - `POST /api/upload` - Single request upload for files up to max_simple_upload_size
//!
//! ## Chunked Upload (Resumable)
//! - `POST /api/upload/init` - Initialize a chunked upload session
//! - `PATCH /api/upload/{id}/chunk` - Upload a chunk
//! - `POST /api/upload/{id}/complete` - Complete the upload
//! - `GET /api/upload/{id}/status` - Get upload status (for resuming)
//!
//! # Example: Simple Upload
//!
//! ```bash
//! curl -X POST http://localhost:3000/api/upload \
//!   -F "file=@image.jpg"
//! ```
//!
//! # Example: Chunked Upload
//!
//! ```bash
//! # 1. Initialize
//! curl -X POST http://localhost:3000/api/upload/init \
//!   -H "Content-Type: application/json" \
//!   -d '{"filename": "large.jpg", "mime_type": "image/jpeg", "total_size": 10485760}'
//!
//! # 2. Upload chunks
//! curl -X PATCH "http://localhost:3000/api/upload/{id}/chunk" \
//!   -H "Content-Range: bytes 0-5242879/10485760" \
//!   --data-binary @chunk1
//!
//! # 3. Complete
//! curl -X POST "http://localhost:3000/api/upload/{id}/complete"
//! ```

use axum::{
    extract::{Multipart, Path, State},
    http::{HeaderMap, StatusCode},
    routing::{get, patch, post},
    Json, Router,
};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::models::{
    InitUploadRequest, Media, UploadResponse, UploadSession, UploadSessionResponse,
};
use crate::services::image_processor::{calculate_hash, ImageProcessor};
use crate::state::AppState;

// =============================================================================
// Simple Upload
// =============================================================================

/// Handle simple file upload via multipart form
///
/// POST /api/upload
///
/// Accepts a multipart form with a `file` field containing the image.
/// Returns the media ID and URL on success.
async fn simple_upload(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<UploadResponse>)> {
    // Extract file from multipart
    let mut file_data: Option<(String, Vec<u8>)> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::validation(format!("Invalid multipart data: {}", e)))?
    {
        let name = field.name().unwrap_or("").to_string();

        if name == "file" {
            let filename = field
                .file_name()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "upload".to_string());

            let data = field
                .bytes()
                .await
                .map_err(|e| AppError::validation(format!("Failed to read file: {}", e)))?;

            // Check size limit
            if data.len() as u64 > state.max_upload_size() {
                return Err(AppError::payload_too_large(format!(
                    "File size {} exceeds maximum allowed size {}",
                    data.len(),
                    state.max_upload_size()
                )));
            }

            file_data = Some((filename, data.to_vec()));
            break;
        }
    }

    let (filename, data) = file_data.ok_or_else(|| {
        AppError::validation("No file field found in multipart request")
    })?;

    info!(filename = %filename, size = data.len(), "Received upload");

    // Process the image
    let media = process_and_store_image(&state, &filename, &data).await?;

    // Return response
    let response = UploadResponse::from_media(&media, state.base_url(), state.keep_originals());

    Ok((StatusCode::CREATED, Json(response)))
}

// =============================================================================
// Chunked Upload
// =============================================================================

/// Initialize a chunked upload session
///
/// POST /api/upload/init
///
/// Creates a new upload session and returns the session ID.
async fn init_chunked_upload(
    State(state): State<AppState>,
    Json(request): Json<InitUploadRequest>,
) -> Result<(StatusCode, Json<UploadSessionResponse>)> {
    // Validate request
    if request.total_size == 0 {
        return Err(AppError::validation("total_size must be greater than 0"));
    }

    if request.total_size > state.config.upload.max_chunked_upload_size {
        return Err(AppError::payload_too_large(format!(
            "File size {} exceeds maximum allowed size {}",
            request.total_size, state.config.upload.max_chunked_upload_size
        )));
    }

    // Validate MIME type
    if !state.config.upload.is_allowed_type(&request.mime_type) {
        return Err(AppError::unsupported_media_type(format!(
            "MIME type {} is not allowed",
            request.mime_type
        )));
    }

    // Create session
    let session = UploadSession::new(
        request.filename,
        request.mime_type,
        request.total_size,
        state.chunk_size(),
        state.upload_session_timeout(),
    );

    // Create temp directory for chunks
    state.storage.create_temp_session_dir(session.id).await?;

    // Save session to database
    state.db.insert_session(&session)?;

    info!(
        session_id = %session.id,
        total_size = request.total_size,
        "Created upload session"
    );

    let response = UploadSessionResponse::from_session(&session, Some(state.base_url()));

    Ok((StatusCode::CREATED, Json(response)))
}

/// Upload a chunk
///
/// PATCH /api/upload/{id}/chunk
///
/// Accepts binary data with Content-Range header indicating chunk position.
async fn upload_chunk(
    State(state): State<AppState>,
    Path(session_id): Path<Uuid>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<UploadSessionResponse>> {
    // Get session
    let mut session = state
        .db
        .get_session(session_id)?
        .ok_or_else(|| AppError::not_found(format!("Upload session not found: {}", session_id)))?;

    // Check session status
    if !session.status.can_accept_chunks() {
        return Err(AppError::upload_session(format!(
            "Session {} is not accepting chunks (status: {:?})",
            session_id, session.status
        )));
    }

    // Check if session expired
    if session.is_expired() {
        session.mark_expired();
        state.db.update_session(&session)?;
        return Err(AppError::upload_session("Upload session has expired"));
    }

    // Parse Content-Range header if present
    let (start, _end) = parse_content_range(&headers, body.len() as u64, session.total_size)?;

    // Verify chunk is at expected offset
    if start != session.received_bytes {
        warn!(
            session_id = %session_id,
            expected = session.received_bytes,
            got = start,
            "Chunk offset mismatch"
        );
        // Return current status so client can resume correctly
        return Ok(Json(UploadSessionResponse::from_session(
            &session,
            Some(state.base_url()),
        )));
    }

    // Save chunk data
    state
        .storage
        .append_to_temp_file(session_id, &body)
        .await?;

    // Update session
    session.add_received_bytes(body.len() as u64);
    state.db.update_session(&session)?;

    debug!(
        session_id = %session_id,
        received = session.received_bytes,
        total = session.total_size,
        progress = format!("{:.1}%", session.progress_percent()),
        "Chunk received"
    );

    Ok(Json(UploadSessionResponse::from_session(
        &session,
        Some(state.base_url()),
    )))
}

/// Complete a chunked upload
///
/// POST /api/upload/{id}/complete
///
/// Assembles all chunks and processes the final file.
async fn complete_upload(
    State(state): State<AppState>,
    Path(session_id): Path<Uuid>,
) -> Result<Json<UploadResponse>> {
    // Get session
    let mut session = state
        .db
        .get_session(session_id)?
        .ok_or_else(|| AppError::not_found(format!("Upload session not found: {}", session_id)))?;

    // Check if all data received
    if !session.is_complete() {
        return Err(AppError::upload_session(format!(
            "Upload incomplete: received {} of {} bytes",
            session.received_bytes, session.total_size
        )));
    }

    // Mark as processing
    session.mark_processing();
    state.db.update_session(&session)?;

    // Read assembled file
    let data = state.storage.read_temp_file(session_id).await?;

    // Process the image
    let media = match process_and_store_image(&state, &session.filename, &data).await {
        Ok(media) => {
            // Mark session as completed
            session.mark_completed(media.id);
            state.db.update_session(&session)?;

            // Clean up temp files
            if let Err(e) = state.storage.delete_temp_session(session_id).await {
                warn!(session_id = %session_id, error = %e, "Failed to cleanup temp session");
            }

            media
        }
        Err(e) => {
            // Mark session as failed
            session.mark_failed(e.to_string());
            state.db.update_session(&session)?;
            return Err(e);
        }
    };

    info!(
        session_id = %session_id,
        media_id = %media.id,
        "Completed chunked upload"
    );

    let response = UploadResponse::from_media(&media, state.base_url(), state.keep_originals());

    Ok(Json(response))
}

/// Get upload session status
///
/// GET /api/upload/{id}/status
///
/// Returns current upload progress for resuming interrupted uploads.
async fn get_upload_status(
    State(state): State<AppState>,
    Path(session_id): Path<Uuid>,
) -> Result<Json<UploadSessionResponse>> {
    let session = state
        .db
        .get_session(session_id)?
        .ok_or_else(|| AppError::not_found(format!("Upload session not found: {}", session_id)))?;

    Ok(Json(UploadSessionResponse::from_session(
        &session,
        Some(state.base_url()),
    )))
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Process and store an uploaded image
async fn process_and_store_image(
    state: &AppState,
    filename: &str,
    data: &[u8],
) -> Result<Media> {
    // Calculate content hash for deduplication
    let content_hash = calculate_hash(data);

    // Check for duplicate
    if let Some(existing) = state.db.find_by_hash(&content_hash)? {
        info!(
            existing_id = %existing.id,
            hash = %content_hash,
            "Found duplicate content, returning existing media"
        );
        return Ok(existing);
    }

    // Process the image
    let processed = state.image_processor.process(data, &state.config.upload)?;

    // Create media record using output format from config
    let media = Media::new(
        filename.to_string(),
        processed.original_mime.clone(),
        state.output_mime_type().to_string(),
        processed.original_data.len() as u64,
        processed.optimized_data.len() as u64,
        processed.width,
        processed.height,
        content_hash,
    );

    // Save files
    let original_ext = ImageProcessor::mime_to_extension(&processed.original_mime);
    let output_ext = state.output_extension();

    if state.keep_originals() {
        state
            .storage
            .save_original(media.id, original_ext, &processed.original_data)
            .await?;
    }

    state
        .storage
        .save_optimized(media.id, output_ext, &processed.optimized_data)
        .await?;

    // Save to database
    state.db.insert_media(&media)?;

    info!(
        id = %media.id,
        original_size = processed.original_data.len(),
        optimized_size = processed.optimized_data.len(),
        "Stored media"
    );

    Ok(media)
}

/// Parse Content-Range header
///
/// Format: "bytes start-end/total"
fn parse_content_range(
    headers: &HeaderMap,
    body_len: u64,
    _expected_total: u64,
) -> Result<(u64, u64)> {
    if let Some(range_header) = headers.get("content-range") {
        let range_str = range_header
            .to_str()
            .map_err(|_| AppError::validation("Invalid Content-Range header"))?;

        // Parse "bytes start-end/total"
        if let Some(rest) = range_str.strip_prefix("bytes ") {
            let parts: Vec<&str> = rest.split('/').collect();
            if parts.len() == 2 {
                let range_parts: Vec<&str> = parts[0].split('-').collect();
                if range_parts.len() == 2 {
                    let start: u64 = range_parts[0]
                        .parse()
                        .map_err(|_| AppError::validation("Invalid Content-Range start"))?;
                    let end: u64 = range_parts[1]
                        .parse()
                        .map_err(|_| AppError::validation("Invalid Content-Range end"))?;

                    return Ok((start, end));
                }
            }
        }

        Err(AppError::validation("Invalid Content-Range format"))
    } else {
        // No Content-Range header, assume starting from current offset
        Ok((0, body_len - 1))
    }
}

/// Create upload routes
pub fn upload_routes() -> Router<AppState> {
    Router::new()
        // Simple upload
        .route("/", post(simple_upload))
        // Chunked upload
        .route("/init", post(init_chunked_upload))
        .route("/{id}/chunk", patch(upload_chunk))
        .route("/{id}/complete", post(complete_upload))
        .route("/{id}/status", get(get_upload_status))
}

