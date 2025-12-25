//! Service layer for the media upload server.
//!
//! This module contains business logic services that handle:
//! - File storage operations
//! - Image processing and optimization
//! - Database operations

pub mod database;
pub mod image_processor;
pub mod storage;

pub use database::DatabaseService;
pub use image_processor::ImageProcessor;
pub use storage::{StorageService, StorageStats};

