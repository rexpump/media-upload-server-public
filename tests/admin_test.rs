//! Admin API integration tests.

mod common;

use common::{create_test_png, TestServer};
use reqwest::multipart;
use serde_json::Value;

#[tokio::test]
async fn test_admin_delete_media() {
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

    // Verify it exists
    let serve_response = client
        .get(server.url(&format!("/m/{}", id)))
        .send()
        .await
        .expect("Failed to fetch");
    assert_eq!(serve_response.status(), 200);

    // Delete via admin API
    let delete_response = client
        .delete(server.admin(&format!("/admin/media/{}", id)))
        .send()
        .await
        .expect("Failed to delete");

    assert_eq!(delete_response.status(), 200);

    // Verify it's gone
    let serve_response = client
        .get(server.url(&format!("/m/{}", id)))
        .send()
        .await
        .expect("Failed to fetch");
    assert_eq!(serve_response.status(), 404);
}

#[tokio::test]
async fn test_admin_delete_nonexistent() {
    let server = TestServer::start().await;
    let client = server.client();

    let response = client
        .delete(server.admin("/admin/media/00000000-0000-0000-0000-000000000000"))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn test_admin_stats() {
    let server = TestServer::start().await;
    let client = server.client();

    // Upload a few images
    for i in 0..3 {
        let image_data = create_test_png(50 + i * 10, 50 + i * 10);
        let form = multipart::Form::new().part(
            "file",
            multipart::Part::bytes(image_data)
                .file_name(format!("test{}.png", i))
                .mime_str("image/png")
                .unwrap(),
        );

        let resp = client
            .post(server.url("/api/upload"))
            .multipart(form)
            .send()
            .await
            .expect("Failed to upload");
        assert!(resp.status().is_success());
    }

    // Get stats
    let stats_response = client
        .get(server.admin("/admin/stats"))
        .send()
        .await
        .expect("Failed to get stats");

    assert_eq!(stats_response.status(), 200);

    let stats: Value = stats_response.json().await.unwrap();
    assert_eq!(stats["media_count"], 3);
    assert!(stats["storage"]["originals_size"].is_number());
    assert!(stats["storage"]["optimized_size"].is_number());
}

#[tokio::test]
async fn test_admin_get_media_info() {
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

    // Get media info via admin API
    let info_response = client
        .get(server.admin(&format!("/admin/media/{}", id)))
        .send()
        .await
        .expect("Failed to get info");

    assert_eq!(info_response.status(), 200);

    let info: Value = info_response.json().await.unwrap();
    assert_eq!(info["id"], id);
    assert_eq!(info["original_mime_type"], "image/png");
    assert_eq!(info["optimized_mime_type"], "image/webp");
    assert_eq!(info["width"], 100);
    assert_eq!(info["height"], 100);
    assert!(info["content_hash"].is_string());
    assert!(info["created_at"].is_string());
}

