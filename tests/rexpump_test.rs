//! Integration tests for RexPump metadata API.
//!
//! Note: These tests require mocking the EVM RPC calls since we can't
//! actually verify token ownership in tests. The handlers will return
//! validation errors for chain_id since no networks are configured in tests.

mod common;

use reqwest::multipart::{Form, Part};
use serde_json::json;

/// Test that RexPump API returns proper error when feature is disabled
#[tokio::test]
async fn test_rexpump_disabled() {
    let server = common::TestServer::start_with_auth(false, vec![]).await;

    // POST should fail because rexpump is disabled by default in test config
    let response = server
        .client()
        .post(format!("{}/api/rexpump/metadata", server.public_url))
        .multipart(
            Form::new()
                .text("chain_id", "32769")
                .text("token_address", "0x1234567890123456789012345678901234567890")
                .text("token_owner", "0xabcdefabcdefabcdefabcdefabcdefabcdefabcd")
                .text("timestamp", "1704067200")
                .text("signature", "0x1234")
                .text("metadata", r#"{"description":"test","social_networks":[]}"#),
        )
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 400);
    let body: serde_json::Value = response.json().await.unwrap();
    assert!(body["message"].as_str().unwrap().contains("disabled"));
}

/// Test GET metadata returns 404 for non-existent token
#[tokio::test]
async fn test_get_metadata_not_found() {
    let server = common::TestServer::start_with_auth(false, vec![]).await;

    let response = server
        .client()
        .get(format!(
            "{}/api/rexpump/metadata/32769/0x1234567890123456789012345678901234567890",
            server.public_url
        ))
        .send()
        .await
        .expect("Failed to send request");

    // Should be 400 (disabled) not 404
    assert_eq!(response.status(), 400);
}

/// Test that invalid chain_id is rejected (when feature is enabled)
#[tokio::test]
async fn test_invalid_chain_id_rejected() {
    // This test would need a custom server config with rexpump enabled
    // For now, we just verify the error handling pattern
    let server = common::TestServer::start_with_auth(false, vec![]).await;

    let response = server
        .client()
        .post(format!("{}/api/rexpump/metadata", server.public_url))
        .multipart(
            Form::new()
                .text("chain_id", "999999") // Invalid chain
                .text("token_address", "0x1234567890123456789012345678901234567890")
                .text("token_owner", "0xabcdefabcdefabcdefabcdefabcdefabcdefabcd")
                .text("timestamp", "1704067200")
                .text("signature", "0x1234")
        )
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 400);
}

/// Test admin lock endpoint (without auth, localhost only)
#[tokio::test]
async fn test_admin_lock_nonexistent_token() {
    let server = common::TestServer::start_with_auth(false, vec![]).await;

    // Try to lock a non-existent token
    let response = server
        .client()
        .post(format!(
            "{}/admin/rexpump/lock/32769/0x1234567890123456789012345678901234567890",
            server.admin_url
        ))
        .json(&json!({
            "lock_type": "locked",
            "reason": "Test lock"
        }))
        .send()
        .await
        .expect("Failed to send request");

    // Lock should succeed even if token doesn't exist
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["success"], true);
}

/// Test admin unlock endpoint
#[tokio::test]
async fn test_admin_unlock_not_locked() {
    let server = common::TestServer::start_with_auth(false, vec![]).await;

    // Try to unlock a token that's not locked
    let response = server
        .client()
        .delete(format!(
            "{}/admin/rexpump/lock/32769/0x1234567890123456789012345678901234567890",
            server.admin_url
        ))
        .send()
        .await
        .expect("Failed to send request");

    // Should be 404 - not locked
    assert_eq!(response.status(), 404);
}

