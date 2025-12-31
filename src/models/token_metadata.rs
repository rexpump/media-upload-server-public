//! Token metadata model for RexPump feature.
//!
//! This module defines the data structures for storing and validating
//! token metadata in the RexPump mempad system.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, Result};

// =============================================================================
// Validation Constants
// =============================================================================

/// Maximum length for description field
pub const MAX_DESCRIPTION_LENGTH: usize = 255;
/// Maximum length for social network name
pub const MAX_SOCIAL_NAME_LENGTH: usize = 32;
/// Maximum length for social network link
pub const MAX_SOCIAL_LINK_LENGTH: usize = 256;

// =============================================================================
// Token Metadata
// =============================================================================

/// Token metadata stored in RocksDB
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenMetadata {
    /// Chain ID of the network
    pub chain_id: u64,
    /// Token contract address (normalized: lowercase, 0x prefix)
    pub token_address: String,
    /// Token description (max 255 chars)
    pub description: String,
    /// Social network links
    pub social_networks: Vec<SocialNetwork>,
    /// Reference to light theme image (UUID of media file)
    pub image_light_id: Option<Uuid>,
    /// Reference to dark theme image (UUID of media file)
    pub image_dark_id: Option<Uuid>,
    /// When metadata was first created
    pub created_at: DateTime<Utc>,
    /// When metadata was last updated
    pub updated_at: DateTime<Utc>,
    /// Address that last updated the metadata
    pub last_update_by: String,
}

impl TokenMetadata {
    /// Create a new token metadata record
    pub fn new(
        chain_id: u64,
        token_address: String,
        description: String,
        social_networks: Vec<SocialNetwork>,
        owner_address: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            chain_id,
            token_address,
            description,
            social_networks,
            image_light_id: None,
            image_dark_id: None,
            created_at: now,
            updated_at: now,
            last_update_by: owner_address,
        }
    }

    /// Get the storage key for this metadata
    pub fn storage_key(&self) -> String {
        format!("{}:{}", self.chain_id, self.token_address.to_lowercase())
    }

    /// Create storage key from chain_id and address
    pub fn make_key(chain_id: u64, address: &str) -> String {
        format!("{}:{}", chain_id, normalize_address(address))
    }
}

/// Social network link
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialNetwork {
    /// Social network name (e.g., "telegram", "twitter", "discord")
    pub name: String,
    /// Full URL to the social network profile/channel
    pub link: String,
}

// =============================================================================
// Token Lock
// =============================================================================

/// Lock status for admin control
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenLock {
    /// Chain ID of the network
    pub chain_id: u64,
    /// Token contract address
    pub token_address: String,
    /// When the lock was applied
    pub locked_at: DateTime<Utc>,
    /// Admin identifier who applied the lock
    pub locked_by: String,
    /// Type of lock applied
    pub lock_type: TokenLockType,
    /// Optional reason for the lock
    pub reason: Option<String>,
}

impl TokenLock {
    /// Create a new lock
    pub fn new(
        chain_id: u64,
        token_address: String,
        lock_type: TokenLockType,
        locked_by: String,
        reason: Option<String>,
    ) -> Self {
        Self {
            chain_id,
            token_address,
            locked_at: Utc::now(),
            locked_by,
            lock_type,
            reason,
        }
    }

    /// Get the storage key for this lock
    pub fn storage_key(&self) -> String {
        TokenMetadata::make_key(self.chain_id, &self.token_address)
    }
}

/// Type of lock applied to a token
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TokenLockType {
    /// Locked - no changes allowed, content preserved as-is
    Locked,
    /// Full lockdown - images AND JSON replaced with defaults
    LockedWithDefaults,
}

// =============================================================================
// Rate Limiting
// =============================================================================

/// Record of last update time for rate limiting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUpdateRecord {
    /// Chain ID of the network
    pub chain_id: u64,
    /// Token contract address
    pub token_address: String,
    /// When the last update occurred
    pub last_update_at: DateTime<Utc>,
}

impl TokenUpdateRecord {
    /// Create a new update record
    pub fn new(chain_id: u64, token_address: String) -> Self {
        Self {
            chain_id,
            token_address,
            last_update_at: Utc::now(),
        }
    }

    /// Check if cooldown has passed
    pub fn can_update(&self, cooldown_seconds: u64) -> bool {
        let elapsed = Utc::now()
            .signed_duration_since(self.last_update_at)
            .num_seconds();
        elapsed >= cooldown_seconds as i64
    }

    /// Get seconds until next allowed update
    pub fn seconds_until_update(&self, cooldown_seconds: u64) -> i64 {
        let elapsed = Utc::now()
            .signed_duration_since(self.last_update_at)
            .num_seconds();
        (cooldown_seconds as i64 - elapsed).max(0)
    }
}

// =============================================================================
// API Request/Response Types
// =============================================================================

/// Request body for metadata JSON (from client)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataInput {
    /// Token description
    pub description: String,
    /// Social network links
    #[serde(default)]
    pub social_networks: Vec<SocialNetwork>,
}

