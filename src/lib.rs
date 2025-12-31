//! # Media Upload Server
//!
//! A high-performance media upload and serving server written in Rust.
//!
//! ## Features
//!
//! - **Simple Upload**: Single-request file upload for small files
//! - **Chunked Upload**: Resumable uploads for large files
//! - **Image Processing**: Automatic WebP conversion and optimization
//! - **Content Deduplication**: Avoids storing duplicate files
//! - **Admin API**: Content moderation endpoints
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────┐
//! │                  HTTP Server                     │
//! │  ┌─────────────┐ ┌─────────────┐ ┌───────────┐ │
//! │  │ Upload API  │ │ Serve API   │ │ Admin API │ │
//! │  └─────────────┘ └─────────────┘ └───────────┘ │
//! ├─────────────────────────────────────────────────┤
//! │                   Services                       │
//! │  ┌─────────────┐ ┌─────────────┐ ┌───────────┐ │
//! │  │  Storage    │ │   Image     │ │ Database  │ │
//! │  │  Service    │ │  Processor  │ │  Service  │ │
//! │  └─────────────┘ └─────────────┘ └───────────┘ │
//! ├─────────────────────────────────────────────────┤
//! │              File System / RocksDB               │
//! └─────────────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```bash
//! # Start the server
//! cargo run --release
//!
//! # Upload an image
//! curl -X POST http://localhost:3000/api/upload -F "file=@image.jpg"
//!
//! # Get the image
//! curl http://localhost:3000/m/{id}
//! ```

pub mod config;
pub mod error;
pub mod handlers;
pub mod middleware;
pub mod models;
pub mod services;
pub mod state;

pub use config::{AuthConfig, Config};
pub use error::{AppError, Result};
pub use middleware::{ApiKeyAuth, RateLimiter};
pub use state::AppState;

use axum::Router;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::TcpListener;
use tower_http::{
    cors::{Any, CorsLayer},
    limit::RequestBodyLimitLayer,
    trace::TraceLayer,
};
use tracing::info;

/// Run the media upload server with the given configuration.
///
/// This function starts both the public and admin API servers.
pub async fn run(config: Config) -> anyhow::Result<()> {
    // Create application state
    let state = AppState::new(config.clone()).await?;

    // Create public API router
    let public_app = create_public_router(state.clone());

    // Create admin API router
    let admin_app = create_admin_router(state.clone());

    // Start servers
    let public_addr: SocketAddr = format!("{}:{}", config.server.host, config.server.port)
        .parse()
        .expect("Invalid public server address");

    let admin_addr: SocketAddr =
        format!("{}:{}", config.server.admin_host, config.server.admin_port)
            .parse()
            .expect("Invalid admin server address");

    info!(
        address = %public_addr,
        "Public API server starting"
    );

    info!(
        address = %admin_addr,
        "Admin API server starting"
    );

    // Start cleanup task
    let cleanup_state = state.clone();
    tokio::spawn(async move {
        cleanup_task(cleanup_state).await;
    });

    // Run both servers concurrently
    let public_listener = TcpListener::bind(public_addr).await?;
    let admin_listener = TcpListener::bind(admin_addr).await?;

    tokio::select! {
        result = axum::serve(public_listener, public_app) => {
            if let Err(e) = result {
                tracing::error!(error = %e, "Public server error");
            }
        }
        result = axum::serve(admin_listener, admin_app) => {
            if let Err(e) = result {
                tracing::error!(error = %e, "Admin server error");
            }
        }
    }

    Ok(())
}

/// Create the public API router
pub fn create_public_router(state: AppState) -> Router {
    // CORS configuration
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Body size limit (from config)
    let body_limit = RequestBodyLimitLayer::new(
        state.config.upload.max_chunked_upload_size as usize + 1024,
    );

    // Rate limiter (from config)
    let rate_limiter = RateLimiter::new(&state.config.rate_limit);

    // API key authentication (from config)
    let api_auth = ApiKeyAuth::new(&state.config.auth);

    // Log auth status
    if state.config.auth.enabled {
        info!(
            keys_count = state.config.auth.api_keys.len(),
            "API key authentication enabled"
        );
    }

    if state.config.rate_limit.enabled {
        info!(
            requests_per_window = state.config.rate_limit.requests_per_window,
            window_seconds = state.config.rate_limit.window_seconds,
            "Rate limiting enabled"
        );
    }

    Router::new()
        .nest("/api/upload", handlers::upload_routes())
        .nest("/api/rexpump", handlers::rexpump_routes())
        .nest("/m", handlers::serve_routes())
        .nest("/health", handlers::health_routes())
        .layer(cors)
        .layer(body_limit)
        .layer(api_auth.layer())
        .layer(rate_limiter.layer())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Create the admin API router (localhost only)
pub fn create_admin_router(state: AppState) -> Router {
    Router::new()
        .nest("/admin", handlers::admin_routes())
        .nest("/admin/rexpump", handlers::admin_rexpump_routes())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Background task for periodic cleanup
async fn cleanup_task(state: AppState) {
    let interval = Duration::from_secs(state.cleanup_interval());

    loop {
        tokio::time::sleep(interval).await;

        if let Ok(ids) = state.db.cleanup_expired_sessions() {
            for id in ids {
                let _ = state.storage.delete_temp_session(id).await;
            }
        }

        let _ = state
            .storage
            .cleanup_expired_sessions(state.upload_session_timeout())
            .await;
    }
}

