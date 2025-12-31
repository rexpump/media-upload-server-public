//! HTTP request handlers for the media upload server.
//!
//! This module contains all endpoint handlers organized by functionality:
//! - `upload`: Handles file uploads (simple and chunked)
//! - `serve`: Serves media files to clients
//! - `admin`: Administrative endpoints (local only)
//! - `health`: Health check endpoints
//! - `rexpump`: RexPump token metadata endpoints

pub mod admin;
pub mod health;
pub mod rexpump;
pub mod serve;
pub mod upload;

pub use admin::admin_routes;
pub use health::health_routes;
pub use rexpump::{admin_rexpump_routes, rexpump_routes};
pub use serve::serve_routes;
pub use upload::upload_routes;

