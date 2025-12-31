//! RexPump token metadata API handlers.
//!
//! This module provides endpoints for managing RexPump token metadata:
//! - Public API for token owners to manage their metadata
//! - Admin API for moderation and control

use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    routing::{delete, get, post, put},
    Json, Router,
};
use chrono::Utc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::models::{
    validate_address, validate_metadata_input, LockRequest, MetadataInput, MetadataResponse,
    TokenLock, TokenLockType, TokenMetadata,
};
use crate::services::evm_service::EvmService;
use crate::services::image_processor::{calculate_hash, ImageProcessor};
use crate::state::AppState;

// =============================================================================
// Public API Handlers
// =============================================================================

/// POST /api/rexpump/metadata - Create or update token metadata
///
/// Multipart form fields:
/// - chain_id: Network chain ID
/// - token_address: Token contract address
/// - token_owner: Expected owner address (for verification)
/// - timestamp: Unix timestamp (for signature freshness)
/// - signature: personal_sign signature
/// - metadata: JSON string with description and social_networks (optional if only updating images)
/// - image_light: Light theme image (optional)
/// - image_dark: Dark theme image (optional)
async fn upsert_metadata(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<MetadataResponse>)> {
    // Check if feature is enabled
    if !state.config.rexpump.enabled {
        return Err(AppError::validation("RexPump feature is disabled"));
    }

    // Parse multipart form
    let mut chain_id: Option<u64> = None;
    let mut token_address: Option<String> = None;
    let mut token_owner: Option<String> = None;
    let mut timestamp: Option<u64> = None;
    let mut signature: Option<String> = None;
    let mut metadata_json: Option<String> = None;
    let mut image_light_data: Option<Vec<u8>> = None;
    let mut image_dark_data: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::validation(format!("Invalid multipart data: {}", e)))?
    {
        let name = field.name().unwrap_or("").to_string();
        
        match name.as_str() {
            "chain_id" => {
                let text = field.text().await
                    .map_err(|e| AppError::validation(format!("Invalid chain_id: {}", e)))?;
                chain_id = Some(text.parse()
                    .map_err(|_| AppError::validation("chain_id must be a number"))?);
            }
            "token_address" => {
                token_address = Some(field.text().await
                    .map_err(|e| AppError::validation(format!("Invalid token_address: {}", e)))?);
            }
            "token_owner" => {
                token_owner = Some(field.text().await
                    .map_err(|e| AppError::validation(format!("Invalid token_owner: {}", e)))?);
            }
            "timestamp" => {
                let text = field.text().await
                    .map_err(|e| AppError::validation(format!("Invalid timestamp: {}", e)))?;
                timestamp = Some(text.parse()
                    .map_err(|_| AppError::validation("timestamp must be a number"))?);
            }
            "signature" => {
                signature = Some(field.text().await
                    .map_err(|e| AppError::validation(format!("Invalid signature: {}", e)))?);
            }
            "metadata" => {
                metadata_json = Some(field.text().await
                    .map_err(|e| AppError::validation(format!("Invalid metadata: {}", e)))?);
            }
            "image_light" => {
                let data = field.bytes().await
                    .map_err(|e| AppError::validation(format!("Failed to read image_light: {}", e)))?;
                if !data.is_empty() {
                    image_light_data = Some(data.to_vec());
                }
            }
            "image_dark" => {
                let data = field.bytes().await
                    .map_err(|e| AppError::validation(format!("Failed to read image_dark: {}", e)))?;
                if !data.is_empty() {
                    image_dark_data = Some(data.to_vec());
                }
            }
            _ => {
                // Ignore unknown fields
            }
        }
    }

    // Validate required fields
    let chain_id = chain_id.ok_or_else(|| AppError::validation("chain_id is required"))?;
    let token_address = token_address.ok_or_else(|| AppError::validation("token_address is required"))?;
    let token_owner = token_owner.ok_or_else(|| AppError::validation("token_owner is required"))?;
    let timestamp = timestamp.ok_or_else(|| AppError::validation("timestamp is required"))?;
    let signature = signature.ok_or_else(|| AppError::validation("signature is required"))?;

    // Validate chain_id is supported
    if !state.config.rexpump.is_chain_supported(chain_id) {
        return Err(AppError::validation(format!("Chain {} is not supported", chain_id)));
    }

    // Normalize addresses
    let token_address = validate_address(&token_address)?;
    let token_owner = validate_address(&token_owner)?;

    // Validate timestamp freshness (anti-replay)
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let max_age = state.config.rexpump.signature_max_age_seconds;
    
    if now > timestamp && now - timestamp > max_age {
        return Err(AppError::InvalidSignature(format!(
            "Signature timestamp too old (max {} seconds)",
            max_age
        )));
    }
    if timestamp > now + 60 {
        return Err(AppError::InvalidSignature("Signature timestamp is in the future".to_string()));
    }

    // Verify signature
    let message = EvmService::build_sign_message(chain_id, &token_address, timestamp);
    let recovered_signer = EvmService::recover_signer(&message, &signature)
        .map_err(|e| AppError::InvalidSignature(format!("Failed to recover signer: {}", e)))?;

    if recovered_signer != token_owner {
        return Err(AppError::InvalidSignature(format!(
            "Signature signer {} does not match token_owner {}",
            recovered_signer, token_owner
        )));
    }

    // Verify signer is the token creator via EVM RPC
    let is_owner = state.evm.verify_token_owner(chain_id, &token_address, &token_owner).await?;
    if !is_owner {
        return Err(AppError::NotAuthorized(format!(
            "Address {} is not the creator of token {}",
            token_owner, token_address
        )));
    }

    // Check if token is locked
    if let Some(lock) = state.db.get_token_lock(chain_id, &token_address)? {
        return Err(AppError::TokenLocked(format!(
            "Token is locked since {} (type: {:?})",
            lock.locked_at, lock.lock_type
        )));
    }

    // Check rate limit
    let cooldown = state.config.rexpump.update_cooldown_seconds;
    if !state.db.can_update_token(chain_id, &token_address, cooldown)? {
        let wait = state.db.seconds_until_update(chain_id, &token_address, cooldown)?;
        return Err(AppError::UpdateCooldown(format!(
            "Please wait {} seconds before updating again",
            wait
        )));
    }

    // Must have metadata or at least one image
    if metadata_json.is_none() && image_light_data.is_none() && image_dark_data.is_none() {
        return Err(AppError::validation("Must provide metadata or at least one image"));
    }

    // Get existing metadata or create new
    let mut metadata = state.db.get_token_metadata(chain_id, &token_address)?
        .unwrap_or_else(|| TokenMetadata::new(
            chain_id,
            token_address.clone(),
            String::new(),
            vec![],
            token_owner.clone(),
        ));

    // Update metadata JSON if provided
    if let Some(json_str) = metadata_json {
        let input: MetadataInput = serde_json::from_str(&json_str)
            .map_err(|e| AppError::validation(format!("Invalid metadata JSON: {}", e)))?;
        
        validate_metadata_input(&input)?;
        
        metadata.description = input.description;
        metadata.social_networks = input.social_networks;
    }

    // Process and store images
    if let Some(data) = image_light_data {
        // Delete old image if exists
        if let Some(old_id) = metadata.image_light_id {
            delete_media_files(&state, old_id).await;
        }
        
        let media = process_and_store_image(&state, &data).await?;
        metadata.image_light_id = Some(media.id);
    }

    if let Some(data) = image_dark_data {
        // Delete old image if exists
        if let Some(old_id) = metadata.image_dark_id {
            delete_media_files(&state, old_id).await;
        }
        
        let media = process_and_store_image(&state, &data).await?;
        metadata.image_dark_id = Some(media.id);
    }

    // Update timestamps
    metadata.updated_at = Utc::now();
    metadata.last_update_by = token_owner;

    // Save metadata
    state.db.upsert_token_metadata(&metadata)?;

    // Record update for rate limiting
    state.db.record_token_update(chain_id, &token_address)?;

    info!(
        chain_id = chain_id,
        token = %token_address,
        "Updated token metadata"
    );

    let response = MetadataResponse::from_metadata(&metadata, state.base_url());
    Ok((StatusCode::OK, Json(response)))
}

