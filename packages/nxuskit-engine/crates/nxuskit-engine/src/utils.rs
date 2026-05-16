//! Utility functions for nxusKit

use crate::error::{NxuskitError, Result};

/// Maximum image size for Claude API (5 MB)
pub const CLAUDE_MAX_IMAGE_SIZE: u64 = 5 * 1024 * 1024;

/// Maximum image size for OpenAI API (20 MB)
pub const OPENAI_MAX_IMAGE_SIZE: u64 = 20 * 1024 * 1024;

/// Validate image format using magic byte detection
///
/// Supports: JPEG, PNG, GIF, WebP
///
/// # Arguments
///
/// * `data` - Raw image bytes
///
/// # Returns
///
/// Returns the MIME type string if valid, otherwise an error
///
/// # Example
///
/// ```
/// # use nxuskit_engine::utils::validate_image_format;
/// let jpeg_data = vec![0xFF, 0xD8, 0xFF]; // JPEG magic bytes
/// let mime_type = validate_image_format(&jpeg_data).unwrap();
/// assert_eq!(mime_type, "image/jpeg");
/// ```
pub fn validate_image_format(data: &[u8]) -> Result<&'static str> {
    let kind = infer::get(data)
        .ok_or_else(|| NxuskitError::InvalidImageFormat("Unable to determine image type".into()))?;

    match kind.mime_type() {
        "image/jpeg" => Ok("image/jpeg"),
        "image/png" => Ok("image/png"),
        "image/gif" => Ok("image/gif"),
        "image/webp" => Ok("image/webp"),
        other => Err(NxuskitError::InvalidImageFormat(format!(
            "Unsupported format: {}. Supported: JPEG, PNG, GIF, WebP",
            other
        ))),
    }
}

/// Validate image size against provider limits
///
/// # Arguments
///
/// * `size` - Image size in bytes
/// * `provider` - Provider name ("claude", "openai", or other)
///
/// # Returns
///
/// Returns Ok(()) if within limits, otherwise an error
///
/// # Example
///
/// ```
/// # use nxuskit_engine::utils::validate_image_size;
/// // 3 MB image for Claude
/// let result = validate_image_size(3 * 1024 * 1024, "claude");
/// assert!(result.is_ok());
///
/// // 10 MB image for Claude (exceeds 5 MB limit)
/// let result = validate_image_size(10 * 1024 * 1024, "claude");
/// assert!(result.is_err());
/// ```
pub fn validate_image_size(size: u64, provider: &str) -> Result<()> {
    let limit = match provider.to_lowercase().as_str() {
        "claude" => CLAUDE_MAX_IMAGE_SIZE,
        "openai" => OPENAI_MAX_IMAGE_SIZE,
        _ => return Ok(()), // No validation for unknown providers
    };

    if size > limit {
        return Err(NxuskitError::ImageTooLarge {
            size,
            limit,
            provider: provider.to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_image_format_jpeg() {
        // JPEG magic bytes: FF D8 FF
        let jpeg_data = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        let result = validate_image_format(&jpeg_data);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "image/jpeg");
    }

    #[test]
    fn test_validate_image_format_png() {
        // PNG magic bytes: 89 50 4E 47 0D 0A 1A 0A
        let png_data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        let result = validate_image_format(&png_data);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "image/png");
    }

    #[test]
    fn test_validate_image_format_gif() {
        // GIF magic bytes: 47 49 46 38 39 61 (GIF89a)
        let gif_data = vec![0x47, 0x49, 0x46, 0x38, 0x39, 0x61];
        let result = validate_image_format(&gif_data);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "image/gif");
    }

    #[test]
    fn test_validate_image_format_invalid() {
        let invalid_data = vec![0x00, 0x01, 0x02, 0x03];
        let result = validate_image_format(&invalid_data);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_size_claude_ok() {
        let result = validate_image_size(3 * 1024 * 1024, "claude");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_size_claude_exceed() {
        let result = validate_image_size(10 * 1024 * 1024, "claude");
        assert!(result.is_err());
        match result {
            Err(NxuskitError::ImageTooLarge { size, limit, .. }) => {
                assert_eq!(size, 10 * 1024 * 1024);
                assert_eq!(limit, 5 * 1024 * 1024);
            }
            _ => panic!("Expected ImageTooLarge error"),
        }
    }

    #[test]
    fn test_validate_size_openai_ok() {
        let result = validate_image_size(15 * 1024 * 1024, "openai");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_size_openai_exceed() {
        let result = validate_image_size(25 * 1024 * 1024, "openai");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_size_unknown_provider() {
        // Unknown providers have no size limit
        let result = validate_image_size(100 * 1024 * 1024, "unknown");
        assert!(result.is_ok());
    }
}
