//! Model discovery and store management
//!
//! Scans local directories and optionally the Ollama model store for GGUF files.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::types::ModelInfo;

/// Discover GGUF model files from configured search paths.
///
/// Scans each directory for `.gguf` files and returns `ModelInfo` entries.
/// Discovery priority: explicit search_paths → nxusKit model dir → Ollama store.
pub fn discover_models(search_paths: &[String], include_ollama: bool) -> Vec<ModelInfo> {
    let mut models = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();

    // Scan explicit search paths
    for dir in search_paths {
        scan_directory(dir, "local", &mut models, &mut seen_paths);
    }

    // Optionally scan Ollama model store
    if include_ollama && let Some(ollama_dir) = ollama_models_dir() {
        scan_ollama_store(&ollama_dir, &mut models, &mut seen_paths);
    }

    models.sort_by(|a, b| a.name.cmp(&b.name));
    models
}

/// Scan a directory for GGUF model files.
fn scan_directory(
    dir: &str,
    source: &str,
    models: &mut Vec<ModelInfo>,
    seen: &mut std::collections::HashSet<PathBuf>,
) {
    let path = Path::new(dir);
    if !path.is_dir() {
        return;
    }

    let entries = match std::fs::read_dir(path) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let file_path = entry.path();
        if file_path.extension().is_some_and(|e| e == "gguf") {
            if !seen.insert(file_path.clone()) {
                continue; // Already seen
            }

            let name = file_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let size = std::fs::metadata(&file_path).ok().map(|m| m.len());

            let mut metadata = HashMap::new();
            metadata.insert("source".to_string(), source.to_string());
            metadata.insert("format".to_string(), "gguf".to_string());
            metadata.insert(
                "file_path".to_string(),
                file_path.to_string_lossy().to_string(),
            );

            // Try to extract quantization from filename
            if let Some(quant) = extract_quantization(&name) {
                metadata.insert("quantization".to_string(), quant);
            }

            models.push(ModelInfo {
                name,
                size_bytes: size,
                description: Some("GGUF model (local)".to_string()),
                context_window: None,
                metadata,
            });
        }
    }
}

/// Extract quantization level from model filename.
///
/// Common patterns: `Q4_K_M`, `Q5_K_S`, `Q8_0`, `F16`, `F32`.
fn extract_quantization(name: &str) -> Option<String> {
    let parts: Vec<&str> = name.split('.').collect();
    for part in &parts {
        let upper = part.to_uppercase();
        if upper.starts_with("Q4")
            || upper.starts_with("Q5")
            || upper.starts_with("Q6")
            || upper.starts_with("Q8")
            || upper == "F16"
            || upper == "F32"
        {
            return Some(upper);
        }
    }
    // Also check hyphen-separated
    for part in name.split('-') {
        let upper = part.to_uppercase();
        if upper.starts_with("Q4")
            || upper.starts_with("Q5")
            || upper.starts_with("Q6")
            || upper.starts_with("Q8")
            || upper == "F16"
            || upper == "F32"
        {
            return Some(upper);
        }
    }
    None
}

/// Get the Ollama models directory for the current platform.
fn ollama_models_dir() -> Option<PathBuf> {
    // Check env var first
    if let Ok(dir) = std::env::var("OLLAMA_MODELS") {
        return Some(PathBuf::from(dir));
    }

    // Platform defaults
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return Some(PathBuf::from(home).join(".ollama").join("models"));
        }
    }

    #[cfg(target_os = "linux")]
    {
        let path = PathBuf::from("/usr/share/ollama/.ollama/models");
        if path.is_dir() {
            return Some(path);
        }
        // Fall back to user home
        if let Some(home) = std::env::var_os("HOME") {
            return Some(PathBuf::from(home).join(".ollama").join("models"));
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(profile) = std::env::var_os("USERPROFILE") {
            return Some(PathBuf::from(profile).join(".ollama").join("models"));
        }
    }

    None
}

/// Scan Ollama model store for GGUF files.
///
/// Ollama stores models as blobs referenced by manifest files.
/// Structure: models/manifests/registry.ollama.ai/<namespace>/<model>/latest
/// Each manifest references blob SHA digests containing GGUF data.
fn scan_ollama_store(
    base_dir: &Path,
    models: &mut Vec<ModelInfo>,
    seen: &mut std::collections::HashSet<PathBuf>,
) {
    let manifests_dir = base_dir.join("manifests").join("registry.ollama.ai");
    if !manifests_dir.is_dir() {
        return;
    }

    // Walk namespace/model/tag structure
    let namespaces = match std::fs::read_dir(&manifests_dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for ns_entry in namespaces.flatten() {
        if !ns_entry.path().is_dir() {
            continue;
        }
        let ns_name = ns_entry.file_name().to_string_lossy().to_string();

        let model_dirs = match std::fs::read_dir(ns_entry.path()) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for model_entry in model_dirs.flatten() {
            if !model_entry.path().is_dir() {
                continue;
            }
            let model_name = model_entry.file_name().to_string_lossy().to_string();

            // Look for manifest files (tags)
            let tags = match std::fs::read_dir(model_entry.path()) {
                Ok(e) => e,
                Err(_) => continue,
            };

            for tag_entry in tags.flatten() {
                let tag_path = tag_entry.path();
                let tag = tag_entry.file_name().to_string_lossy().to_string();

                // Read manifest to find the model blob
                if let Ok(manifest_content) = std::fs::read_to_string(&tag_path)
                    && let Ok(manifest) =
                        serde_json::from_str::<serde_json::Value>(&manifest_content)
                {
                    // Find the model layer (mediaType contains "model")
                    if let Some(layers) = manifest.get("layers").and_then(|l| l.as_array()) {
                        for layer in layers {
                            let media_type = layer
                                .get("mediaType")
                                .and_then(|m| m.as_str())
                                .unwrap_or("");
                            if media_type.contains("model")
                                && let Some(digest) = layer.get("digest").and_then(|d| d.as_str())
                            {
                                let blob_path =
                                    base_dir.join("blobs").join(digest.replace(':', "-"));
                                if blob_path.exists() && seen.insert(blob_path.clone()) {
                                    let display_name = if ns_name == "library" {
                                        format!("{}:{}", model_name, tag)
                                    } else {
                                        format!("{}/{}:{}", ns_name, model_name, tag)
                                    };

                                    let size = std::fs::metadata(&blob_path).ok().map(|m| m.len());

                                    let mut metadata = HashMap::new();
                                    metadata.insert("source".to_string(), "ollama".to_string());
                                    metadata.insert("format".to_string(), "gguf".to_string());
                                    metadata.insert(
                                        "file_path".to_string(),
                                        blob_path.to_string_lossy().to_string(),
                                    );

                                    models.push(ModelInfo {
                                        name: display_name,
                                        size_bytes: size,
                                        description: Some("Ollama model (GGUF)".to_string()),
                                        context_window: None,
                                        metadata,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
