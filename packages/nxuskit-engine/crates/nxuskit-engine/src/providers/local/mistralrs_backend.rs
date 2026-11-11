//! mistral.rs inference backend via `mistralrs` crate
//!
//! Feature-gated behind `provider-local-mistralrs`.
//!
//! Provides high-performance local LLM inference with features like:
//! - PagedAttention for efficient KV-cache management
//! - ISQ (In-Situ Quantization) for on-the-fly quantization
//! - Automatic chat template detection from model metadata
//! - Speculative decoding support

use std::collections::HashMap;
use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;

use crate::error::{NxuskitError, Result};
use crate::types::{ChatRequest, ModelInfo, StreamChunk};

use super::backend::{BackendCapabilities, GenerateResponse, InferenceBackend, LoadedModel};

/// mistral.rs inference backend.
///
/// Uses the `mistralrs` crate for high-performance local inference with
/// PagedAttention and other advanced features.
#[derive(Debug)]
pub struct MistralRsBackend {
    // Backend state will be initialized when models are loaded
}

impl MistralRsBackend {
    /// Create a new mistral.rs backend.
    pub fn new() -> Result<Self> {
        Ok(Self {})
    }
}

/// A model loaded through mistral.rs.
#[derive(Debug)]
struct MistralRsLoadedModel {
    model_path: String,
    // The actual mistralrs::Model handle would go here
}

impl LoadedModel for MistralRsLoadedModel {
    fn description(&self) -> String {
        format!("mistralrs:{}", self.model_path)
    }

    fn memory_bytes(&self) -> Option<u64> {
        None // TODO: Extract from mistralrs model stats
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[async_trait]
impl InferenceBackend for MistralRsBackend {
    async fn load_model(
        &self,
        model_path: &str,
        _n_gpu_layers: i32,
        _context_size: Option<u32>,
        _batch_size: Option<u32>,
        _threads: Option<u32>,
    ) -> Result<Box<dyn LoadedModel>> {
        // TODO: Use mistralrs::ModelBuilder to load the model
        // For now, create a stub that validates the path exists
        if !std::path::Path::new(model_path).exists() {
            return Err(NxuskitError::Configuration(format!(
                "Model file not found: '{}'. Provide a valid GGUF model path.",
                model_path
            )));
        }

        Ok(Box::new(MistralRsLoadedModel {
            model_path: model_path.to_string(),
        }))
    }

    async fn generate(
        &self,
        _model: &dyn LoadedModel,
        _request: &ChatRequest,
    ) -> Result<GenerateResponse> {
        // TODO: Implement actual mistralrs generation
        Err(NxuskitError::Configuration(
            "mistral.rs backend generation not yet fully implemented. \
             Use provider-local-llama for production inference."
                .to_string(),
        ))
    }

    async fn generate_stream(
        &self,
        _model: &dyn LoadedModel,
        _request: &ChatRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>> {
        // TODO: Implement actual mistralrs streaming
        Err(NxuskitError::Configuration(
            "mistral.rs backend streaming not yet fully implemented. \
             Use provider-local-llama for production inference."
                .to_string(),
        ))
    }

    async fn list_local_models(&self, search_paths: &[String]) -> Result<Vec<ModelInfo>> {
        // Scan search paths for GGUF files (same as llama-cpp)
        let mut models = Vec::new();
        for dir in search_paths {
            let path = std::path::Path::new(dir);
            if !path.is_dir() {
                continue;
            }
            if let Ok(entries) = std::fs::read_dir(path) {
                for entry in entries.flatten() {
                    let file_path = entry.path();
                    if file_path.extension().is_some_and(|e| e == "gguf") {
                        let name = file_path
                            .file_stem()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_default();
                        let size = std::fs::metadata(&file_path).ok().map(|m| m.len());

                        let mut metadata = HashMap::new();
                        metadata.insert("backend".to_string(), "mistralrs".to_string());
                        metadata
                            .insert("path".to_string(), file_path.to_string_lossy().to_string());

                        models.push(ModelInfo {
                            name,
                            size_bytes: size,
                            description: Some("GGUF model (mistral.rs)".to_string()),
                            context_window: None,
                            metadata,
                        });
                    }
                }
            }
        }
        Ok(models)
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            supports_vision: false,
            supports_seed: true,
            supports_json_mode: true,
            supports_json_schema: false,
            supports_stop_sequences: true,
            max_context_size: None,
        }
    }

    fn backend_name(&self) -> &str {
        "mistralrs"
    }
}

#[cfg(test)]
mod zip_security_tests {
    use mistralrs::{Device, Tensor};

    // Independently encoded ZIP (stored method) and NPY v1 fixtures. Neither
    // fixture construction nor expected values use Candle/ZIP mapping helpers.
    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = u32::MAX;
        for byte in bytes {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
            }
        }
        !crc
    }