/// GET /api/rexpump/metadata/{chain_id}/{token_address}
async fn get_metadata(
    State(state): State<AppState>,
    Path((chain_id, token_address)): Path<(u64, String)>,
) -> Result<Json<MetadataResponse>> {
    // Check if feature is enabled
    if !state.config.rexpump.enabled {
        return Err(AppError::validation("RexPump feature is disabled"));
    }

    let token_address = validate_address(&token_address)?;

    // Check if locked with defaults
    if let Some(lock) = state.db.get_token_lock(chain_id, &token_address)? {
        if lock.lock_type == TokenLockType::LockedWithDefaults {
            // Return default response
            return Ok(Json(MetadataResponse::default_locked(
                chain_id,
                &token_address,
                state.base_url(),
            )));
        }
    }

    // Get metadata
    let metadata = state.db.get_token_metadata(chain_id, &token_address)?
        .ok_or_else(|| AppError::not_found(format!(
            "Metadata not found for {}:{}", chain_id, token_address
        )))?;

    Ok(Json(MetadataResponse::from_metadata(&metadata, state.base_url())))
}

// =============================================================================
// Admin API Handlers
// =============================================================================

/// POST /admin/rexpump/lock/{chain_id}/{token_address}
async fn admin_lock_token(
    State(state): State<AppState>,
    Path((chain_id, token_address)): Path<(u64, String)>,
    Json(request): Json<LockRequest>,
) -> Result<Json<serde_json::Value>> {
    let token_address = validate_address(&token_address)?;

    // If locking with defaults, replace content
    if request.lock_type == TokenLockType::LockedWithDefaults {
        // Get existing metadata to delete images
        if let Some(metadata) = state.db.get_token_metadata(chain_id, &token_address)? {
            if let Some(id) = metadata.image_light_id {
                delete_media_files(&state, id).await;
            }
            if let Some(id) = metadata.image_dark_id {
                delete_media_files(&state, id).await;
            }
        }

        // Create empty metadata (or update existing to empty)
        let metadata = TokenMetadata::new(
            chain_id,
            token_address.clone(),
            String::new(),
            vec![],
            "admin".to_string(),
        );
        state.db.upsert_token_metadata(&metadata)?;
    }

    // Create lock
    let lock = TokenLock::new(
        chain_id,
        token_address.clone(),
        request.lock_type.clone(),
        "admin".to_string(),
        request.reason,
    );
    state.db.lock_token(&lock)?;

    info!(
        chain_id = chain_id,
        token = %token_address,
        lock_type = ?request.lock_type,
        "Admin locked token"
    );

    Ok(Json(serde_json::json!({
        "success": true,
        "locked_at": lock.locked_at,
        "lock_type": lock.lock_type
    })))
}

