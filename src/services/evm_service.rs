//! EVM service for RexPump signature verification and RPC calls.
//!
//! This module handles:
//! - Personal signature recovery (EIP-191)
//! - RPC calls to get token creator via `creator()` function
//! - Network configuration with fallback support

use crate::config::EvmNetworkConfig;
use crate::error::{AppError, Result};
use alloy_primitives::{PrimitiveSignature, B256};
use std::collections::HashMap;
use std::time::Duration;
use tracing::{debug, warn};

/// EVM service for blockchain interactions
#[derive(Debug, Clone)]
pub struct EvmService {
    /// Network configurations indexed by chain_id
    networks: HashMap<u64, EvmNetworkConfig>,
    /// HTTP client for RPC calls
    client: reqwest::Client,
}

impl EvmService {
    /// Create a new EVM service
    pub fn new(networks: HashMap<String, EvmNetworkConfig>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("Failed to create HTTP client");

        // Index by chain_id for fast lookup
        let networks_by_chain_id: HashMap<u64, EvmNetworkConfig> = networks
            .into_iter()
            .map(|(_, v)| (v.chain_id, v))
            .collect();

        Self {
            networks: networks_by_chain_id,
            client,
        }
    }

    /// Check if chain is supported
    pub fn is_chain_supported(&self, chain_id: u64) -> bool {
        self.networks.contains_key(&chain_id)
    }

    /// Get network config by chain_id
    pub fn get_network(&self, chain_id: u64) -> Option<&EvmNetworkConfig> {
        self.networks.get(&chain_id)
    }

    /// Recover signer address from personal_sign signature
    ///
    /// Message format expected:
    /// "RexPump Metadata Update\nChain: {chain_id}\nToken: {token_address}\nTimestamp: {timestamp}"
    ///
    /// # Arguments
    /// * `message` - The original message that was signed
    /// * `signature` - Hex-encoded signature (with or without 0x prefix)
    ///
    /// # Returns
    /// Recovered signer address (lowercase, 0x-prefixed)
    pub fn recover_signer(message: &str, signature: &str) -> Result<String> {
        // Decode signature bytes
        let sig_bytes = decode_hex(signature)?;
        
        if sig_bytes.len() != 65 {
            return Err(AppError::validation(format!(
                "Invalid signature length: expected 65 bytes, got {}",
                sig_bytes.len()
            )));
        }

        // Parse signature (r, s, v)
        let sig = PrimitiveSignature::try_from(sig_bytes.as_slice())
            .map_err(|e| AppError::validation(format!("Invalid signature format: {}", e)))?;

        // Create EIP-191 personal message hash
        // The prefixed message is: "\x19Ethereum Signed Message:\n" + len(message) + message
        let prefixed_message = format!(
            "\x19Ethereum Signed Message:\n{}{}",
            message.len(),
            message
        );
        let message_hash = keccak256(prefixed_message.as_bytes());

        // Recover the signer address
        let recovered = sig
            .recover_address_from_prehash(&B256::from_slice(&message_hash))
            .map_err(|e| AppError::validation(format!("Failed to recover signer: {}", e)))?;

        Ok(format!("{:?}", recovered).to_lowercase())
    }

    /// Build the message that should be signed
    pub fn build_sign_message(chain_id: u64, token_address: &str, timestamp: u64) -> String {
        format!(
            "RexPump Metadata Update\nChain: {}\nToken: {}\nTimestamp: {}",
            chain_id,
            token_address.to_lowercase(),
            timestamp
        )
    }

    /// Get token creator by calling creator() on the token contract
    ///
    /// # Arguments
    /// * `chain_id` - Network chain ID
    /// * `token_address` - Token contract address
    ///
    /// # Returns
    /// Creator address (lowercase, 0x-prefixed)
    pub async fn get_token_creator(
        &self,
        chain_id: u64,
        token_address: &str,
    ) -> Result<String> {
        let network = self.networks.get(&chain_id).ok_or_else(|| {
            AppError::validation(format!("Chain {} is not supported", chain_id))
        })?;

        // Try primary RPC first
        match self.call_creator(&network.rpc_url, token_address).await {
            Ok(creator) => return Ok(creator),
            Err(e) => {
                warn!(
                    chain_id = chain_id,
                    rpc = %network.rpc_url,
                    error = %e,
                    "Primary RPC failed"
                );
            }
        }

        // Try fallback if available
        if let Some(fallback_url) = &network.fallback_rpc_url {
            match self.call_creator(fallback_url, token_address).await {
                Ok(creator) => return Ok(creator),
                Err(e) => {
                    warn!(
                        chain_id = chain_id,
                        rpc = %fallback_url,
                        error = %e,
                        "Fallback RPC failed"
                    );
                }
            }
        }

        Err(AppError::internal(format!(
            "EVM service unavailable for chain {}",
            chain_id
        )))
    }

