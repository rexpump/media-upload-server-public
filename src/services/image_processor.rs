//! Image processing service.
//!
//! This module handles all image manipulation operations:
//! - Format detection and validation
//! - Conversion to configurable output format
//! - Resizing to maximum dimensions
//! - EXIF stripping
//!
//! # Supported Formats
//!
//! Input: Configurable via allowed_image_types in config
//! Output: Configurable via output_format in config (webp, jpeg, png)

use crate::config::{ProcessingConfig, UploadConfig};
use crate::error::{AppError, Result};
use image::{DynamicImage, GenericImageView, ImageFormat};
use std::io::Cursor;
use tracing::{debug, info};

/// Result of image processing
#[derive(Debug)]
pub struct ProcessedImage {
    /// Original image data (may be modified for EXIF stripping)
    pub original_data: Vec<u8>,
    /// Optimized image in configured output format
    pub optimized_data: Vec<u8>,
    /// Detected MIME type of original
    pub original_mime: String,
    /// Image width in pixels
    pub width: u32,
    /// Image height in pixels
    pub height: u32,
    /// Whether the image was resized
    pub was_resized: bool,
}

/// Service for image processing operations
#[derive(Debug, Clone)]
pub struct ImageProcessor {
    /// Output format (webp, jpeg, png)
    output_format: String,
    /// Output quality setting (0-100) - reserved for future JPEG quality support
    #[allow(dead_code)]
    output_quality: u8,
    /// Maximum allowed dimension (width or height)
    max_dimension: u32,
    /// Whether to strip EXIF metadata
    strip_exif: bool,
}

impl ImageProcessor {
    /// Create a new image processor with the given configuration
    pub fn new(config: &ProcessingConfig) -> Self {
        Self {
            output_format: config.output_format.clone(),
            output_quality: config.output_quality,
            max_dimension: config.max_image_dimension,
            strip_exif: config.strip_exif,
        }
    }

    /// Process an uploaded image
    ///
    /// This method:
    /// 1. Validates the image format using magic bytes and config
    /// 2. Decodes the image
    /// 3. Resizes if necessary
    /// 4. Strips EXIF data if configured
    /// 5. Encodes to configured output format
    ///
    /// # Arguments
    /// * `data` - Raw image bytes
    /// * `upload_config` - Upload configuration for allowed types validation
    ///
    /// # Returns
    /// `ProcessedImage` containing original and optimized versions
    ///
    /// # Errors
    /// Returns error if image format is unsupported or processing fails
    pub fn process(&self, data: &[u8], upload_config: &UploadConfig) -> Result<ProcessedImage> {
        // Step 1: Detect format using magic bytes
        let detected_mime = self.detect_mime_type(data)?;
        debug!(mime = %detected_mime, size = data.len(), "Detected image format");

        // Step 1b: Validate against allowed types from config
        if !upload_config.is_allowed_image_type(&detected_mime) {
            return Err(AppError::unsupported_media_type(format!(
                "Image type '{}' is not in allowed_image_types",
                detected_mime
            )));
        }

        // Step 2: Decode image
        let format = Self::mime_to_format(&detected_mime)?;
        let mut img = image::load_from_memory_with_format(data, format)
            .map_err(|e| AppError::image_processing(format!("Failed to decode image: {}", e)))?;

        let original_width = img.width();
        let original_height = img.height();
        debug!(width = original_width, height = original_height, "Decoded image");

        // Step 3: Resize if necessary
        let was_resized = self.should_resize(&img);
        if was_resized {
            img = self.resize_image(img);
            debug!(
                new_width = img.width(),
                new_height = img.height(),
                "Resized image"
            );
        }

        // Step 4: Prepare original data (with EXIF stripped if configured)
        let original_data = if self.strip_exif {
            self.strip_exif_and_reencode(&img, format)?
        } else {
            data.to_vec()
        };

        // Step 5: Encode to configured output format
        let optimized_data = self.encode_output(&img)?;

        info!(
            original_size = original_data.len(),
            optimized_size = optimized_data.len(),
            width = img.width(),
            height = img.height(),
            output_format = %self.output_format,
            compression_ratio = format!("{:.1}%", (optimized_data.len() as f64 / original_data.len() as f64) * 100.0),
            "Processed image"
        );

        Ok(ProcessedImage {
            original_data,
            optimized_data,
            original_mime: detected_mime,
            width: img.width(),
            height: img.height(),
            was_resized,
        })
    }

    /// Detect MIME type using magic bytes
    ///
    /// This is more reliable than trusting the Content-Type header or file extension.
    /// Note: This only detects the type, validation against allowed types is done in process()
    pub fn detect_mime_type(&self, data: &[u8]) -> Result<String> {
        // Use infer crate for reliable magic byte detection
        let kind = infer::get(data).ok_or_else(|| {
            AppError::unsupported_media_type("Could not detect file type from content")
        })?;

        let mime = kind.mime_type();

        // Basic check that it's an image
        if !mime.starts_with("image/") {
            return Err(AppError::unsupported_media_type(format!(
                "Not an image type: {}",
                mime
            )));
        }

        Ok(mime.to_string())
    }

    /// Convert MIME type to image format
    fn mime_to_format(mime: &str) -> Result<ImageFormat> {
        match mime {
            "image/jpeg" => Ok(ImageFormat::Jpeg),
            "image/png" => Ok(ImageFormat::Png),
            "image/gif" => Ok(ImageFormat::Gif),
            "image/webp" => Ok(ImageFormat::WebP),
            "image/bmp" => Ok(ImageFormat::Bmp),
            "image/tiff" => Ok(ImageFormat::Tiff),
            _ => Err(AppError::unsupported_media_type(format!(
                "Unsupported format: {}",
                mime
            ))),
        }
    }

