//! Admin API handlers (local only).
//!
//! These endpoints are only accessible from localhost and provide
//! administrative functionality for content moderation.
//!
//! ## Endpoints
//!
//! - `DELETE /admin/media/{id}` - Delete a media file
//! - `GET /admin/media/{id}` - Get detailed media info
//!
//! ## Security
//!
//! The admin API is bound to 127.0.0.1 only and should never be
//! exposed to the public internet.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get},
    Json, Router,
};
use serde::Serialize;
use tracing::{info, warn};
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::models::MediaInfoResponse;
use crate::services::image_processor::ImageProcessor;
use crate::state::AppState;

/// Delete a media file
///
/// DELETE /admin/media/{id}
///
/// Permanently removes a media file and all associated data.
/// This is used for content moderation (e.g., removing illegal content).
async fn delete_media(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<(StatusCode, Json<DeleteResponse>)> {
    // Get media record first to know file extensions
    let media = state
        .db
        .get_media(id)?
        .ok_or_else(|| AppError::not_found(format!("Media not found: {}", id)))?;

    // Delete files using output extension from config
    let original_ext = ImageProcessor::mime_to_extension(&media.original_mime_type);
    let optimized_ext = state.output_extension();

    if let Err(e) = state
        .storage
        .delete_media_files(id, original_ext, optimized_ext)
        .await
    {
        warn!(id = %id, error = %e, "Failed to delete some media files");
    }

    // Delete database record
    state.db.delete_media(id)?;

    info!(id = %id, filename = %media.original_filename, "Deleted media");

    Ok((
        StatusCode::OK,
        Json(DeleteResponse {
            success: true,
            message: format!("Media {} deleted successfully", id),
            id,
        }),
    ))
}

/// Delete response
#[derive(Debug, Serialize)]
pub struct DeleteResponse {
    pub success: bool,
    pub message: String,
    pub id: Uuid,
}

/// Get detailed media information
///
/// GET /admin/media/{id}
///
/// Returns all metadata about a media file.
async fn get_media_info(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<MediaInfoResponse>> {
    let media = state
        .db
        .get_media(id)?
        .ok_or_else(|| AppError::not_found(format!("Media not found: {}", id)))?;

    Ok(Json(MediaInfoResponse::from_media(&media, state.base_url())))
}

/// Get storage statistics
///
/// GET /admin/stats
async fn get_stats(State(state): State<AppState>) -> Result<Json<AdminStatsResponse>> {
    let storage_stats = state.storage.get_stats().await?;
    let media_count = state.db.get_media_count()?;

    Ok(Json(AdminStatsResponse {
        media_count,
        storage: storage_stats,
    }))
}

/// Admin stats response
#[derive(Debug, Serialize)]
pub struct AdminStatsResponse {
    pub media_count: u64,
    pub storage: crate::services::storage::StorageStats,
}

/// Cleanup expired upload sessions
///
/// POST /admin/cleanup
///
/// Removes expired upload sessions and their temporary files.
async fn cleanup_sessions(State(state): State<AppState>) -> Result<Json<CleanupResponse>> {
    // Cleanup database records
    let expired_ids = state.db.cleanup_expired_sessions()?;

    // Cleanup temp files
    let mut files_cleaned = 0;
    for id in &expired_ids {
        if let Err(e) = state.storage.delete_temp_session(*id).await {
            warn!(session_id = %id, error = %e, "Failed to cleanup temp session files");
        } else {
            files_cleaned += 1;
        }
    }

    // Also cleanup any orphaned temp directories
    let orphaned = state
        .storage
        .cleanup_expired_sessions(state.upload_session_timeout())
        .await
        .unwrap_or(0);

    info!(
        sessions = expired_ids.len(),
        files = files_cleaned,
        orphaned = orphaned,
        "Cleanup completed"
    );

    Ok(Json(CleanupResponse {
        sessions_cleaned: expired_ids.len(),
        files_cleaned,
        orphaned_dirs_cleaned: orphaned,
    }))
}

/// Cleanup response
#[derive(Debug, Serialize)]
pub struct CleanupResponse {
    pub sessions_cleaned: usize,
    pub files_cleaned: usize,
    pub orphaned_dirs_cleaned: usize,
}

/// Create admin routes
pub fn admin_routes() -> Router<AppState> {
    Router::new()
        .route("/media/{id}", delete(delete_media))
        .route("/media/{id}", get(get_media_info))
        .route("/stats", get(get_stats))
        .route("/cleanup", axum::routing::post(cleanup_sessions))
}