    /// Verify that signer is the token creator
    pub async fn verify_token_owner(
        &self,
        chain_id: u64,
        token_address: &str,
        signer_address: &str,
    ) -> Result<bool> {
        let creator = self.get_token_creator(chain_id, token_address).await?;
        
        // Normalize both addresses for comparison
        let creator_normalized = creator.to_lowercase();
        let signer_normalized = signer_address.to_lowercase();
        
        debug!(
            creator = %creator_normalized,
            signer = %signer_normalized,
            "Comparing addresses"
        );
        
        Ok(creator_normalized == signer_normalized)
    }

    /// Call creator() function on a token contract
    async fn call_creator(&self, rpc_url: &str, token_address: &str) -> Result<String> {
        // Function selector for creator() = keccak256("creator()")[0:4]
        // creator() = 0x02d05d3f
        let call_data = "0x02d05d3f";

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_call",
            "params": [{
                "to": token_address,
                "data": call_data
            }, "latest"],
            "id": 1
        });

        let response = self
            .client
            .post(rpc_url)
            .json(&request)
            .send()
            .await
            .map_err(|e| AppError::internal(format!("RPC request failed: {}", e)))?;

        let response_json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| AppError::internal(format!("Failed to parse RPC response: {}", e)))?;

        // Check for error
        if let Some(error) = response_json.get("error") {
            return Err(AppError::internal(format!(
                "RPC error: {}",
                error.get("message").unwrap_or(&serde_json::Value::Null)
            )));
        }

        // Extract result
        let result = response_json
            .get("result")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::internal("Invalid RPC response: missing result"))?;

        // Result is 32 bytes (64 hex chars + 0x), address is last 20 bytes
        if result.len() < 66 {
            return Err(AppError::internal(format!(
                "Invalid creator() response length: {}",
                result.len()
            )));
        }

        // Extract address from the last 40 hex characters
        let address = format!("0x{}", &result[result.len() - 40..]).to_lowercase();
        
        debug!(token = %token_address, creator = %address, "Got token creator");
        
        Ok(address)
    }
}

/// Decode hex string to bytes
fn decode_hex(hex: &str) -> Result<Vec<u8>> {
    let hex = hex.strip_prefix("0x").unwrap_or(hex);
    hex::decode(hex).map_err(|e| AppError::validation(format!("Invalid hex: {}", e)))
}

/// Compute keccak256 hash
fn keccak256(data: &[u8]) -> [u8; 32] {
    use alloy_primitives::keccak256 as alloy_keccak;
    *alloy_keccak(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_sign_message() {
        let msg = EvmService::build_sign_message(
            32769,
            "0x1234567890123456789012345678901234567890",
            1704067200,
        );
        
        assert!(msg.contains("Chain: 32769"));
        assert!(msg.contains("Token: 0x1234567890123456789012345678901234567890"));
        assert!(msg.contains("Timestamp: 1704067200"));
    }

    #[test]
    fn test_decode_hex() {
        let result = decode_hex("0x1234").unwrap();
        assert_eq!(result, vec![0x12, 0x34]);
        
        let result = decode_hex("1234").unwrap();
        assert_eq!(result, vec![0x12, 0x34]);
        
        assert!(decode_hex("invalid").is_err());
    }

    #[test]
    fn test_is_chain_supported() {
        let mut networks = HashMap::new();
        networks.insert(
            "test".to_string(),
            EvmNetworkConfig {
                name: "test".to_string(),
                chain_id: 1,
                rpc_url: "http://localhost:8545".to_string(),
                fallback_rpc_url: None,
            },
        );
        
        let service = EvmService::new(networks);
        
        assert!(service.is_chain_supported(1));
        assert!(!service.is_chain_supported(2));
    }
}
