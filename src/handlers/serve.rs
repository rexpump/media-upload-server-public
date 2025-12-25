//! Media serving handlers.
//!
//! This module handles serving uploaded media files to clients.
//!
//! ## Endpoints
//!
//! - `GET /m/{id}` - Serve optimized version (format from config)
//! - `GET /m/{id}/original` - Serve original version (if available)
//!
//! ## Caching
//!
//! Responses include appropriate cache headers:
//! - `Cache-Control: public, max-age={from config}, immutable`
//! - `ETag` based on content hash
//!
//! ## Content Negotiation
//!
//! The server checks `Accept` header and may serve original format
//! if optimized format is not supported (future enhancement).

use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use tokio::fs::File;
use tokio_util::io::ReaderStream;
use tracing::debug;
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::services::image_processor::ImageProcessor;
use crate::state::AppState;

/// Serve optimized media file
///
/// GET /m/{id}
///
/// Returns the optimized version of the uploaded media.
async fn serve_media(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response> {
    // Get media record
    let media = state
        .db
        .get_media(id)?
        .ok_or_else(|| AppError::not_found(format!("Media not found: {}", id)))?;

    // Check ETag for caching
    let etag = format!("\"{}\"", &media.content_hash);
    if let Some(if_none_match) = headers.get(header::IF_NONE_MATCH) {
        if if_none_match.to_str().unwrap_or("") == etag {
            return Ok(StatusCode::NOT_MODIFIED.into_response());
        }
    }

    // Get file path using output extension from config
    let output_ext = state.output_extension();
    let file_path = state.storage.optimized_path(id, output_ext);

    if !file_path.exists() {
        return Err(AppError::not_found(format!(
            "Optimized file not found for media: {}",
            id
        )));
    }

    // Update last accessed time (fire and forget)
    let db = state.db.clone();
    tokio::spawn(async move {
        let _ = db.update_last_accessed(id);
    });

    // Open file and create stream
    let file = File::open(&file_path).await?;
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    // Build cache control header from config
    let cache_control = format!("public, max-age={}, immutable", state.cache_max_age());

    // Build response with headers
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, state.output_mime_type())
        .header(header::CACHE_CONTROL, cache_control)
        .header(header::ETAG, etag)
        .header("X-Content-Type-Options", "nosniff")
        .body(body)
        .map_err(|e| AppError::internal(format!("Failed to build response: {}", e)))?;

    debug!(id = %id, "Served optimized media");

    Ok(response)
}

/// Serve original media file
///
/// GET /m/{id}/original
///
/// Returns the original uploaded file (if originals are kept).
async fn serve_original(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response> {
    // Check if originals are kept
    if !state.keep_originals() {
        return Err(AppError::not_found(
            "Original files are not available (not stored)",
        ));
    }

    // Get media record
    let media = state
        .db
        .get_media(id)?
        .ok_or_else(|| AppError::not_found(format!("Media not found: {}", id)))?;

    // Check ETag for caching
    let etag = format!("\"{}\"", &media.content_hash);
    if let Some(if_none_match) = headers.get(header::IF_NONE_MATCH) {
        if if_none_match.to_str().unwrap_or("") == etag {
            return Ok(StatusCode::NOT_MODIFIED.into_response());
        }
    }

    // Get file path
    let ext = ImageProcessor::mime_to_extension(&media.original_mime_type);
    let file_path = state.storage.original_path(id, ext);

    if !file_path.exists() {
        return Err(AppError::not_found(format!(
            "Original file not found for media: {}",
            id
        )));
    }

    // Open file and create stream
    let file = File::open(&file_path).await?;
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    // Build cache control header from config
    let cache_control = format!("public, max-age={}, immutable", state.cache_max_age());

    // Build response with headers
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, &media.original_mime_type)
        .header(header::CACHE_CONTROL, cache_control)
        .header(header::ETAG, etag)
        .header("X-Content-Type-Options", "nosniff")
        .header(
            header::CONTENT_DISPOSITION,
            format!(
                "inline; filename=\"{}\"",
                sanitize_filename(&media.original_filename)
            ),
        )
        .body(body)
        .map_err(|e| AppError::internal(format!("Failed to build response: {}", e)))?;

    debug!(id = %id, "Served original media");

    Ok(response)
}

/// Sanitize filename for Content-Disposition header
fn sanitize_filename(filename: &str) -> String {
    filename
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '.' || *c == '-' || *c == '_')
        .collect()
}

/// Create serve routes
pub fn serve_routes() -> Router<AppState> {
    Router::new()
        .route("/{id}", get(serve_media))
        .route("/{id}/original", get(serve_original))
}

