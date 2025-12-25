//! Media Upload Server - Entry Point
//!
//! This is the main entry point for the media upload server.
//! It initializes logging, loads configuration, and starts the HTTP servers.

use media_upload_server::{config::Config, run};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load configuration
    let config = Config::load_default()?;

    // Initialize logging
    init_logging(&config.logging)?;

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        "Starting Media Upload Server"
    );

    // Run the server
    run(config).await
}

/// Initialize logging based on configuration
fn init_logging(config: &media_upload_server::config::LoggingConfig) -> anyhow::Result<()> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&config.level));

    match config.format.as_str() {
        "json" => {
            tracing_subscriber::registry()
                .with(filter)
                .with(fmt::layer().json())
                .init();
        }
        _ => {
            tracing_subscriber::registry()
                .with(filter)
                .with(fmt::layer().pretty())
                .init();
        }
    }

    Ok(())
}