/// Response for GET metadata endpoint
#[derive(Debug, Clone, Serialize)]
pub struct MetadataResponse {
    pub chain_id: u64,
    pub token_address: String,
    pub description: String,
    pub social_networks: Vec<SocialNetwork>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_light_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_dark_url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl MetadataResponse {
    /// Create response from TokenMetadata
    pub fn from_metadata(meta: &TokenMetadata, base_url: &str) -> Self {
        Self {
            chain_id: meta.chain_id,
            token_address: meta.token_address.clone(),
            description: meta.description.clone(),
            social_networks: meta.social_networks.clone(),
            image_light_url: meta.image_light_id.map(|id| format!("{}/m/{}", base_url, id)),
            image_dark_url: meta.image_dark_id.map(|id| format!("{}/m/{}", base_url, id)),
            created_at: meta.created_at,
            updated_at: meta.updated_at,
        }
    }

    /// Create default/locked response
    pub fn default_locked(chain_id: u64, token_address: &str, base_url: &str) -> Self {
        Self {
            chain_id,
            token_address: token_address.to_string(),
            description: String::new(),
            social_networks: vec![],
            image_light_url: Some(format!("{}/m/default", base_url)),
            image_dark_url: Some(format!("{}/m/default", base_url)),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}

/// Admin lock request body
#[derive(Debug, Clone, Deserialize)]
pub struct LockRequest {
    pub lock_type: TokenLockType,
    #[serde(default)]
    pub reason: Option<String>,
}

// =============================================================================
// Validation Functions
// =============================================================================

/// Normalize an Ethereum address to lowercase with 0x prefix
pub fn normalize_address(addr: &str) -> String {
    let addr = addr.trim();
    if addr.starts_with("0x") || addr.starts_with("0X") {
        format!("0x{}", &addr[2..].to_lowercase())
    } else {
        format!("0x{}", addr.to_lowercase())
    }
}

/// Validate an Ethereum address format
pub fn validate_address(addr: &str) -> Result<String> {
    let normalized = normalize_address(addr);
    
    // Check length (0x + 40 hex chars)
    if normalized.len() != 42 {
        return Err(AppError::validation(format!(
            "Invalid address length: expected 42 chars, got {}",
            normalized.len()
        )));
    }
    
    // Check all chars after 0x are valid hex
    if !normalized[2..].chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::validation("Invalid address: contains non-hex characters"));
    }
    
    Ok(normalized)
}

/// Validate metadata input
pub fn validate_metadata_input(input: &MetadataInput) -> Result<()> {
    // Validate description
    if input.description.len() > MAX_DESCRIPTION_LENGTH {
        return Err(AppError::validation(format!(
            "Description too long: max {} chars, got {}",
            MAX_DESCRIPTION_LENGTH,
            input.description.len()
        )));
    }
    
    // Validate social networks
    for (i, sn) in input.social_networks.iter().enumerate() {
        if sn.name.is_empty() {
            return Err(AppError::validation(format!(
                "Social network #{}: name cannot be empty",
                i + 1
            )));
        }
        if sn.name.len() > MAX_SOCIAL_NAME_LENGTH {
            return Err(AppError::validation(format!(
                "Social network #{}: name too long (max {} chars)",
                i + 1,
                MAX_SOCIAL_NAME_LENGTH
            )));
        }
        if sn.link.is_empty() {
            return Err(AppError::validation(format!(
                "Social network #{}: link cannot be empty",
                i + 1
            )));
        }
        if sn.link.len() > MAX_SOCIAL_LINK_LENGTH {
            return Err(AppError::validation(format!(
                "Social network #{}: link too long (max {} chars)",
                i + 1,
                MAX_SOCIAL_LINK_LENGTH
            )));
        }
        // Basic URL validation
        if !sn.link.starts_with("http://") && !sn.link.starts_with("https://") {
            return Err(AppError::validation(format!(
                "Social network #{}: link must start with http:// or https://",
                i + 1
            )));
        }
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_address() {
        assert_eq!(
            normalize_address("0xABCD1234"),
            "0xabcd1234"
        );
        assert_eq!(
            normalize_address("0XABCD1234"),
            "0xabcd1234"
        );
        assert_eq!(
            normalize_address("abcd1234"),
            "0xabcd1234"
        );
    }

    #[test]
    fn test_validate_address() {
        // Valid address
        let valid = "0x1234567890123456789012345678901234567890";
        assert!(validate_address(valid).is_ok());
        
        // Too short
        let short = "0x1234";
        assert!(validate_address(short).is_err());
        
        // Invalid chars
        let invalid = "0x123456789012345678901234567890123456789G";
        assert!(validate_address(invalid).is_err());
    }

    #[test]
    fn test_validate_metadata_input() {
        let valid = MetadataInput {
            description: "Test token".to_string(),
            social_networks: vec![
                SocialNetwork {
                    name: "telegram".to_string(),
                    link: "https://t.me/test".to_string(),
                }
            ],
        };
        assert!(validate_metadata_input(&valid).is_ok());
        
        // Description too long
        let long_desc = MetadataInput {
            description: "x".repeat(300),
            social_networks: vec![],
        };
        assert!(validate_metadata_input(&long_desc).is_err());
        
        // Invalid URL
        let bad_url = MetadataInput {
            description: "Test".to_string(),
            social_networks: vec![
                SocialNetwork {
                    name: "test".to_string(),
                    link: "not-a-url".to_string(),
                }
            ],
        };
        assert!(validate_metadata_input(&bad_url).is_err());
    }

    #[test]
    fn test_update_record_cooldown() {
        let record = TokenUpdateRecord::new(1, "0x123".to_string());
        
        // Just created, should not be able to update with 60s cooldown
        assert!(!record.can_update(60));
        
        // But with 0s cooldown, should be able to update
        assert!(record.can_update(0));
    }

    #[test]
    fn test_storage_key() {
        let meta = TokenMetadata::new(
            32769,
            "0xABCD1234".to_string(),
            "Test".to_string(),
            vec![],
            "0xOwner".to_string(),
        );
        
        // Key should be normalized
        assert!(meta.storage_key().contains("32769:0x"));
    }
}
