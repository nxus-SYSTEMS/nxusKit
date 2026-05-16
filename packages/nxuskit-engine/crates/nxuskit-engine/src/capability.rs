//! Model capability detection trait and types
//!
//! This module provides a provider-agnostic interface for detecting model capabilities
//! such as vision support and streaming. Each LLM provider can implement the
//! [`CapabilityDetector`] trait to expose its detection logic.

use crate::error::Result;
use serde::{Deserialize, Serialize};

/// Vision capability mode - distinguishes single vs multi-image support
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum VisionMode {
    /// Model does not support vision/images
    #[default]
    None,
    /// Model supports single image only
    SingleImage,
    /// Model supports multiple images per request
    MultiImage,
}

impl VisionMode {
    /// Check if this mode supports any vision capability
    pub fn supports_vision(self) -> bool {
        matches!(self, VisionMode::SingleImage | VisionMode::MultiImage)
    }

    /// Check if this mode supports multiple images
    pub fn supports_multiple_images(self) -> bool {
        matches!(self, VisionMode::MultiImage)
    }
}

impl std::fmt::Display for VisionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VisionMode::None => write!(f, "none"),
            VisionMode::SingleImage => write!(f, "single-image"),
            VisionMode::MultiImage => write!(f, "multi-image"),
        }
    }
}

/// Capabilities supported by a model
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelCapabilities {
    /// Vision capability mode (none, single-image, or multi-image)
    #[serde(default)]
    pub vision_mode: VisionMode,

    /// Whether the model supports streaming responses
    pub supports_streaming: bool,

    /// Whether the model supports function/tool calling
    #[serde(default)]
    pub supports_function_calling: bool,
}

// Backward compatibility: expose supports_vision derived from vision_mode
impl ModelCapabilities {
    /// Check if model supports any vision capability
    pub fn supports_vision(&self) -> bool {
        self.vision_mode.supports_vision()
    }

    /// Check if model supports multiple images
    pub fn supports_multiple_images(&self) -> bool {
        self.vision_mode.supports_multiple_images()
    }
}

impl Default for ModelCapabilities {
    fn default() -> Self {
        Self {
            vision_mode: VisionMode::None,
            supports_streaming: true,
            supports_function_calling: false,
        }
    }
}

impl ModelCapabilities {
    /// Create capabilities with defaults
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder-style method to set vision mode
    pub fn with_vision_mode(mut self, vision_mode: VisionMode) -> Self {
        self.vision_mode = vision_mode;
        self
    }

    /// Builder-style method to set streaming support
    pub fn with_streaming(mut self, supports_streaming: bool) -> Self {
        self.supports_streaming = supports_streaming;
        self
    }

    /// Create with single-image vision support
    pub fn with_single_image_vision(mut self) -> Self {
        self.vision_mode = VisionMode::SingleImage;
        self
    }

    /// Create with multi-image vision support
    pub fn with_multi_image_vision(mut self) -> Self {
        self.vision_mode = VisionMode::MultiImage;
        self
    }

    /// Builder-style method to set function calling support
    pub fn with_function_calling(mut self, supports: bool) -> Self {
        self.supports_function_calling = supports;
        self
    }
}

/// Trait for detecting model capabilities
///
/// This trait provides a unified interface for querying model capabilities
/// across different providers. Each provider can implement this trait to expose
/// capability detection based on their specific APIs or heuristics.
///
/// # Example
///
/// ```no_run
/// use nxuskit_engine::capability::CapabilityDetector;
/// use nxuskit_engine::providers::OllamaProvider;
///
/// # async fn example() -> std::result::Result<(), Box<dyn std::error::Error>> {
/// let provider = OllamaProvider::builder().build()?;
/// let capabilities = provider.get_model_capabilities("llava:latest").await?;
///
/// if capabilities.supports_vision() {
///     println!("Model supports vision!");
/// }
/// # Ok(())
/// # }
/// ```
#[async_trait::async_trait]
pub trait CapabilityDetector: Send + Sync {
    /// Get capabilities for a specific model
    ///
    /// Returns model capabilities including vision support, streaming, and other features.
    /// If capability detection fails (e.g., model not found), implementations should
    /// return sensible defaults rather than erroring when possible.
    ///
    /// # Arguments
    ///
    /// * `model_name` - The name/identifier of the model
    ///
    /// # Returns
    ///
    /// A [`ModelCapabilities`] struct describing the model's features,
    /// or an error if the provider API is unreachable.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The provider API is unreachable
    /// - Authentication fails (for providers requiring credentials)
    /// - A network error occurs
    async fn get_model_capabilities(&self, model_name: &str) -> Result<ModelCapabilities>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vision_mode_supports_vision() {
        assert!(!VisionMode::None.supports_vision());
        assert!(VisionMode::SingleImage.supports_vision());
        assert!(VisionMode::MultiImage.supports_vision());
    }

    #[test]
    fn test_vision_mode_supports_multiple_images() {
        assert!(!VisionMode::None.supports_multiple_images());
        assert!(!VisionMode::SingleImage.supports_multiple_images());
        assert!(VisionMode::MultiImage.supports_multiple_images());
    }

    #[test]
    fn test_vision_mode_display() {
        assert_eq!(VisionMode::None.to_string(), "none");
        assert_eq!(VisionMode::SingleImage.to_string(), "single-image");
        assert_eq!(VisionMode::MultiImage.to_string(), "multi-image");
    }

    #[test]
    fn test_model_capabilities_default() {
        let caps = ModelCapabilities::default();
        assert_eq!(caps.vision_mode, VisionMode::None);
        assert!(!caps.supports_vision());
        assert!(caps.supports_streaming);
        assert!(!caps.supports_function_calling);
    }

    #[test]
    fn test_model_capabilities_builder() {
        let caps = ModelCapabilities::new()
            .with_multi_image_vision()
            .with_streaming(false);

        assert_eq!(caps.vision_mode, VisionMode::MultiImage);
        assert!(caps.supports_vision());
        assert!(caps.supports_multiple_images());
        assert!(!caps.supports_streaming);
    }

    #[test]
    fn test_model_capabilities_single_image() {
        let caps = ModelCapabilities::new().with_single_image_vision();

        assert_eq!(caps.vision_mode, VisionMode::SingleImage);
        assert!(caps.supports_vision());
        assert!(!caps.supports_multiple_images());
    }

    #[test]
    fn test_model_capabilities_function_calling() {
        let caps = ModelCapabilities::new().with_function_calling(true);
        assert!(caps.supports_function_calling);

        let caps = ModelCapabilities::new().with_function_calling(false);
        assert!(!caps.supports_function_calling);
    }

    #[test]
    fn test_model_capabilities_serde_backward_compat() {
        // JSON without supports_function_calling should deserialize with default (false)
        let json = r#"{"vision_mode":"none","supports_streaming":true}"#;
        let caps: ModelCapabilities = serde_json::from_str(json).unwrap();
        assert!(!caps.supports_function_calling);

        // JSON with supports_function_calling should round-trip
        let caps = ModelCapabilities::new().with_function_calling(true);
        let json = serde_json::to_string(&caps).unwrap();
        let caps2: ModelCapabilities = serde_json::from_str(&json).unwrap();
        assert!(caps2.supports_function_calling);
    }
}