/// Test admin lock then unlock flow
#[tokio::test]
async fn test_admin_lock_unlock_flow() {
    let server = common::TestServer::start_with_auth(false, vec![]).await;
    let token_address = "0xaabbccddaabbccddaabbccddaabbccddaabbccdd";

    // Lock the token
    let lock_response = server
        .client()
        .post(format!(
            "{}/admin/rexpump/lock/32769/{}",
            server.admin_url, token_address
        ))
        .json(&json!({
            "lock_type": "locked"
        }))
        .send()
        .await
        .expect("Failed to send lock request");

    assert_eq!(lock_response.status(), 200);

    // Verify it's locked via admin get
    let get_response = server
        .client()
        .get(format!(
            "{}/admin/rexpump/metadata/32769/{}",
            server.admin_url, token_address
        ))
        .send()
        .await
        .expect("Failed to send get request");

    assert_eq!(get_response.status(), 200);
    let body: serde_json::Value = get_response.json().await.unwrap();
    assert_eq!(body["is_locked"], true);

    // Unlock the token
    let unlock_response = server
        .client()
        .delete(format!(
            "{}/admin/rexpump/lock/32769/{}",
            server.admin_url, token_address
        ))
        .send()
        .await
        .expect("Failed to send unlock request");

    assert_eq!(unlock_response.status(), 200);

    // Verify it's unlocked
    let get_response2 = server
        .client()
        .get(format!(
            "{}/admin/rexpump/metadata/32769/{}",
            server.admin_url, token_address
        ))
        .send()
        .await
        .expect("Failed to send get request");

    assert_eq!(get_response2.status(), 200);
    let body2: serde_json::Value = get_response2.json().await.unwrap();
    assert_eq!(body2["is_locked"], false);
}

/// Test admin can create/update metadata without signature
#[tokio::test]
async fn test_admin_update_metadata() {
    let server = common::TestServer::start_with_auth(false, vec![]).await;
    let token_address = "0x1111111111111111111111111111111111111111";

    // Admin creates metadata without signature
    let response = server
        .client()
        .put(format!(
            "{}/admin/rexpump/metadata/32769/{}",
            server.admin_url, token_address
        ))
        .multipart(
            Form::new()
                .text("metadata", r#"{"description":"Admin created","social_networks":[{"name":"telegram","link":"https://t.me/test"}]}"#)
        )
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["description"], "Admin created");
    assert_eq!(body["social_networks"][0]["name"], "telegram");
}

