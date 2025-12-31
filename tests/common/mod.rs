//! Common test utilities and helpers.

use media_upload_server::{
    config::{
        Config, LoggingConfig, ProcessingConfig, RateLimitConfig, RexPumpConfig, ServerConfig,
        StorageConfig, UploadConfig, AuthConfig,
    },
    create_admin_router, create_public_router, AppState,
};
use std::net::TcpListener;
use std::time::Duration;
use tempfile::TempDir;
use tokio::net::TcpListener as TokioTcpListener;

/// Test server instance
pub struct TestServer {
    pub public_url: String,
    pub admin_url: String,
    pub data_dir: TempDir,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl TestServer {
    /// Start a test server with random ports
    pub async fn start() -> Self {
        Self::start_with_auth(false, vec![]).await
    }

    /// Start a test server with authentication enabled
    pub async fn start_with_auth(auth_enabled: bool, api_keys: Vec<String>) -> Self {
        let public_port = get_available_port();
        let admin_port = get_available_port();
        let data_dir = TempDir::new().expect("Failed to create temp dir");

        let public_url = format!("http://127.0.0.1:{}", public_port);
        let admin_url = format!("http://127.0.0.1:{}", admin_port);

        let config = create_test_config(
            &data_dir,
            public_port,
            admin_port,
            &public_url,
            auth_enabled,
            api_keys,
        );

        let state = AppState::new(config)
            .await
            .expect("Failed to create app state");

        let public_app = create_public_router(state.clone());
        let admin_app = create_admin_router(state);

        let public_addr: std::net::SocketAddr = format!("127.0.0.1:{}", public_port)
            .parse()
            .unwrap();
        let admin_addr: std::net::SocketAddr = format!("127.0.0.1:{}", admin_port)
            .parse()
            .unwrap();

        let public_listener = TokioTcpListener::bind(public_addr)
            .await
            .expect("Failed to bind public listener");
        let admin_listener = TokioTcpListener::bind(admin_addr)
            .await
            .expect("Failed to bind admin listener");

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        // Start servers in background
        tokio::spawn(async move {
            tokio::select! {
                _ = axum::serve(public_listener, public_app) => {}
                _ = axum::serve(admin_listener, admin_app) => {}
                _ = shutdown_rx => {}
            }
        });

        // Give servers time to start
        tokio::time::sleep(Duration::from_millis(50)).await;

        Self {
            public_url,
            admin_url,
            data_dir,
            shutdown_tx: Some(shutdown_tx),
        }
    }

    /// Get HTTP client
    pub fn client(&self) -> reqwest::Client {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap()
    }

    /// Get public URL
    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.public_url, path)
    }

    /// Get admin URL
    pub fn admin(&self, path: &str) -> String {
        format!("{}{}", self.admin_url, path)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

/// Create test configuration
fn create_test_config(
    data_dir: &TempDir,
    public_port: u16,
    admin_port: u16,
    base_url: &str,
    auth_enabled: bool,
    api_keys: Vec<String>,
) -> Config {
    Config {
        server: ServerConfig {
            host: "127.0.0.1".to_string(),
            port: public_port,
            admin_host: "127.0.0.1".to_string(),
            admin_port,
            base_url: base_url.to_string(),
            request_timeout: 30,
            max_connections: 100,
            cache_max_age: 3600,
            cleanup_interval_seconds: 60,
        },
        storage: StorageConfig {
            data_dir: data_dir.path().to_path_buf(),
            originals_dir: "originals".to_string(),
            optimized_dir: "optimized".to_string(),
            temp_dir: "temp".to_string(),
            directory_levels: 2,
            database_file: String::new(),
        },
        upload: UploadConfig {
            max_simple_upload_size: 10 * 1024 * 1024,
            max_chunked_upload_size: 50 * 1024 * 1024,
            chunk_size: 1024 * 1024,
            allowed_image_types: vec![
                "image/jpeg".to_string(),
                "image/png".to_string(),
                "image/gif".to_string(),
                "image/webp".to_string(),
            ],
            allowed_video_types: vec![],
            upload_session_timeout: 300,
        },
        processing: ProcessingConfig {
            output_format: "webp".to_string(),
            output_quality: 80,
            max_image_dimension: 2048,
            keep_originals: true,
            strip_exif: true,
        },
        rate_limit: RateLimitConfig {
            enabled: false,
            requests_per_window: 1000,
            window_seconds: 60,
            uploads_per_window: 100,
        },
        logging: LoggingConfig {
            level: "warn".to_string(),
            format: "pretty".to_string(),
            file: String::new(),
        },
        auth: AuthConfig {
            enabled: auth_enabled,
            api_keys,
            protected_paths: vec!["/api/upload".to_string()],
            public_paths: vec!["/health".to_string(), "/m/".to_string()],
        },
        rexpump: RexPumpConfig::default(),
    }
}

/// Find an available TCP port
fn get_available_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("Failed to bind to random port")
        .local_addr()
        .expect("Failed to get local address")
        .port()
}

/// Create a test PNG image
pub fn create_test_png(width: u32, height: u32) -> Vec<u8> {
    use image::codecs::png::PngEncoder;
    use image::{ImageBuffer, ImageEncoder, Rgb};

    let img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(width, height, |x, y| {
        Rgb([
            ((x * 255) / width) as u8,
            ((y * 255) / height) as u8,
            128,
        ])
    });

    let mut buffer = Vec::new();
    let encoder = PngEncoder::new(&mut buffer);
    encoder
        .write_image(img.as_raw(), width, height, image::ExtendedColorType::Rgb8)
        .expect("Failed to encode PNG");

    buffer
}

/// Create a test JPEG image
pub fn create_test_jpeg(width: u32, height: u32, quality: u8) -> Vec<u8> {
    use image::codecs::jpeg::JpegEncoder;
    use image::{ImageBuffer, ImageEncoder, Rgb};

    let img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(width, height, |x, y| {
        Rgb([
            ((x * 255) / width) as u8,
            ((y * 255) / height) as u8,
            200,
        ])
    });

    let mut buffer = Vec::new();
    let encoder = JpegEncoder::new_with_quality(&mut buffer, quality);
    encoder
        .write_image(img.as_raw(), width, height, image::ExtendedColorType::Rgb8)
        .expect("Failed to encode JPEG");

    buffer
}
