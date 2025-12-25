//! Rate limiting middleware.
//!
//! Provides IP-based rate limiting using the token bucket algorithm.
//! Configuration is loaded from the config file.
//!
//! # Example
//!
//! ```rust,ignore
//! let rate_limiter = RateLimiter::new(&config.rate_limit);
//! let app = Router::new()
//!     .route("/api/upload", post(upload))
//!     .layer(rate_limiter.layer());
//! ```

use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
};
use dashmap::DashMap;
use governor::{
    clock::DefaultClock,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter as GovRateLimiter,
};
use std::{
    net::{IpAddr, SocketAddr},
    num::NonZeroU32,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};
use tower::{Layer, Service};
use tracing::warn;

use crate::config::RateLimitConfig;

/// Rate limiter state shared across requests
#[derive(Clone)]
pub struct RateLimiter {
    /// Per-IP rate limiters
    limiters: Arc<DashMap<IpAddr, Arc<GovRateLimiter<NotKeyed, InMemoryState, DefaultClock>>>>,
    /// Requests per window
    requests_per_window: u32,
    /// Window duration
    window_seconds: u64,
    /// Whether rate limiting is enabled
    enabled: bool,
}

impl RateLimiter {
    /// Create a new rate limiter from configuration
    pub fn new(config: &RateLimitConfig) -> Self {
        Self {
            limiters: Arc::new(DashMap::new()),
            requests_per_window: config.requests_per_window,
            window_seconds: config.window_seconds,
            enabled: config.enabled,
        }
    }

    /// Create a Tower Layer for this rate limiter
    pub fn layer(&self) -> RateLimiterLayer {
        RateLimiterLayer {
            rate_limiter: self.clone(),
        }
    }

    /// Check if a request from the given IP is allowed
    pub fn check(&self, ip: IpAddr) -> bool {
        if !self.enabled {
            return true;
        }

        let limiter = self.get_or_create_limiter(ip);

        limiter.check().is_ok()
    }

    /// Get or create a rate limiter for an IP
    fn get_or_create_limiter(
        &self,
        ip: IpAddr,
    ) -> Arc<GovRateLimiter<NotKeyed, InMemoryState, DefaultClock>> {
        if let Some(limiter) = self.limiters.get(&ip) {
            return Arc::clone(&limiter);
        }

        // Create new limiter for this IP
        let quota = Quota::with_period(Duration::from_secs(self.window_seconds))
            .unwrap()
            .allow_burst(NonZeroU32::new(self.requests_per_window).unwrap());

        let limiter = Arc::new(GovRateLimiter::direct(quota));
        self.limiters.insert(ip, Arc::clone(&limiter));

        limiter
    }

    /// Clean up old limiters (call periodically)
    pub fn cleanup(&self) {
        // Remove limiters that haven't been used recently
        // In production, you might want more sophisticated cleanup
        if self.limiters.len() > 10000 {
            self.limiters.clear();
        }
    }
}

/// Tower Layer for rate limiting
#[derive(Clone)]
pub struct RateLimiterLayer {
    rate_limiter: RateLimiter,
}

impl<S> Layer<S> for RateLimiterLayer {
    type Service = RateLimiterMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RateLimiterMiddleware {
            inner,
            rate_limiter: self.rate_limiter.clone(),
        }
    }
}

/// Rate limiting middleware service
#[derive(Clone)]
pub struct RateLimiterMiddleware<S> {
    inner: S,
    rate_limiter: RateLimiter,
}

impl<S> Service<Request<Body>> for RateLimiterMiddleware<S>
where
    S: Service<Request<Body>, Response = Response> + Send + Clone + 'static,
    S::Future: Send + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        // Extract client IP from various sources
        let ip = extract_client_ip(&req);

        let rate_limiter = self.rate_limiter.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            // Check rate limit
            if !rate_limiter.check(ip) {
                warn!(ip = %ip, "Rate limit exceeded");
                return Ok(rate_limit_response());
            }

            // Proceed with request
            inner.call(req).await
        })
    }
}

/// Extract client IP from request
fn extract_client_ip<B>(req: &Request<B>) -> IpAddr {
    // Try X-Forwarded-For header first (for reverse proxy setups)
    if let Some(forwarded) = req.headers().get("x-forwarded-for") {
        if let Ok(forwarded_str) = forwarded.to_str() {
            // Take the first IP in the chain
            if let Some(first_ip) = forwarded_str.split(',').next() {
                if let Ok(ip) = first_ip.trim().parse::<IpAddr>() {
                    return ip;
                }
            }
        }
    }

    // Try X-Real-IP header
    if let Some(real_ip) = req.headers().get("x-real-ip") {
        if let Ok(ip_str) = real_ip.to_str() {
            if let Ok(ip) = ip_str.parse::<IpAddr>() {
                return ip;
            }
        }
    }

    // Try to get from connection info (requires ConnectInfo extractor)
    if let Some(connect_info) = req.extensions().get::<ConnectInfo<SocketAddr>>() {
        return connect_info.0.ip();
    }

    // Fallback to localhost
    "127.0.0.1".parse().unwrap()
}

/// Create rate limit exceeded response
fn rate_limit_response() -> Response {
    let body = serde_json::json!({
        "error": "rate_limit_exceeded",
        "message": "Too many requests. Please try again later.",
        "status": 429
    });

    (
        StatusCode::TOO_MANY_REQUESTS,
        [
            ("content-type", "application/json"),
            ("retry-after", "60"),
        ],
        body.to_string(),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter_enabled() {
        let config = RateLimitConfig {
            enabled: true,
            requests_per_window: 5,
            window_seconds: 60,
            uploads_per_window: 3,
        };

        let limiter = RateLimiter::new(&config);
        let ip: IpAddr = "192.168.1.1".parse().unwrap();

        // First 5 requests should succeed
        for _ in 0..5 {
            assert!(limiter.check(ip));
        }

        // 6th request should fail
        assert!(!limiter.check(ip));
    }

    #[test]
    fn test_rate_limiter_disabled() {
        let config = RateLimitConfig {
            enabled: false,
            requests_per_window: 1,
            window_seconds: 60,
            uploads_per_window: 1,
        };

        let limiter = RateLimiter::new(&config);
        let ip: IpAddr = "192.168.1.1".parse().unwrap();

        // All requests should succeed when disabled
        for _ in 0..100 {
            assert!(limiter.check(ip));
        }
    }
}

