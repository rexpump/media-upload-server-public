//! Media serving integration tests.

mod common;

use common::{create_test_png, TestServer};
use reqwest::multipart;
use serde_json::Value;

#[tokio::test]
async fn test_serve_uploaded_image() {
    let server = TestServer::start().await;
    let client = server.client();

    // Upload an image
    let image_data = create_test_png(100, 100);
    let form = multipart::Form::new().part(
        "file",
        multipart::Part::bytes(image_data)
            .file_name("test.png")
            .mime_str("image/png")
            .unwrap(),
    );

    let upload_response = client
        .post(server.url("/api/upload"))
        .multipart(form)
        .send()
        .await
        .expect("Failed to upload");

    assert!(upload_response.status().is_success());
    let json: Value = upload_response.json().await.unwrap();
    let id = json["id"].as_str().unwrap();

    // Fetch the image
    let serve_response = client
        .get(server.url(&format!("/m/{}", id)))
        .send()
        .await
        .expect("Failed to fetch");

    assert_eq!(serve_response.status(), 200);
    assert_eq!(
        serve_response.headers().get("content-type").unwrap(),
        "image/webp"
    );
    assert!(serve_response
        .headers()
        .get("cache-control")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("max-age="));
    assert!(serve_response.headers().get("etag").is_some());

    let body = serve_response.bytes().await.unwrap();
    assert!(!body.is_empty());
}

#[tokio::test]
async fn test_serve_original_image() {
    let server = TestServer::start().await;
    let client = server.client();

    // Upload an image
    let image_data = create_test_png(100, 100);
    let form = multipart::Form::new().part(
        "file",
        multipart::Part::bytes(image_data)
            .file_name("test.png")
            .mime_str("image/png")
            .unwrap(),
    );

    let upload_response = client
        .post(server.url("/api/upload"))
        .multipart(form)
        .send()
        .await
        .expect("Failed to upload");

    assert!(upload_response.status().is_success());
    let json: Value = upload_response.json().await.unwrap();
    let id = json["id"].as_str().unwrap();

    // Fetch the original
    let serve_response = client
        .get(server.url(&format!("/m/{}/original", id)))
        .send()
        .await
        .expect("Failed to fetch");

    assert_eq!(serve_response.status(), 200);
    assert_eq!(
        serve_response.headers().get("content-type").unwrap(),
        "image/png"
    );
}

#[tokio::test]
async fn test_serve_nonexistent_image() {
    let server = TestServer::start().await;
    let client = server.client();

    let response = client
        .get(server.url("/m/00000000-0000-0000-0000-000000000000"))
        .send()
        .await
        .expect("Failed to fetch");

    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn test_serve_invalid_uuid() {
    let server = TestServer::start().await;
    let client = server.client();

    let response = client
        .get(server.url("/m/not-a-valid-uuid"))
        .send()
        .await
        .expect("Failed to fetch");

    assert_eq!(response.status(), 400);
}

#[tokio::test]
async fn test_etag_caching() {
    let server = TestServer::start().await;
    let client = server.client();

    // Upload an image
    let image_data = create_test_png(100, 100);
    let form = multipart::Form::new().part(
        "file",
        multipart::Part::bytes(image_data)
            .file_name("test.png")
            .mime_str("image/png")
            .unwrap(),
    );

    let upload_response = client
        .post(server.url("/api/upload"))
        .multipart(form)
        .send()
        .await
        .expect("Failed to upload");

    assert!(upload_response.status().is_success());
    let json: Value = upload_response.json().await.unwrap();
    let id = json["id"].as_str().unwrap();

    // First request - get ETag
    let response1 = client
        .get(server.url(&format!("/m/{}", id)))
        .send()
        .await
        .expect("Failed to fetch");

    assert_eq!(response1.status(), 200);
    let etag = response1
        .headers()
        .get("etag")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    // Second request with If-None-Match - should get 304
    let response2 = client
        .get(server.url(&format!("/m/{}", id)))
        .header("If-None-Match", &etag)
        .send()
        .await
        .expect("Failed to fetch");

    assert_eq!(response2.status(), 304);
}

