//! Chunked upload integration tests.

mod common;

use common::{create_test_png, TestServer};
use serde_json::Value;

#[tokio::test]
async fn test_chunked_upload_init() {
    let server = TestServer::start().await;
    let client = server.client();

    let response = client
        .post(server.url("/api/upload/init"))
        .header("Content-Type", "application/json")
        .body(r#"{"filename":"test.png","mime_type":"image/png","total_size":10000}"#)
        .send()
        .await
        .expect("Failed to send request");

    assert!(response.status().is_success(), "Expected success, got {}", response.status());

    let json: Value = response.json().await.unwrap();
    assert!(json["id"].is_string());
    assert_eq!(json["status"], "in_progress");
    assert_eq!(json["received_bytes"], 0);
    assert_eq!(json["total_size"], 10000);
    assert!(json["chunk_size"].is_number());
    assert!(json["expires_at"].is_string());
}

#[tokio::test]
async fn test_chunked_upload_full_flow() {
    let server = TestServer::start().await;
    let client = server.client();

    let image_data = create_test_png(100, 100);
    let total_size = image_data.len();

    // Step 1: Init session
    let init_response = client
        .post(server.url("/api/upload/init"))
        .header("Content-Type", "application/json")
        .body(format!(
            r#"{{"filename":"test.png","mime_type":"image/png","total_size":{}}}"#,
            total_size
        ))
        .send()
        .await
        .expect("Failed to init");

    assert!(init_response.status().is_success());
    let init_json: Value = init_response.json().await.unwrap();
    let session_id = init_json["id"].as_str().unwrap();

    // Step 2: Upload chunk (entire file as one chunk)
    let chunk_response = client
        .patch(server.url(&format!("/api/upload/{}/chunk", session_id)))
        .header("Content-Range", format!("bytes 0-{}/{}", total_size - 1, total_size))
        .body(image_data)
        .send()
        .await
        .expect("Failed to upload chunk");

    assert!(chunk_response.status().is_success());

    // Step 3: Complete upload
    let complete_response = client
        .post(server.url(&format!("/api/upload/{}/complete", session_id)))
        .send()
        .await
        .expect("Failed to complete");

    assert!(complete_response.status().is_success());
    let complete_json: Value = complete_response.json().await.unwrap();
    assert!(complete_json["id"].is_string());
    assert!(complete_json["url"].is_string());
    assert_eq!(complete_json["mime_type"], "image/webp");
}

#[tokio::test]
async fn test_chunked_upload_multiple_chunks() {
    let server = TestServer::start().await;
    let client = server.client();

    let image_data = create_test_png(200, 200);
    let total_size = image_data.len();
    let chunk_size = total_size / 3;

    // Step 1: Init session
    let init_response = client
        .post(server.url("/api/upload/init"))
        .header("Content-Type", "application/json")
        .body(format!(
            r#"{{"filename":"test.png","mime_type":"image/png","total_size":{}}}"#,
            total_size
        ))
        .send()
        .await
        .expect("Failed to init");

    let init_json: Value = init_response.json().await.unwrap();
    let session_id = init_json["id"].as_str().unwrap();

    // Step 2: Upload in chunks
    let mut offset = 0;
    while offset < total_size {
        let end = std::cmp::min(offset + chunk_size, total_size);
        let chunk = &image_data[offset..end];

        let chunk_response = client
            .patch(server.url(&format!("/api/upload/{}/chunk", session_id)))
            .header(
                "Content-Range",
                format!("bytes {}-{}/{}", offset, end - 1, total_size),
            )
            .body(chunk.to_vec())
            .send()
            .await
            .expect("Failed to upload chunk");

        assert!(chunk_response.status().is_success());
        offset = end;
    }

    // Step 3: Complete
    let complete_response = client
        .post(server.url(&format!("/api/upload/{}/complete", session_id)))
        .send()
        .await
        .expect("Failed to complete");

    assert!(complete_response.status().is_success());
}

#[tokio::test]
async fn test_chunked_upload_status() {
    let server = TestServer::start().await;
    let client = server.client();

    let image_data = create_test_png(100, 100);
    let total_size = image_data.len();

    // Init session
    let init_response = client
        .post(server.url("/api/upload/init"))
        .header("Content-Type", "application/json")
        .body(format!(
            r#"{{"filename":"test.png","mime_type":"image/png","total_size":{}}}"#,
            total_size
        ))
        .send()
        .await
        .expect("Failed to init");

    let init_json: Value = init_response.json().await.unwrap();
    let session_id = init_json["id"].as_str().unwrap();

    // Check status
    let status_response = client
        .get(server.url(&format!("/api/upload/{}/status", session_id)))
        .send()
        .await
        .expect("Failed to get status");

    assert!(status_response.status().is_success());
    let status_json: Value = status_response.json().await.unwrap();
    assert_eq!(status_json["status"], "in_progress");
    assert_eq!(status_json["received_bytes"], 0);
}

#[tokio::test]
async fn test_chunked_upload_invalid_session() {
    let server = TestServer::start().await;
    let client = server.client();

    let response = client
        .get(server.url("/api/upload/00000000-0000-0000-0000-000000000000/status"))
        .send()
        .await
        .expect("Failed to get status");

    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn test_chunked_upload_invalid_content_range() {
    let server = TestServer::start().await;
    let client = server.client();

    // Init session
    let init_response = client
        .post(server.url("/api/upload/init"))
        .header("Content-Type", "application/json")
        .body(r#"{"filename":"test.png","mime_type":"image/png","total_size":1000}"#)
        .send()
        .await
        .expect("Failed to init");

    let init_json: Value = init_response.json().await.unwrap();
    let session_id = init_json["id"].as_str().unwrap();

    // Invalid Content-Range header
    let chunk_response = client
        .patch(server.url(&format!("/api/upload/{}/chunk", session_id)))
        .header("Content-Range", "invalid")
        .body(vec![0u8; 100])
        .send()
        .await
        .expect("Failed to upload chunk");

    assert_eq!(chunk_response.status(), 400);
}