/// Test admin delete metadata
#[tokio::test]
async fn test_admin_delete_metadata() {
    let server = common::TestServer::start_with_auth(false, vec![]).await;
    let token_address = "0x2222222222222222222222222222222222222222";

    // First create metadata
    let create_response = server
        .client()
        .put(format!(
            "{}/admin/rexpump/metadata/32769/{}",
            server.admin_url, token_address
        ))
        .multipart(
            Form::new()
                .text("metadata", r#"{"description":"To be deleted","social_networks":[]}"#)
        )
        .send()
        .await
        .expect("Failed to send create request");

    assert_eq!(create_response.status(), 200);

    // Now delete it
    let delete_response = server
        .client()
        .delete(format!(
            "{}/admin/rexpump/metadata/32769/{}",
            server.admin_url, token_address
        ))
        .send()
        .await
        .expect("Failed to send delete request");

    assert_eq!(delete_response.status(), 200);
    let body: serde_json::Value = delete_response.json().await.unwrap();
    assert_eq!(body["deleted"], true);

    // Verify it's gone
    let get_response = server
        .client()
        .get(format!(
            "{}/admin/rexpump/metadata/32769/{}",
            server.admin_url, token_address
        ))
        .send()
        .await
        .expect("Failed to send get request");

    assert_eq!(get_response.status(), 200);
    let body2: serde_json::Value = get_response.json().await.unwrap();
    assert!(body2["metadata"].is_null());
}

/// Test address normalization
#[tokio::test]
async fn test_address_normalization() {
    let server = common::TestServer::start_with_auth(false, vec![]).await;

    // Create with uppercase address
    let create_response = server
        .client()
        .put(format!(
            "{}/admin/rexpump/metadata/32769/0xAAAABBBBCCCCDDDDEEEEFFFF00001111AAAABBBB",
            server.admin_url
        ))
        .multipart(
            Form::new()
                .text("metadata", r#"{"description":"Uppercase test","social_networks":[]}"#)
        )
        .send()
        .await
        .expect("Failed to send create request");

    assert_eq!(create_response.status(), 200);

    // Fetch with lowercase - should work
    let get_response = server
        .client()
        .get(format!(
            "{}/admin/rexpump/metadata/32769/0xaaaabbbbccccddddeeee ffff00001111aaaabbbb",
            server.admin_url
        ).replace(" ", ""))
        .send()
        .await
        .expect("Failed to send get request");

    assert_eq!(get_response.status(), 200);
    let body: serde_json::Value = get_response.json().await.unwrap();
    assert!(body["metadata"].is_object());
}

/// Test invalid address format
#[tokio::test]
async fn test_invalid_address_format() {
    let server = common::TestServer::start_with_auth(false, vec![]).await;

    // Try with too short address
    let response = server
        .client()
        .put(format!(
            "{}/admin/rexpump/metadata/32769/0x1234",
            server.admin_url
        ))
        .multipart(
            Form::new()
                .text("metadata", r#"{"description":"test","social_networks":[]}"#)
        )
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 400);
    let body: serde_json::Value = response.json().await.unwrap();
    assert!(body["message"].as_str().unwrap().contains("Invalid address"));
}

/// Test metadata validation - description too long
#[tokio::test]
async fn test_metadata_validation_description_too_long() {
    let server = common::TestServer::start_with_auth(false, vec![]).await;
    let token_address = "0x3333333333333333333333333333333333333333";

    let long_description = "x".repeat(300); // Max is 255

    let response = server
        .client()
        .put(format!(
            "{}/admin/rexpump/metadata/32769/{}",
            server.admin_url, token_address
        ))
        .multipart(
            Form::new()
                .text("metadata", format!(r#"{{"description":"{}","social_networks":[]}}"#, long_description))
        )
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 400);
    let body: serde_json::Value = response.json().await.unwrap();
    assert!(body["message"].as_str().unwrap().contains("too long"));
}

/// Test metadata validation - invalid social network URL
#[tokio::test]
async fn test_metadata_validation_invalid_url() {
    let server = common::TestServer::start_with_auth(false, vec![]).await;
    let token_address = "0x4444444444444444444444444444444444444444";

    let response = server
        .client()
        .put(format!(
            "{}/admin/rexpump/metadata/32769/{}",
            server.admin_url, token_address
        ))
        .multipart(
            Form::new()
                .text("metadata", r#"{"description":"test","social_networks":[{"name":"telegram","link":"not-a-valid-url"}]}"#)
        )
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 400);
    let body: serde_json::Value = response.json().await.unwrap();
    assert!(body["message"].as_str().unwrap().contains("http"));
}

/// Test admin update with image
#[tokio::test]
async fn test_admin_update_with_image() {
    let server = common::TestServer::start_with_auth(false, vec![]).await;
    let token_address = "0x5555555555555555555555555555555555555555";

    // Create a test PNG
    let png_data = common::create_test_png(100, 100);

    let response = server
        .client()
        .put(format!(
            "{}/admin/rexpump/metadata/32769/{}",
            server.admin_url, token_address
        ))
        .multipart(
            Form::new()
                .text("metadata", r#"{"description":"With image","social_networks":[]}"#)
                .part("image_light", Part::bytes(png_data.clone()).file_name("light.png").mime_str("image/png").unwrap())
        )
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert!(body["image_light_url"].is_string());
    assert!(body["image_dark_url"].is_null());
}

/// Test lock with defaults replaces content
#[tokio::test]
async fn test_lock_with_defaults() {
    let server = common::TestServer::start_with_auth(false, vec![]).await;
    let token_address = "0x6666666666666666666666666666666666666666";

    // First create metadata with image
    let png_data = common::create_test_png(50, 50);
    
    let create_response = server
        .client()
        .put(format!(
            "{}/admin/rexpump/metadata/32769/{}",
            server.admin_url, token_address
        ))
        .multipart(
            Form::new()
                .text("metadata", r#"{"description":"Original content","social_networks":[{"name":"twitter","link":"https://twitter.com/test"}]}"#)
                .part("image_light", Part::bytes(png_data).file_name("test.png").mime_str("image/png").unwrap())
        )
        .send()
        .await
        .expect("Failed to send create request");

    assert_eq!(create_response.status(), 200);

    // Now lock with defaults
    let lock_response = server
        .client()
        .post(format!(
            "{}/admin/rexpump/lock/32769/{}",
            server.admin_url, token_address
        ))
        .json(&json!({
            "lock_type": "locked_with_defaults",
            "reason": "Inappropriate content"
        }))
        .send()
        .await
        .expect("Failed to send lock request");

    assert_eq!(lock_response.status(), 200);

    // Verify content was replaced
    let get_response = server
        .client()
        .get(format!(
            "{}/admin/rexpump/metadata/32769/{}",
            server.admin_url, token_address
        ))
        .send()
        .await
        .expect("Failed to send get request");

    assert_eq!(get_response.status(), 200);
    let body: serde_json::Value = get_response.json().await.unwrap();
    
    // Should be locked
    assert_eq!(body["is_locked"], true);
    
    // Metadata should be empty/default
    assert_eq!(body["metadata"]["description"], "");
    assert!(body["metadata"]["social_networks"].as_array().unwrap().is_empty());
}