/// DELETE /admin/rexpump/lock/{chain_id}/{token_address}
async fn admin_unlock_token(
    State(state): State<AppState>,
    Path((chain_id, token_address)): Path<(u64, String)>,
) -> Result<Json<serde_json::Value>> {
    let token_address = validate_address(&token_address)?;

    let unlocked = state.db.unlock_token(chain_id, &token_address)?;
    
    if !unlocked {
        return Err(AppError::not_found("Token is not locked"));
    }

    info!(
        chain_id = chain_id,
        token = %token_address,
        "Admin unlocked token"
    );

    Ok(Json(serde_json::json!({
        "success": true,
        "unlocked": true
    })))
}

/// GET /admin/rexpump/metadata/{chain_id}/{token_address}
async fn admin_get_metadata(
    State(state): State<AppState>,
    Path((chain_id, token_address)): Path<(u64, String)>,
) -> Result<Json<serde_json::Value>> {
    let token_address = validate_address(&token_address)?;

    let metadata = state.db.get_token_metadata(chain_id, &token_address)?;
    let lock = state.db.get_token_lock(chain_id, &token_address)?;

    Ok(Json(serde_json::json!({
        "metadata": metadata,
        "lock": lock,
        "is_locked": lock.is_some()
    })))
}

/// PUT /admin/rexpump/metadata/{chain_id}/{token_address}
/// Admin edit without signature verification
async fn admin_update_metadata(
    State(state): State<AppState>,
    Path((chain_id, token_address)): Path<(u64, String)>,
    mut multipart: Multipart,
) -> Result<Json<MetadataResponse>> {
    let token_address = validate_address(&token_address)?;

    // Parse multipart
    let mut metadata_json: Option<String> = None;
    let mut image_light_data: Option<Vec<u8>> = None;
    let mut image_dark_data: Option<Vec<u8>> = None;
    let mut remove_image_light = false;
    let mut remove_image_dark = false;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::validation(format!("Invalid multipart data: {}", e)))?
    {
        let name = field.name().unwrap_or("").to_string();
        
        match name.as_str() {
            "metadata" => {
                metadata_json = Some(field.text().await
                    .map_err(|e| AppError::validation(format!("Invalid metadata: {}", e)))?);
            }
            "image_light" => {
                let data = field.bytes().await
                    .map_err(|e| AppError::validation(format!("Failed to read image_light: {}", e)))?;
                if !data.is_empty() {
                    image_light_data = Some(data.to_vec());
                }
            }
            "image_dark" => {
                let data = field.bytes().await
                    .map_err(|e| AppError::validation(format!("Failed to read image_dark: {}", e)))?;
                if !data.is_empty() {
                    image_dark_data = Some(data.to_vec());
                }
            }
            "remove_image_light" => {
                let text = field.text().await.unwrap_or_default();
                remove_image_light = text == "true" || text == "1";
            }
            "remove_image_dark" => {
                let text = field.text().await.unwrap_or_default();
                remove_image_dark = text == "true" || text == "1";
            }
            _ => {}
        }
    }

    // Get or create metadata
    let mut metadata = state.db.get_token_metadata(chain_id, &token_address)?
        .unwrap_or_else(|| TokenMetadata::new(
            chain_id,
            token_address.clone(),
            String::new(),
            vec![],
            "admin".to_string(),
        ));

    // Update JSON if provided
    if let Some(json_str) = metadata_json {
        let input: MetadataInput = serde_json::from_str(&json_str)
            .map_err(|e| AppError::validation(format!("Invalid metadata JSON: {}", e)))?;
        
        validate_metadata_input(&input)?;
        
        metadata.description = input.description;
        metadata.social_networks = input.social_networks;
    }

    // Handle image removal
    if remove_image_light {
        if let Some(id) = metadata.image_light_id.take() {
            delete_media_files(&state, id).await;
        }
    }
    if remove_image_dark {
        if let Some(id) = metadata.image_dark_id.take() {
            delete_media_files(&state, id).await;
        }
    }

    // Handle new images
    if let Some(data) = image_light_data {
        if let Some(old_id) = metadata.image_light_id {
            delete_media_files(&state, old_id).await;
        }
        let media = process_and_store_image(&state, &data).await?;
        metadata.image_light_id = Some(media.id);
    }
    if let Some(data) = image_dark_data {
        if let Some(old_id) = metadata.image_dark_id {
            delete_media_files(&state, old_id).await;
        }
        let media = process_and_store_image(&state, &data).await?;
        metadata.image_dark_id = Some(media.id);
    }

    // Update timestamps
    metadata.updated_at = Utc::now();
    metadata.last_update_by = "admin".to_string();

    state.db.upsert_token_metadata(&metadata)?;

    info!(
        chain_id = chain_id,
        token = %token_address,
        "Admin updated token metadata"
    );

    Ok(Json(MetadataResponse::from_metadata(&metadata, state.base_url())))
}

