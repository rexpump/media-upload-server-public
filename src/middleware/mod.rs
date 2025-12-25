//! Middleware components for the media upload server.
//!
//! This module contains middleware for:
//! - Rate limiting
//! - API key authentication

pub mod auth;
pub mod rate_limit;

pub use auth::ApiKeyAuth;
pub use rate_limit::{RateLimiter, RateLimiterLayer};

