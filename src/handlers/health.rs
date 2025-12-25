//! Health check endpoints.
//!
//! Provides endpoints for monitoring server health and readiness.

use axum::{extract::State, routing::get, Json, Router};
use serde::Serialize;

use crate::state::AppState;

/// Health check response
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    /// Server status
    pub status: &'static str,
    /// Server version
    pub version: &'static str,
    /// Uptime message
    pub uptime: &'static str,
}

/// Liveness probe - server is running
///
/// GET /health/live
async fn liveness() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        uptime: "running",
    })
}

/// Readiness probe - server can accept requests
///
/// GET /health/ready
async fn readiness(State(state): State<AppState>) -> Json<ReadinessResponse> {
    // Check database connectivity
    let db_ok = state.db.get_media_count().is_ok();

    let status = if db_ok { "ready" } else { "not_ready" };

    Json(ReadinessResponse {
        status,
        database: if db_ok { "connected" } else { "disconnected" },
    })
}

/// Readiness response
#[derive(Debug, Serialize)]
pub struct ReadinessResponse {
    pub status: &'static str,
    pub database: &'static str,
}

/// Storage stats endpoint
///
/// GET /health/stats
async fn stats(State(state): State<AppState>) -> Json<StatsResponse> {
    let storage_stats = state.storage.get_stats().await.ok();
    let media_count = state.db.get_media_count().unwrap_or(0);

    Json(StatsResponse {
        media_count,
        storage: storage_stats,
    })
}

/// Stats response
#[derive(Debug, Serialize)]
pub struct StatsResponse {
    pub media_count: u64,
    pub storage: Option<crate::services::storage::StorageStats>,
}

/// Create health check routes
pub fn health_routes() -> Router<AppState> {
    Router::new()
        .route("/live", get(liveness))
        .route("/ready", get(readiness))
        .route("/stats", get(stats))
}