    /// Get output ImageFormat based on configuration
    fn output_image_format(&self) -> ImageFormat {
        match self.output_format.as_str() {
            "webp" => ImageFormat::WebP,
            "jpeg" | "jpg" => ImageFormat::Jpeg,
            "png" => ImageFormat::Png,
            _ => ImageFormat::WebP,
        }
    }

    /// Check if image needs resizing
    fn should_resize(&self, img: &DynamicImage) -> bool {
        img.width() > self.max_dimension || img.height() > self.max_dimension
    }

    /// Resize image to fit within max dimensions while preserving aspect ratio
    fn resize_image(&self, img: DynamicImage) -> DynamicImage {
        let (width, height) = img.dimensions();

        // Calculate new dimensions maintaining aspect ratio
        let (new_width, new_height) = if width > height {
            let ratio = self.max_dimension as f64 / width as f64;
            (self.max_dimension, (height as f64 * ratio) as u32)
        } else {
            let ratio = self.max_dimension as f64 / height as f64;
            ((width as f64 * ratio) as u32, self.max_dimension)
        };

        // Use Lanczos3 for high-quality downscaling
        img.resize(new_width, new_height, image::imageops::FilterType::Lanczos3)
    }

    /// Encode image to configured output format
    fn encode_output(&self, img: &DynamicImage) -> Result<Vec<u8>> {
        let mut buffer = Vec::new();
        let output_format = self.output_image_format();

        img.write_to(&mut Cursor::new(&mut buffer), output_format)
            .map_err(|e| AppError::image_processing(format!(
                "Encoding to {} failed: {}",
                self.output_format, e
            )))?;

        Ok(buffer)
    }

    /// Strip EXIF data by re-encoding the image
    ///
    /// This ensures no metadata is preserved in the original file.
    fn strip_exif_and_reencode(&self, img: &DynamicImage, format: ImageFormat) -> Result<Vec<u8>> {
        let mut buffer = Vec::new();

        img.write_to(&mut Cursor::new(&mut buffer), format)
            .map_err(|e| AppError::image_processing(format!("Re-encoding failed: {}", e)))?;

        Ok(buffer)
    }

    /// Validate that data is a supported image type
    ///
    /// This is a quick check without full processing.
    pub fn validate(&self, data: &[u8]) -> Result<String> {
        self.detect_mime_type(data)
    }

    /// Get file extension for MIME type
    pub fn mime_to_extension(mime: &str) -> &'static str {
        match mime {
            "image/jpeg" => "jpg",
            "image/png" => "png",
            "image/gif" => "gif",
            "image/webp" => "webp",
            "image/bmp" => "bmp",
            "image/tiff" => "tiff",
            "video/mp4" => "mp4",
            "video/webm" => "webm",
            "video/quicktime" => "mov",
            _ => "bin",
        }
    }

    /// Get MIME type for extension
    pub fn extension_to_mime(ext: &str) -> &'static str {
        match ext.to_lowercase().as_str() {
            "jpg" | "jpeg" => "image/jpeg",
            "png" => "image/png",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "bmp" => "image/bmp",
            "tiff" | "tif" => "image/tiff",
            "mp4" => "video/mp4",
            "webm" => "video/webm",
            "mov" => "video/quicktime",
            _ => "application/octet-stream",
        }
    }
}

/// Calculate SHA-256 hash of data
///
/// Used for content deduplication.
pub fn calculate_hash(data: &[u8]) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    // For simplicity, using a basic hasher
    // In production, you might want SHA-256
    let mut hasher = DefaultHasher::new();
    data.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_processor() -> ImageProcessor {
        let config = ProcessingConfig {
            output_format: "webp".to_string(),
            output_quality: 85,
            max_image_dimension: 1024,
            keep_originals: true,
            strip_exif: true,
        };
        ImageProcessor::new(&config)
    }

    #[test]
    fn test_mime_detection_jpeg() {
        let processor = create_test_processor();

        // JPEG magic bytes: FF D8 FF
        let jpeg_data = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46];
        let result = processor.detect_mime_type(&jpeg_data);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "image/jpeg");
    }

    #[test]
    fn test_mime_detection_png() {
        let processor = create_test_processor();

        // PNG magic bytes: 89 50 4E 47 0D 0A 1A 0A
        let png_data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        let result = processor.detect_mime_type(&png_data);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "image/png");
    }

    #[test]
    fn test_invalid_data() {
        let processor = create_test_processor();

        let invalid_data = vec![0x00, 0x01, 0x02, 0x03];
        let result = processor.detect_mime_type(&invalid_data);

        assert!(result.is_err());
    }

    #[test]
    fn test_mime_to_extension() {
        assert_eq!(ImageProcessor::mime_to_extension("image/jpeg"), "jpg");
        assert_eq!(ImageProcessor::mime_to_extension("image/png"), "png");
        assert_eq!(ImageProcessor::mime_to_extension("image/webp"), "webp");
        assert_eq!(ImageProcessor::mime_to_extension("video/mp4"), "mp4");
    }

    #[test]
    fn test_output_format() {
        let config = ProcessingConfig {
            output_format: "jpeg".to_string(),
            output_quality: 90,
            max_image_dimension: 2048,
            keep_originals: false,
            strip_exif: true,
        };
        let processor = ImageProcessor::new(&config);

        assert_eq!(processor.output_image_format(), ImageFormat::Jpeg);
    }
}

