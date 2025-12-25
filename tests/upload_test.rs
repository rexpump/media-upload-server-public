//! Upload API integration tests.

mod common;

use common::{create_test_jpeg, create_test_png, TestServer};
use reqwest::multipart;
use serde_json::Value;

#[tokio::test]
async fn test_simple_upload_png() {
    let server = TestServer::start().await;
    let client = server.client();

    let image_data = create_test_png(100, 100);

    let form = multipart::Form::new().part(
        "file",
        multipart::Part::bytes(image_data)
            .file_name("test.png")
            .mime_str("image/png")
            .unwrap(),
    );

    let response = client
        .post(server.url("/api/upload"))
        .multipart(form)
        .send()
        .await
        .expect("Failed to send request");

    assert!(
        response.status() == 200 || response.status() == 201,
        "Expected 200 or 201, got {}",
        response.status()
    );

    let json: Value = response.json().await.expect("Failed to parse JSON");

    assert!(json["id"].is_string());
    assert!(json["url"].is_string());
    assert_eq!(json["media_type"], "image");
    assert_eq!(json["mime_type"], "image/webp"); // Converted to WebP
    assert_eq!(json["width"], 100);
    assert_eq!(json["height"], 100);
}

#[tokio::test]
async fn test_simple_upload_jpeg() {
    let server = TestServer::start().await;
    let client = server.client();

    let image_data = create_test_jpeg(200, 150, 80);

    let form = multipart::Form::new().part(
        "file",
        multipart::Part::bytes(image_data)
            .file_name("photo.jpg")
            .mime_str("image/jpeg")
            .unwrap(),
    );

    let response = client
        .post(server.url("/api/upload"))
        .multipart(form)
        .send()
        .await
        .expect("Failed to send request");

    assert!(
        response.status() == 200 || response.status() == 201,
        "Expected 200 or 201, got {}",
        response.status()
    );

    let json: Value = response.json().await.expect("Failed to parse JSON");

    assert!(json["id"].is_string());
    assert_eq!(json["width"], 200);
    assert_eq!(json["height"], 150);
}

#[tokio::test]
async fn test_upload_invalid_file_type() {
    let server = TestServer::start().await;
    let client = server.client();

    let form = multipart::Form::new().part(
        "file",
        multipart::Part::bytes(b"this is not an image".to_vec())
            .file_name("test.txt")
            .mime_str("text/plain")
            .unwrap(),
    );

    let response = client
        .post(server.url("/api/upload"))
        .multipart(form)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 415); // Unsupported Media Type
}

#[tokio::test]
async fn test_upload_empty_file() {
    let server = TestServer::start().await;
    let client = server.client();

    let form = multipart::Form::new().part(
        "file",
        multipart::Part::bytes(vec![])
            .file_name("empty.png")
            .mime_str("image/png")
            .unwrap(),
    );

    let response = client
        .post(server.url("/api/upload"))
        .multipart(form)
        .send()
        .await
        .expect("Failed to send request");

    // Should fail - empty file
    assert!(response.status().is_client_error() || response.status().is_server_error());
}

#[tokio::test]
async fn test_upload_deduplication() {
    let server = TestServer::start().await;
    let client = server.client();

    let image_data = create_test_png(50, 50);

    // Upload first time
    let form1 = multipart::Form::new().part(
        "file",
        multipart::Part::bytes(image_data.clone())
            .file_name("test1.png")
            .mime_str("image/png")
            .unwrap(),
    );

    let response1 = client
        .post(server.url("/api/upload"))
        .multipart(form1)
        .send()
        .await
        .expect("Failed to send request");

    assert!(response1.status().is_success());
    let json1: Value = response1.json().await.unwrap();
    let id1 = json1["id"].as_str().unwrap();

    // Upload same image again
    let form2 = multipart::Form::new().part(
        "file",
        multipart::Part::bytes(image_data)
            .file_name("test2.png")
            .mime_str("image/png")
            .unwrap(),
    );

    let response2 = client
        .post(server.url("/api/upload"))
        .multipart(form2)
        .send()
        .await
        .expect("Failed to send request");

    assert!(response2.status().is_success());
    let json2: Value = response2.json().await.unwrap();
    let id2 = json2["id"].as_str().unwrap();

    // Should return same ID (deduplication)
    assert_eq!(id1, id2);
}

#[tokio::test]
async fn test_upload_with_auth_required() {
    let server = TestServer::start_with_auth(true, vec!["test-key-123".to_string()]).await;
    let client = server.client();

    let image_data = create_test_png(100, 100);

    // Without auth - should fail
    let form = multipart::Form::new().part(
        "file",
        multipart::Part::bytes(image_data.clone())
            .file_name("test.png")
            .mime_str("image/png")
            .unwrap(),
    );

    let response = client
        .post(server.url("/api/upload"))
        .multipart(form)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 401);

    // With auth - should succeed
    let form = multipart::Form::new().part(
        "file",
        multipart::Part::bytes(image_data)
            .file_name("test.png")
            .mime_str("image/png")
            .unwrap(),
    );

    let response = client
        .post(server.url("/api/upload"))
        .header("Authorization", "Bearer test-key-123")
        .multipart(form)
        .send()
        .await
        .expect("Failed to send request");

    assert!(response.status().is_success());
}