    fn put16(bytes: &mut Vec<u8>, value: u16) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn put32(bytes: &mut Vec<u8>, value: u32) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn zip_bytes(name: &str, payload: &[u8], method: u16) -> Vec<u8> {
        let name_len = u16::try_from(name.len()).unwrap();
        let size = u32::try_from(payload.len()).unwrap();
        let checksum = crc32(payload);
        let mut bytes = Vec::new();
        put32(&mut bytes, 0x0403_4b50);
        for value in [20, 0, method, 0, 0x21] {
            put16(&mut bytes, value);
        }
        put32(&mut bytes, checksum);
        put32(&mut bytes, size);
        put32(&mut bytes, size);
        put16(&mut bytes, name_len);
        put16(&mut bytes, 0);
        bytes.extend_from_slice(name.as_bytes());
        bytes.extend_from_slice(payload);
        let central_offset = u32::try_from(bytes.len()).unwrap();
        put32(&mut bytes, 0x0201_4b50);
        for value in [20, 20, 0, method, 0, 0x21] {
            put16(&mut bytes, value);
        }
        put32(&mut bytes, checksum);
        put32(&mut bytes, size);
        put32(&mut bytes, size);
        for value in [name_len, 0, 0, 0, 0] {
            put16(&mut bytes, value);
        }
        put32(&mut bytes, 0);
        put32(&mut bytes, 0);
        bytes.extend_from_slice(name.as_bytes());
        let central_size = u32::try_from(bytes.len()).unwrap() - central_offset;
        put32(&mut bytes, 0x0605_4b50);
        for value in [0, 0, 1, 1] {
            put16(&mut bytes, value);
        }
        put32(&mut bytes, central_size);
        put32(&mut bytes, central_offset);
        put16(&mut bytes, 0);
        bytes
    }

    fn npy_bytes() -> Vec<u8> {
        let mut header = b"{'descr': '<f4', 'fortran_order': False, 'shape': (2,), }".to_vec();
        while (10 + header.len() + 1) % 16 != 0 {
            header.push(b' ');
        }
        header.push(b'\n');
        let mut bytes = b"\x93NUMPY\x01\x00".to_vec();
        put16(&mut bytes, u16::try_from(header.len()).unwrap());
        bytes.extend_from_slice(&header);
        // IEEE-754 little-endian f32 values 1.0 and -2.5, hand-derived.
        bytes.extend_from_slice(&[0, 0, 128, 63, 0, 0, 32, 192]);
        bytes
    }

    fn fixture(bytes: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let inside = directory.path().join("inside");
        std::fs::create_dir(&inside).unwrap();
        let path = inside.join("fixture.npz");
        std::fs::write(&path, bytes).unwrap();
        (directory, path)
    }

    #[test]
    fn literal_stored_npz_has_expected_tensor_values() {
        let (_directory, path) = fixture(&zip_bytes("values.npy", &npy_bytes(), 0));
        let tensors = Tensor::read_npz(&path).unwrap();
        assert_eq!(tensors.len(), 1);
        assert_eq!(tensors[0].0, "values");
        assert_eq!(tensors[0].1.dims(), &[2]);
        assert_eq!(tensors[0].1.to_vec1::<f32>().unwrap(), vec![1.0, -2.5]);
    }

    #[test]
    fn named_npz_selection_is_exact_and_missing_name_errors() {
        let (_directory, path) = fixture(&zip_bytes("values.npy", &npy_bytes(), 0));
        let tensors = Tensor::read_npz_by_name(&path, &["values"]).unwrap();
        assert_eq!(tensors.len(), 1);
        assert_eq!(tensors[0].to_vec1::<f32>().unwrap(), vec![1.0, -2.5]);
        assert!(Tensor::read_npz_by_name(&path, &["missing"]).is_err());
    }

    #[test]
    fn truncated_zip_directory_is_rejected() {
        let mut bytes = zip_bytes("values.npy", &npy_bytes(), 0);
        bytes.truncate(bytes.len() - 7);
        let (_directory, path) = fixture(&bytes);
        assert!(Tensor::read_npz(&path).is_err());
    }

    #[test]
    fn valid_zip_with_invalid_npy_is_rejected() {
        let (_directory, path) = fixture(&zip_bytes("values.npy", b"not a numpy array", 0));
        assert!(Tensor::read_npz(&path).is_err());
    }

    #[test]
    fn unsupported_zip_compression_is_rejected() {
        let (_directory, path) = fixture(&zip_bytes("values.npy", &npy_bytes(), u16::MAX));
        assert!(Tensor::read_npz(&path).is_err());
    }

    #[test]
    fn archive_names_never_extract_files() {
        let (directory, path) = fixture(&zip_bytes("../escape.npy", &npy_bytes(), 0));
        let tensors = Tensor::read_npz(&path).unwrap();
        assert_eq!(tensors[0].0, "../escape");
        assert!(!directory.path().join("escape.npy").exists());
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn writer_emits_complete_stored_zip_and_literal_npy_payload() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("written.npz");
        let tensor = Tensor::from_slice(&[1.0f32, -2.5], (2,), &Device::Cpu).unwrap();
        Tensor::write_npz(&[("values", &tensor)], &path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..4], b"PK\x03\x04");
        assert_eq!(&bytes[8..10], &[0, 0]);
        assert_eq!(&bytes[bytes.len() - 22..bytes.len() - 18], b"PK\x05\x06");
        let name_len = usize::from(u16::from_le_bytes([bytes[26], bytes[27]]));
        let extra_len = usize::from(u16::from_le_bytes([bytes[28], bytes[29]]));
        assert_eq!(&bytes[30..30 + name_len], b"values.npy");
        let payload = 30 + name_len + extra_len;
        assert_eq!(&bytes[payload..payload + 8], b"\x93NUMPY\x01\x00");
        let header_len = usize::from(u16::from_le_bytes([bytes[payload + 8], bytes[payload + 9]]));
        let values = payload + 10 + header_len;
        assert_eq!(&bytes[values..values + 8], &[0, 0, 128, 63, 0, 0, 32, 192]);
        let read = Tensor::read_npz(&path).unwrap();
        assert_eq!(read[0].1.to_vec1::<f32>().unwrap(), vec![1.0, -2.5]);
    }
}