/// DELETE /admin/rexpump/metadata/{chain_id}/{token_address}
async fn admin_delete_metadata(
    State(state): State<AppState>,
    Path((chain_id, token_address)): Path<(u64, String)>,
) -> Result<Json<serde_json::Value>> {
    let token_address = validate_address(&token_address)?;

    // Delete associated images
    if let Some(metadata) = state.db.get_token_metadata(chain_id, &token_address)? {
        if let Some(id) = metadata.image_light_id {
            delete_media_files(&state, id).await;
        }
        if let Some(id) = metadata.image_dark_id {
            delete_media_files(&state, id).await;
        }
    }

    let deleted = state.db.delete_token_metadata(chain_id, &token_address)?;
    
    if !deleted {
        return Err(AppError::not_found("Metadata not found"));
    }

    info!(
        chain_id = chain_id,
        token = %token_address,
        "Admin deleted token metadata"
    );

    Ok(Json(serde_json::json!({
        "success": true,
        "deleted": true
    })))
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Process and store an uploaded image (reuses existing logic)
async fn process_and_store_image(state: &AppState, data: &[u8]) -> Result<crate::models::Media> {
    use crate::models::Media;

    // Calculate hash for deduplication
    let content_hash = calculate_hash(data);

    // Check for duplicate
    if let Some(existing) = state.db.find_by_hash(&content_hash)? {
        debug!(id = %existing.id, "Found duplicate image");
        return Ok(existing);
    }

    // Process image
    let processed = state.image_processor.process(data, &state.config.upload)?;

    // Create media record
    let media = Media::new(
        "token_image".to_string(),
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
        state.storage.save_original(media.id, original_ext, &processed.original_data).await?;
    }
    state.storage.save_optimized(media.id, output_ext, &processed.optimized_data).await?;

    // Save to database
    state.db.insert_media(&media)?;

    debug!(id = %media.id, "Stored token image");
    Ok(media)
}

/// Delete media files for a given ID
async fn delete_media_files(state: &AppState, id: Uuid) {
    if let Ok(Some(media)) = state.db.get_media(id) {
        let original_ext = ImageProcessor::mime_to_extension(&media.original_mime_type);
        let output_ext = state.output_extension();
        
        if let Err(e) = state.storage.delete_media_files(id, original_ext, output_ext).await {
            warn!(id = %id, error = %e, "Failed to delete media files");
        }
        
        if let Err(e) = state.db.delete_media(id) {
            warn!(id = %id, error = %e, "Failed to delete media record");
        }
    }
}

// =============================================================================
// Routes
// =============================================================================

/// Public RexPump routes
pub fn rexpump_routes() -> Router<AppState> {
    Router::new()
        .route("/metadata", post(upsert_metadata))
        .route("/metadata/{chain_id}/{token_address}", get(get_metadata))
}

/// Admin RexPump routes
pub fn admin_rexpump_routes() -> Router<AppState> {
    Router::new()
        .route("/lock/{chain_id}/{token_address}", post(admin_lock_token))
        .route("/lock/{chain_id}/{token_address}", delete(admin_unlock_token))
        .route("/metadata/{chain_id}/{token_address}", get(admin_get_metadata))
        .route("/metadata/{chain_id}/{token_address}", put(admin_update_metadata))
        .route("/metadata/{chain_id}/{token_address}", delete(admin_delete_metadata))
}
