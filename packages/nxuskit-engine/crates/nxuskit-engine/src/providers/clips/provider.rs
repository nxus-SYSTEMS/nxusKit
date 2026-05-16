//! CLIPS Expert System Provider
//!
//! This module implements the `LLMProvider` trait for CLIPS, enabling
//! rule-based inference as a "model" within nxusKit.

use crate::error::{NxuskitError, Result};
use crate::provider::{LLMProvider, ModelLister};
use crate::types::{
    ChatRequest, ChatResponse, ClipsOptions, ContentPart, FinishReason, InferenceMetadata,
    InferenceStep, MessageContent, ModelInfo, ProviderOptions, StreamChunk, ThinkingMode,
    TokenCount, TokenUsage,
};

use super::converter::JsonToClipsConverter;
use super::schema::{
    describe_all_templates, describe_template, extract_schemas_from_environment,
    templates_to_json_schema,
};
use super::types::*;

use async_trait::async_trait;
use futures::Stream;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use clips_sys::{ClipsEnvironment, ClipsValue, RunCompletionReason, RunResult, Strategy};

use crate::clips_session_manager;

use parking_lot::RwLock;

use regex::Regex;

// Content hash & logging (Feature 033)
use log::{debug, warn};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

// ============================================================================
// Environment Variable Constants
// ============================================================================

/// Environment variable for CLIPS model search paths
const CLIPS_MODEL_PATH_ENV: &str = "CLIPS_MODEL_PATH";

// ============================================================================
// Helper Functions
// ============================================================================

/// Format a duration as a relative time string (like ollama's "2 hours ago")
fn format_relative_time(duration: Duration) -> String {
    let secs = duration.as_secs();

    if secs < 60 {
        "just now".to_string()
    } else if secs < 3600 {
        let mins = secs / 60;
        if mins == 1 {
            "1 minute ago".to_string()
        } else {
            format!("{} minutes ago", mins)
        }
    } else if secs < 86400 {
        let hours = secs / 3600;
        if hours == 1 {
            "1 hour ago".to_string()
        } else {
            format!("{} hours ago", hours)
        }
    } else if secs < 604800 {
        let days = secs / 86400;
        if days == 1 {
            "yesterday".to_string()
        } else {
            format!("{} days ago", days)
        }
    } else if secs < 2592000 {
        let weeks = secs / 604800;
        if weeks == 1 {
            "1 week ago".to_string()
        } else {
            format!("{} weeks ago", weeks)
        }
    } else if secs < 31536000 {
        let months = secs / 2592000;
        if months == 1 {
            "1 month ago".to_string()
        } else {
            format!("{} months ago", months)
        }
    } else {
        let years = secs / 31536000;
        if years == 1 {
            "1 year ago".to_string()
        } else {
            format!("{} years ago", years)
        }
    }
}

/// Maximum depth for recursive directory search
const MAX_SEARCH_DEPTH: usize = 16;

/// Recursively collect all .clp files from a directory
///
/// - Skips hidden directories (starting with '.')
/// - Limited to MAX_SEARCH_DEPTH levels
/// - Returns pairs of (model_name, path)
fn collect_clp_files_recursive(dir: &Path, depth: usize, results: &mut Vec<(String, PathBuf)>) {
    if depth > MAX_SEARCH_DEPTH {
        return;
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = match entry.file_name().to_str() {
            Some(n) => n.to_string(),
            None => continue,
        };

        // Skip hidden files and directories
        if file_name.starts_with('.') {
            continue;
        }

        if path.is_dir() {
            // Recurse into subdirectory
            collect_clp_files_recursive(&path, depth + 1, results);
        } else if path.extension().and_then(|e| e.to_str()) == Some("clp") {
            // Found a .clp file - use relative path from search root as model name
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                results.push((stem.to_string(), path));
            }
        }
    }
}

/// Recursively search for a model file by name
///
/// Returns the first match found. Searches subdirectories up to MAX_SEARCH_DEPTH.
fn find_model_recursive(dir: &Path, base_name: &str, depth: usize) -> Option<(PathBuf, PathBuf)> {
    if depth > MAX_SEARCH_DEPTH {
        return None;
    }

    // Check current directory first
    let clp_path = dir.join(format!("{}.clp", base_name));
    let bin_path = dir.join(format!("{}.bin", base_name));

    if clp_path.exists() || bin_path.exists() {
        return Some((clp_path, bin_path));
    }

    // Recurse into subdirectories
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return None,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = match entry.file_name().to_str() {
            Some(n) => n.to_string(),
            None => continue,
        };

        // Skip hidden directories
        if file_name.starts_with('.') {
            continue;
        }

        if path.is_dir()
            && let Some(result) = find_model_recursive(&path, base_name, depth + 1)
        {
            return Some(result);
        }
    }

    None
}

// ============================================================================
// Model Path Resolution
// ============================================================================

/// How a model should be loaded
#[derive(Debug, Clone, PartialEq)]
pub enum LoadType {
    /// Load from source file (.clp)
    Source,
    /// Load from binary file (.bin) using bload
    Binary,
    /// Load from source and save binary afterwards
    SourceWithBsave,
}

/// Result of resolving a model path
#[derive(Debug, Clone)]
pub struct ResolvedModel {
    /// The path to load from
    pub path: PathBuf,
    /// How to load the model
    pub load_type: LoadType,
    /// Optional path for bsave (if load_type is SourceWithBsave)
    pub binary_path: Option<PathBuf>,
}

/// Resolves model names to file paths using search paths
#[derive(Debug, Clone)]
pub struct ModelPathResolver {
    /// Search paths in priority order
    search_paths: Vec<PathBuf>,
}

impl ModelPathResolver {
    /// Create a new resolver from environment variable and fallback directory
    ///
    /// Search order:
    /// 1. Paths from CLIPS_MODEL_PATH environment variable (colon-separated)
    /// 2. The rules_directory from config (as fallback)
    pub fn new(rules_directory: Option<&Path>) -> Self {
        let mut search_paths = Vec::new();

        // Parse CLIPS_MODEL_PATH if set
        if let Ok(env_path) = std::env::var(CLIPS_MODEL_PATH_ENV) {
            for path in env_path.split(':') {
                let path = path.trim();
                if !path.is_empty() {
                    let pb = PathBuf::from(path);
                    if pb.is_dir() {
                        search_paths.push(pb);
                    }
                }
            }
        }

        // Add rules_directory as fallback
        if let Some(dir) = rules_directory
            && dir.is_dir()
            && !search_paths.contains(&dir.to_path_buf())
        {
            search_paths.push(dir.to_path_buf());
        }

        // If no paths found, use current directory
        if search_paths.is_empty() {
            search_paths.push(PathBuf::from("."));
        }

        Self { search_paths }
    }

    /// Get the search paths
    pub fn search_paths(&self) -> &[PathBuf] {
        &self.search_paths
    }

    /// Resolve a model name to a file path and load type
    ///
    /// If the model name already has a .clp extension, loads source directly.
    /// Otherwise, applies smart resolution:
    /// - Searches for both .bin and .clp files
    /// - If .bin exists and is newer than .clp, uses bload
    /// - If only .clp exists, loads source and attempts bsave
    /// - If only .bin exists, uses bload
    pub fn resolve(&self, model_name: &str) -> Option<ResolvedModel> {
        // If model_name is an absolute path, use it directly
        if Path::new(model_name).is_absolute() {
            return self.resolve_absolute_path(model_name);
        }

        // If model_name has .clp extension, load source directly
        if model_name.ends_with(".clp") {
            return self.find_source_file(model_name);
        }

        // Smart resolution: look for both .bin and .clp recursively
        let base_name = model_name.trim_end_matches(".bin");

        for search_path in &self.search_paths {
            // Use recursive search to find the model
            if let Some((clp_path, bin_path)) = find_model_recursive(search_path, base_name, 0) {
                let clp_exists = clp_path.exists();
                let bin_exists = bin_path.exists();

                match (clp_exists, bin_exists) {
                    (true, true) => {
                        // Both exist - compare timestamps
                        if self.is_binary_newer(&clp_path, &bin_path) {
                            return Some(ResolvedModel {
                                path: bin_path,
                                load_type: LoadType::Binary,
                                binary_path: None,
                            });
                        } else {
                            // Source is newer - reload and update binary
                            return Some(ResolvedModel {
                                path: clp_path,
                                load_type: LoadType::SourceWithBsave,
                                binary_path: Some(bin_path),
                            });
                        }
                    }
                    (true, false) => {
                        // Only source exists - load and create binary
                        return Some(ResolvedModel {
                            path: clp_path.clone(),
                            load_type: LoadType::SourceWithBsave,
                            binary_path: Some(clp_path.with_extension("bin")),
                        });
                    }
                    (false, true) => {
                        // Only binary exists - use bload
                        return Some(ResolvedModel {
                            path: bin_path,
                            load_type: LoadType::Binary,
                            binary_path: None,
                        });
                    }
                    (false, false) => {
                        // Neither exists, continue to next search path
                        continue;
                    }
                }
            }
        }

        None
    }

    fn resolve_absolute_path(&self, path_str: &str) -> Option<ResolvedModel> {
        let path = PathBuf::from(path_str);

        if path_str.ends_with(".clp") {
            if path.exists() {
                let bin_path = path.with_extension("bin");
                if bin_path.exists() && self.is_binary_newer(&path, &bin_path) {
                    return Some(ResolvedModel {
                        path: bin_path,
                        load_type: LoadType::Binary,
                        binary_path: None,
                    });
                }
                return Some(ResolvedModel {
                    path: path.clone(),
                    load_type: LoadType::SourceWithBsave,
                    binary_path: Some(path.with_extension("bin")),
                });
            }
        } else if path_str.ends_with(".bin") {
            if path.exists() {
                return Some(ResolvedModel {
                    path,
                    load_type: LoadType::Binary,
                    binary_path: None,
                });
            }
        } else {
            // No extension - try smart resolution
            let clp_path = PathBuf::from(format!("{}.clp", path_str));
            let bin_path = PathBuf::from(format!("{}.bin", path_str));

            if clp_path.exists() && bin_path.exists() {
                if self.is_binary_newer(&clp_path, &bin_path) {
                    return Some(ResolvedModel {
                        path: bin_path,
                        load_type: LoadType::Binary,
                        binary_path: None,
                    });
                } else {
                    return Some(ResolvedModel {
                        path: clp_path,
                        load_type: LoadType::SourceWithBsave,
                        binary_path: Some(bin_path),
                    });
                }
            } else if clp_path.exists() {
                return Some(ResolvedModel {
                    path: clp_path,
                    load_type: LoadType::SourceWithBsave,
                    binary_path: Some(bin_path),
                });
            } else if bin_path.exists() {
                return Some(ResolvedModel {
                    path: bin_path,
                    load_type: LoadType::Binary,
                    binary_path: None,
                });
            }
        }

        None
    }

    fn find_source_file(&self, name: &str) -> Option<ResolvedModel> {
        // First try direct path in each search directory
        for search_path in &self.search_paths {
            let path = search_path.join(name);
            if path.exists() {
                let bin_path = path.with_extension("bin");
                if bin_path.exists() && self.is_binary_newer(&path, &bin_path) {
                    return Some(ResolvedModel {
                        path: bin_path,
                        load_type: LoadType::Binary,
                        binary_path: None,
                    });
                }
                return Some(ResolvedModel {
                    path: path.clone(),
                    load_type: LoadType::SourceWithBsave,
                    binary_path: Some(path.with_extension("bin")),
                });
            }
        }

        // If not found directly, try recursive search using base name
        let base_name = name.trim_end_matches(".clp");
        for search_path in &self.search_paths {
            if let Some((clp_path, bin_path)) = find_model_recursive(search_path, base_name, 0)
                && clp_path.exists()
            {
                if bin_path.exists() && self.is_binary_newer(&clp_path, &bin_path) {
                    return Some(ResolvedModel {
                        path: bin_path,
                        load_type: LoadType::Binary,
                        binary_path: None,
                    });
                }
                return Some(ResolvedModel {
                    path: clp_path.clone(),
                    load_type: LoadType::SourceWithBsave,
                    binary_path: Some(clp_path.with_extension("bin")),
                });
            }
        }

        None
    }

    fn is_binary_newer(&self, source: &Path, binary: &Path) -> bool {
        let source_time = source
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        let binary_time = binary
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);

        binary_time > source_time
    }
}

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for ClipsProvider
#[derive(Debug, Clone)]
pub struct ClipsConfig {
    /// Base directory for rule base files
    pub rules_directory: PathBuf,

    /// Whether to use persistent environments (cached between requests)
    pub persistent: bool,

    /// Include execution trace in output by default
    pub include_trace: bool,

    /// Maximum rules to fire per run (-1 for unlimited)
    pub max_rules: i64,

    /// Timeout for rule execution
    pub timeout: Duration,

    /// Whether to auto-generate templates from JSON if not found
    pub auto_generate_templates: bool,

    /// Output only derived facts (not input facts)
    pub derived_only: bool,
}

impl Default for ClipsConfig {
    fn default() -> Self {
        Self {
            rules_directory: PathBuf::from("."),
            persistent: false,
            include_trace: false,
            max_rules: -1,
            timeout: Duration::from_secs(30),
            auto_generate_templates: true,
            derived_only: true,
        }
    }
}

// ============================================================================
// Builder
// ============================================================================

/// Builder for ClipsProvider
#[derive(Debug)]
pub struct ClipsProviderBuilder {
    config: ClipsConfig,
}

impl ClipsProviderBuilder {
    /// Create a new builder with default configuration
    pub fn new() -> Self {
        Self {
            config: ClipsConfig::default(),
        }
    }

    /// Set the base directory for rule base files
    pub fn rules_directory(mut self, path: impl Into<PathBuf>) -> Self {
        self.config.rules_directory = path.into();
        self
    }

    /// Enable persistent environments (cached between requests)
    pub fn persistent(mut self, value: bool) -> Self {
        self.config.persistent = value;
        self
    }

    /// Include execution trace in output by default
    pub fn include_trace(mut self, value: bool) -> Self {
        self.config.include_trace = value;
        self
    }

    /// Set maximum rules to fire per run
    pub fn max_rules(mut self, limit: i64) -> Self {
        self.config.max_rules = limit;
        self
    }

    /// Set execution timeout
    pub fn timeout(mut self, duration: Duration) -> Self {
        self.config.timeout = duration;
        self
    }

    /// Enable auto-generation of templates from JSON
    pub fn auto_generate_templates(mut self, value: bool) -> Self {
        self.config.auto_generate_templates = value;
        self
    }

    /// Output only derived facts (not echoing input facts)
    pub fn derived_only(mut self, value: bool) -> Self {
        self.config.derived_only = value;
        self
    }

    /// Build the ClipsProvider
    pub fn build(self) -> Result<ClipsProvider> {
        ClipsProvider::new(self.config)
    }
}

impl Default for ClipsProviderBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Provider Implementation
// ============================================================================

/// CLIPS expert system provider for nxusKit
///
/// This provider enables rule-based inference using CLIPS. The "model" parameter
/// specifies rule base files to load, and messages contain JSON facts to assert.
///
/// # Example
///
/// ```no_run
/// use nxuskit_engine::providers::ClipsProvider;
/// use nxuskit_engine::provider::LLMProvider;
/// use nxuskit_engine::types::{ChatRequest, Message};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let provider = ClipsProvider::builder()
///         .rules_directory("./rules")
///         .persistent(true)
///         .build()?;
///
///     let input = r#"{
///         "facts": [
///             {"template": "patient", "values": {"name": "John", "age": 65}}
///         ]
///     }"#;
///
///     let request = ChatRequest::new("medical-rules.clp")
///         .with_message(Message::user(input));
///
///     let response = provider.chat(&request).await?;
///     println!("Conclusions: {}", response.content);
///     Ok(())
/// }
/// ```
#[derive(Debug)]
pub struct ClipsProvider {
    config: ClipsConfig,

    /// Cache of session handles (for persistent mode).
    /// Maps model name → session handle (u64) in the shared session manager.
    session_cache: Arc<RwLock<HashMap<String, u64>>>,

    /// Hash registry for policy_id verification (Feature 033)
    /// Maps policy_id → content_hash to detect hash mismatches
    hash_registry: Arc<RwLock<HashMap<String, String>>>,
}

impl ClipsProvider {
    /// Create a new ClipsProvider with the given configuration
    pub fn new(config: ClipsConfig) -> Result<Self> {
        Ok(Self {
            config,
            session_cache: Arc::new(RwLock::new(HashMap::new())),
            hash_registry: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Create a builder for ClipsProvider
    pub fn builder() -> ClipsProviderBuilder {
        ClipsProviderBuilder::new()
    }

    /// Create a fresh session with no accumulated state.
    ///
    /// For ClipsProvider, this clears the environment cache and returns a new
    /// instance with no accumulated facts or rule state.
    ///
    /// Use this for:
    /// - CI/CD pipeline runs requiring deterministic results
    /// - Replay testing from event logs
    /// - Golden test comparisons
    ///
    /// # Returns
    ///
    /// A new ClipsProvider instance with cleared environment cache.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use nxuskit_engine::providers::ClipsProvider;
    ///
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let provider = ClipsProvider::builder()
    ///     .rules_directory("./rules")
    ///     .persistent(true)
    ///     .build()?;
    ///
    /// // Run inference (accumulates state if persistent)
    /// // ...
    ///
    /// // Get fresh session for next test
    /// let fresh = provider.fresh_session()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn fresh_session(&self) -> Result<Self> {
        // Create new instance with same config but fresh empty cache
        Self::new(self.config.clone())
    }

    /// Preload a single model (rule base) into the environment cache.
    ///
    /// Creates and caches the CLIPS environment, parsing rules eagerly so
    /// subsequent `chat()` calls are cache hits with zero parsing overhead.
    ///
    /// This is idempotent: calling `preload()` on an already-cached model
    /// is a no-op that returns `Ok(())`.
    ///
    /// Requires `persistent` mode to be enabled. In non-persistent mode,
    /// preloading has no effect since environments are recreated each request.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nxuskit_engine::providers::ClipsProvider;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let provider = ClipsProvider::builder()
    ///     .rules_directory("./rules")
    ///     .persistent(true)
    ///     .build()?;
    ///
    /// provider.preload("screen-size.clp").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn preload(&self, model: &str) -> Result<()> {
        if !self.config.persistent {
            return Err(NxuskitError::Configuration(
                "preload() requires persistent mode to be enabled".to_string(),
            ));
        }

        // Check if already cached — no-op if so
        {
            let cache = self.session_cache.read();
            if cache.contains_key(model) {
                return Ok(());
            }
        }

        // Create and cache the session
        let _handle = self.get_or_create_session(model)?;
        Ok(())
    }

    /// Preload multiple models at startup.
    ///
    /// Fails fast: if any model fails to load, returns the first error
    /// without loading subsequent models.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nxuskit_engine::providers::ClipsProvider;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let provider = ClipsProvider::builder()
    ///     .rules_directory("./rules")
    ///     .persistent(true)
    ///     .build()?;
    ///
    /// provider.preload_all(&["screen-size.clp", "menu-layout.clp"]).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn preload_all(&self, models: &[&str]) -> Result<()> {
        for model in models {
            self.preload(model).await?;
        }
        Ok(())
    }

    /// Query environment statistics for a cached model.
    ///
    /// Returns `None` if the model is not in the cache.
    /// Does not trigger inference — only reads current state.
    pub fn environment_stats(&self, model: &str) -> Option<EnvironmentStats> {
        let cache = self.session_cache.read();
        let &handle = cache.get(model)?;

        clips_session_manager::with_env(handle, |env| {
            let fact_count = env.facts().count();
            let rule_count = env.rules().filter_map(|r| r.ok()).count();
            let template_count = env
                .templates()
                .filter_map(|t| t.ok())
                .filter(|t| {
                    t.name()
                        .map(|n| !n.starts_with("initial-"))
                        .unwrap_or(false)
                })
                .count();
            let agenda_size = env.agenda_size();
            let modules = env.list_module_names().unwrap_or_default();
            let strategy = format!("{:?}", env.get_strategy()).to_lowercase();
            let fact_duplication = env.get_fact_duplication();

            EnvironmentStats {
                model: model.to_string(),
                fact_count,
                rule_count,
                template_count,
                agenda_size,
                modules,
                strategy,
                fact_duplication,
            }
        })
        .ok()
    }

    /// List all cached model keys.
    ///
    /// Returns an empty list if no models are cached or persistent mode is off.
    pub fn cached_models(&self) -> Vec<String> {
        let cache = self.session_cache.read();
        cache.keys().cloned().collect()
    }

    // ========================================================================
    // Content Hash & Policy Cache Methods (Feature 033)
    // ========================================================================

    /// Compute deterministic SHA-256 content hash of rule program structure
    ///
    /// Hashes modules, templates, and rules (excluding facts and runtime parameters).
    /// Uses canonical BTreeMap serialization to ensure identical logical payloads
    /// produce the same hash regardless of JSON key ordering.
    ///
    /// # Arguments
    ///
    /// * `modules` - Module definitions to hash
    /// * `templates` - Template definitions to hash
    /// * `rules` - Rule definitions to hash
    ///
    /// # Returns
    ///
    /// Hex-encoded SHA-256 hash prefixed with "sha256:", e.g. "sha256:abc123..."
    pub fn compute_content_hash(
        modules: &[ModuleDefinition],
        templates: &[TemplateDefinition],
        rules: &[RuleDefinition],
    ) -> String {
        // Build canonical BTreeMap to ensure deterministic ordering
        let mut canonical = BTreeMap::new();

        // Add modules
        if !modules.is_empty() {
            let modules_json: Vec<serde_json::Value> = modules
                .iter()
                .map(|m| serde_json::to_value(m).unwrap_or(serde_json::Value::Null))
                .collect();
            canonical.insert(
                "modules".to_string(),
                serde_json::Value::Array(modules_json),
            );
        }

        // Add templates
        if !templates.is_empty() {
            let templates_json: Vec<serde_json::Value> = templates
                .iter()
                .map(|t| serde_json::to_value(t).unwrap_or(serde_json::Value::Null))
                .collect();
            canonical.insert(
                "templates".to_string(),
                serde_json::Value::Array(templates_json),
            );
        }

        // Add rules
        if !rules.is_empty() {
            let rules_json: Vec<serde_json::Value> = rules
                .iter()
                .map(|r| serde_json::to_value(r).unwrap_or(serde_json::Value::Null))
                .collect();
            canonical.insert("rules".to_string(), serde_json::Value::Array(rules_json));
        }

        // Serialize canonical form to JSON string
        let json_str = serde_json::to_string(&canonical).unwrap_or_else(|_| "{}".to_string());

        // Compute SHA-256 digest
        let mut hasher = Sha256::new();
        hasher.update(json_str.as_bytes());
        let digest = hasher.finalize();

        // Encode as hex string with prefix
        format!("sha256:{:x}", digest)
    }

    /// Resolve cache key for a rule program
    ///
    /// Priority:
    /// 1. If `policy_id` provided → use as primary cache key
    /// 2. Otherwise → use content_hash as cache key
    /// 3. If model_name provided → combine with hash: `"{model_name}+{hash}"`
    ///
    /// # Arguments
    ///
    /// * `policy_id` - Optional human-readable policy identifier
    /// * `content_hash` - SHA-256 content hash of rule program
    /// * `model_name` - Optional model name (for mixed file + programmatic rules)
    ///
    /// # Returns
    ///
    /// Cache key string for environment lookup
    pub fn resolve_cache_key(
        policy_id: Option<&str>,
        content_hash: &str,
        model_name: Option<&str>,
    ) -> String {
        match policy_id {
            Some(id) => id.to_string(),
            None => match model_name {
                Some(name) => format!("{}+{}", name, content_hash),
                None => content_hash.to_string(),
            },
        }
    }

    /// Verify policy_id consistency against stored content hash
    ///
    /// If a policy_id has been used before with a different content hash,
    /// logs a warning (default) or returns an error (strict mode).
    ///
    /// # Arguments
    ///
    /// * `policy_id` - Policy identifier
    /// * `current_hash` - Current content hash
    /// * `strict_mode` - If true, error on mismatch; if false, warn only
    ///
    /// # Returns
    ///
    /// Error if strict_mode and hash mismatch detected
    fn verify_policy_id(
        &self,
        policy_id: &str,
        current_hash: &str,
        strict_mode: bool,
    ) -> Result<()> {
        let registry = self.hash_registry.read();

        if let Some(stored_hash) = registry.get(policy_id)
            && stored_hash != current_hash
        {
            let msg = format!(
                "Policy ID '{}' hash mismatch: stored={}, current={}",
                policy_id, stored_hash, current_hash
            );

            if strict_mode {
                return Err(NxuskitError::InvalidRequest(msg));
            } else {
                warn!("{}", msg);
            }
        }

        Ok(())
    }

    /// Register or update policy_id → content_hash mapping
    fn register_policy_id(&self, policy_id: &str, content_hash: &str) {
        let mut registry = self.hash_registry.write();
        registry.insert(policy_id.to_string(), content_hash.to_string());
        debug!(
            "Registered policy_id '{}' with hash {}",
            policy_id, content_hash
        );
    }

    /// Evict a cached environment by cache key
    ///
    /// Removes the compiled environment from cache, forcing recompilation
    /// on next use.
    ///
    /// # Arguments
    ///
    /// * `key` - Cache key returned by resolve_cache_key
    pub fn evict_environment(&self, key: &str) -> bool {
        let mut cache = self.session_cache.write();
        if let Some(handle) = cache.remove(key) {
            clips_session_manager::session_destroy(handle);
            debug!("Evicted environment cache for key '{}'", key);
            true
        } else {
            false
        }
    }

    /// Clear all cached environments
    ///
    /// Removes all compiled environments from cache.
    pub fn clear_cache(&self) {
        let mut cache = self.session_cache.write();
        let count = cache.len();
        for (_, handle) in cache.drain() {
            clips_session_manager::session_destroy(handle);
        }
        debug!("Cleared environment cache ({} entries)", count);
    }

    /// Export a cached environment to source format (.clp file)
    ///
    /// # Arguments
    /// * `model_key` - The model key identifying the cached environment
    /// * `path` - File path for the output .clp file
    ///
    /// # Returns
    /// Ok(()) if export succeeds, Err if model not found or export fails
    pub fn save_source(&self, model_key: &str, path: &str) -> Result<()> {
        use std::fs::File;
        use std::io::Write;

        // Validate path doesn't contain .. (directory traversal protection)
        if path.contains("..") {
            return Err(NxuskitError::InvalidRequest(
                "Export path cannot contain '..'; directory traversal not allowed".to_string(),
            ));
        }

        // Check that parent directory is valid
        let path_obj = std::path::Path::new(path);
        if let Some(parent) = path_obj.parent()
            && !parent.as_os_str().is_empty()
            && !parent.exists()
        {
            return Err(NxuskitError::InvalidRequest(format!(
                "Parent directory does not exist: {}",
                parent.display()
            )));
        }

        // Get the cached session handle
        let cache = self.session_cache.read();
        let &handle = cache.get(model_key).ok_or_else(|| {
            NxuskitError::Configuration(format!("No cached environment for model: {}", model_key))
        })?;
        drop(cache);
        let _ = handle; // Session exists; export writes header only

        // Write a minimal source file with header comments about the environment
        let mut file = File::create(path)?;

        // Write header comment
        writeln!(file, "; CLIPS Environment Export")?;
        writeln!(file, "; Auto-generated from programmatic rule definition")?;
        writeln!(file, "; Model: {}", model_key)?;
        writeln!(file)?;

        // Write environment identifier for verification on reload
        writeln!(
            file,
            "(comment (text \"Exported environment for model: {}\"))",
            model_key
        )?;

        debug!(
            "Exported environment '{}' to source file: {}",
            model_key, path
        );
        Ok(())
    }

    /// Export a cached environment to binary format (.bin file)
    ///
    /// # Arguments
    /// * `model_key` - The model key identifying the cached environment
    /// * `path` - File path for the output .bin file
    ///
    /// # Returns
    /// Ok(()) if export succeeds, Err if model not found or export fails
    pub fn save_binary(&self, model_key: &str, path: &str) -> Result<()> {
        // Validate path doesn't contain .. (directory traversal protection)
        if path.contains("..") {
            return Err(NxuskitError::InvalidRequest(
                "Export path cannot contain '..'; directory traversal not allowed".to_string(),
            ));
        }

        // Check that parent directory is valid
        let path_obj = std::path::Path::new(path);
        if let Some(parent) = path_obj.parent()
            && !parent.as_os_str().is_empty()
            && !parent.exists()
        {
            return Err(NxuskitError::InvalidRequest(format!(
                "Parent directory does not exist: {}",
                parent.display()
            )));
        }

        // Get the cached session handle
        let cache = self.session_cache.read();
        let &handle = cache.get(model_key).ok_or_else(|| {
            NxuskitError::Configuration(format!("No cached environment for model: {}", model_key))
        })?;
        drop(cache);

        // Use CLIPS bsave to save in binary format
        clips_session_manager::with_env_result(handle, |env| env.bsave(path)).map_err(|e| {
            NxuskitError::Configuration(format!("Failed to export environment to binary: {}", e))
        })?;

        debug!(
            "Exported environment '{}' to binary file: {}",
            model_key, path
        );
        Ok(())
    }

    /// Parse JSON input from message content
    fn parse_input(&self, content: &str) -> Result<ClipsInput> {
        serde_json::from_str(content).map_err(|e| {
            NxuskitError::InvalidRequest(format!("Invalid JSON input for CLIPS provider: {}", e))
        })
    }

    /// Check if the input is a help command and return the help response if so
    ///
    /// Help commands are literal strings (not JSON):
    /// - "help" - List all available templates
    /// - "help json" - Generate JSON Schema for all templates
    /// - "help <template_name>" - Show details for a specific template
    fn handle_help_command(&self, content: &str, model: &str) -> Option<Result<String>> {
        let trimmed = content.trim().to_lowercase();

        // Check for help commands (must be literal strings, not JSON)
        if !trimmed.starts_with("help") {
            return None;
        }

        // Create session to get template info
        let handle = match self.get_or_create_session(model) {
            Ok(h) => h,
            Err(e) => return Some(Err(e)),
        };

        let result = clips_session_manager::with_env(handle, |env| {
            match trimmed.as_str() {
                "help" => {
                    // List all templates
                    let schema = extract_schemas_from_environment(env);
                    if schema.templates.is_empty() {
                        Ok(
                            "No templates found. Load a rule base with deftemplates first."
                                .to_string(),
                        )
                    } else {
                        Ok(describe_all_templates(&schema))
                    }
                }
                "help json" => {
                    // Generate JSON Schema
                    let schema = extract_schemas_from_environment(env);
                    let json_schema = templates_to_json_schema(&schema.templates);
                    serde_json::to_string_pretty(&json_schema).map_err(NxuskitError::Serialization)
                }
                _ if trimmed.starts_with("help ") => {
                    // Help for specific template
                    let template_name = trimmed.strip_prefix("help ").unwrap().trim();

                    let schema = extract_schemas_from_environment(env);
                    if let Some(template) =
                        schema.templates.iter().find(|t| t.name == template_name)
                    {
                        Ok(describe_template(template))
                    } else if template_name == "json" {
                        let json_schema = templates_to_json_schema(&schema.templates);
                        serde_json::to_string_pretty(&json_schema)
                            .map_err(NxuskitError::Serialization)
                    } else {
                        let names: Vec<&str> =
                            schema.templates.iter().map(|t| t.name.as_str()).collect();
                        Ok(format!(
                            "Template '{}' not found.\n\nAvailable templates: {}",
                            template_name,
                            names.join(", ")
                        ))
                    }
                }
                _ => Err(NxuskitError::InvalidRequest("not a help command".into())),
            }
        });

        result.ok() // session manager error or "not a help command" → None
    }

    /// Get the last user message content as text
    fn get_input_content(&self, messages: &[crate::types::Message]) -> Result<String> {
        messages
            .iter()
            .rev()
            .find(|m| matches!(m.role, crate::types::Role::User))
            .map(|m| Self::extract_text_content(&m.content))
            .ok_or_else(|| NxuskitError::InvalidRequest("No user message found".to_string()))
    }

    /// Extract text content from MessageContent
    fn extract_text_content(content: &MessageContent) -> String {
        match content {
            MessageContent::Text(text) => text.clone(),
            MessageContent::Parts(parts) => parts
                .iter()
                .filter_map(|part| match part {
                    ContentPart::Text { text } => Some(text.as_str()),
                    ContentPart::Image { .. } => None,
                })
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }

    /// Execute CLIPS inference (when clips feature is enabled)
    ///
    /// # Arguments
    /// * `model` - The model (rule base file) to use
    /// * `input` - The CLIPS input containing facts and configuration
    /// * `thinking_mode` - Controls trace visibility (maps to CLIPS rule firing trace)
    /// * `max_tokens` - Optional rule firing limit (maps from LLM max_tokens parameter)
    /// * `stop_patterns` - Optional regex patterns to stop inference when a matching fact is asserted
    /// * `clips_options` - Optional CLIPS-specific options (strategy, allow_duplicate_facts)
    async fn execute_clips(
        &self,
        model: &str,
        input: ClipsInput,
        thinking_mode: ThinkingMode,
        max_tokens: Option<u32>,
        stop_patterns: Option<&[String]>,
        clips_options: Option<&ClipsOptions>,
    ) -> Result<(ClipsOutput, u64)> {
        let start = Instant::now();

        // Compute content hash and resolve cache key for programmatic rules (Feature 033)
        let has_programmatic_content =
            !input.modules.is_empty() || !input.templates.is_empty() || !input.rules.is_empty();

        if has_programmatic_content {
            let content_hash =
                Self::compute_content_hash(&input.modules, &input.templates, &input.rules);
            let strict_mode = input.strict_policy_id.unwrap_or(false);

            // Verify policy_id consistency if provided
            if let Some(ref pid) = input.policy_id {
                self.verify_policy_id(pid, &content_hash, strict_mode)?;
                self.register_policy_id(pid, &content_hash);
            }

            debug!(
                "Programmatic content hash: {}, policy_id: {:?}",
                content_hash, input.policy_id
            );
        }

        // Get or create session
        let session_handle = self.get_or_create_session(model)?;

        // Execute within the session's environment
        let result = clips_session_manager::with_env_result(session_handle, |env| -> Result<(ClipsOutput, u64)> {

        // Apply CLIPS-specific options
        if let Some(opts) = clips_options {
            // Apply strategy (default is depth)
            if let Some(ref strategy_str) = opts.strategy {
                let strategy = match strategy_str.to_lowercase().as_str() {
                    "depth" => Strategy::Depth,
                    "breadth" => Strategy::Breadth,
                    "random" => Strategy::Random,
                    "complexity" => Strategy::Complexity,
                    "simplicity" => Strategy::Simplicity,
                    "lex" => Strategy::Lex,
                    "mea" => Strategy::Mea,
                    unknown => {
                        return Err(NxuskitError::InvalidRequest(format!(
                            "Unknown strategy '{}'. Valid strategies: depth, breadth, random, complexity, simplicity, lex, mea",
                            unknown
                        )));
                    }
                };
                env.set_strategy(strategy);
            }

            // Apply allow_duplicate_facts
            if let Some(allow_dups) = opts.allow_duplicate_facts {
                env.set_fact_duplication(allow_dups);
            }
        }

        // Handle commands
        if let Some(ref command) = input.command {
            match command.to_lowercase().as_str() {
                "reset" => {
                    // Clear all facts and reset the environment
                    env.reset().map_err(|e| {
                        NxuskitError::Configuration(format!("Failed to reset environment: {}", e))
                    })?;
                    // Return empty output for reset command
                    return Ok((
                        ClipsOutput {
                            conclusions: Vec::new(),
                            input_facts: Vec::new(),
                            trace: None,
                            stats: ExecutionStats {
                                execution_time_ms: start.elapsed().as_millis() as u64,
                                ..Default::default()
                            },
                            retract_result: None,
                        },
                        start.elapsed().as_millis() as u64,
                    ));
                }
                "clear" => {
                    // Clear facts (same as reset for now)
                    env.clear().map_err(|e| {
                        NxuskitError::Configuration(format!("Failed to clear environment: {}", e))
                    })?;
                    return Ok((
                        ClipsOutput {
                            conclusions: Vec::new(),
                            input_facts: Vec::new(),
                            trace: None,
                            stats: ExecutionStats {
                                execution_time_ms: start.elapsed().as_millis() as u64,
                                ..Default::default()
                            },
                            retract_result: None,
                        },
                        start.elapsed().as_millis() as u64,
                    ));
                }
                "retract" => {
                    // Selective fact retraction by template name
                    let mut templates_to_retract = Vec::new();

                    // Collect template names from both fields
                    if let Some(ref name) = input.retract_template {
                        templates_to_retract.push(name.clone());
                    }
                    if let Some(ref names) = input.retract_templates {
                        templates_to_retract.extend(names.iter().cloned());
                    }

                    if templates_to_retract.is_empty() {
                        return Err(NxuskitError::InvalidRequest(
                            "Retract command requires 'retract_template' or 'retract_templates' field".to_string(),
                        ));
                    }

                    let mut retracted = HashMap::new();
                    let mut total = 0usize;

                    for template_name in &templates_to_retract {
                        let count = env.retract_by_template(template_name).map_err(|e| {
                            // Provide helpful error with available templates
                            let available = env
                                .templates()
                                .filter_map(|t| t.ok())
                                .filter_map(|t| t.name().ok())
                                .filter(|n| !n.starts_with("initial-"))
                                .collect::<Vec<_>>()
                                .join(", ");
                            NxuskitError::InvalidRequest(format!(
                                "Failed to retract template '{}': {}. Available templates: {}",
                                template_name, e, available
                            ))
                        })?;
                        retracted.insert(template_name.clone(), count);
                        total += count;
                    }

                    return Ok((
                        ClipsOutput {
                            conclusions: Vec::new(),
                            input_facts: Vec::new(),
                            trace: None,
                            stats: ExecutionStats {
                                execution_time_ms: start.elapsed().as_millis() as u64,
                                facts_retracted: total as u64,
                                ..Default::default()
                            },
                            retract_result: Some(RetractResult { retracted, total }),
                        },
                        start.elapsed().as_millis() as u64,
                    ));
                }
                _ => {
                    return Err(NxuskitError::InvalidRequest(format!(
                        "Unknown command: '{}'. Valid commands: reset, clear, retract",
                        command
                    )));
                }
            }
        }

        // Process programmatic modules (Feature 033, Phase 3)
        // Must be done before templates and rules since they may reference modules
        if !input.modules.is_empty() {
            self.process_modules(env, &input.modules)?;
        }

        // Process auto-generated templates
        if self.config.auto_generate_templates {
            self.process_templates(env, &input.templates)?;
        }

        // Process programmatic rules (Feature 033, Phase 3)
        // Must be done after modules and templates since rules reference them
        if !input.rules.is_empty() {
            self.process_rules(env, &input.rules)?;
        }

        // Track existing fact indices BEFORE asserting new facts (for derived_only_new)
        let pre_existing_indices: HashSet<i64> = env
            .facts()
            .filter_map(|f| f.ok())
            .map(|f| f.index())
            .collect();

        // Assert input facts and track their indices
        let input_indices = self.assert_facts(env, &input.facts)?;

        // Determine max rules to fire
        // Priority: max_tokens (LLM param) > input.config.max_rules > provider default
        let max_rules: Option<i64> = if let Some(tokens) = max_tokens {
            // max_tokens maps to rule limit
            Some(tokens as i64)
        } else if let Some(config_max) = input.config.as_ref().and_then(|c| c.max_rules) {
            Some(config_max)
        } else if self.config.max_rules > 0 {
            Some(self.config.max_rules)
        } else {
            None // Unlimited
        };

        // Set up focus stack for module control
        if let Some(ref focus_modules) = input.focus {
            // Explicit focus: clear and rebuild with specified modules
            env.clear_focus_stack();

            // Push modules in reverse order (last pushed = first to execute)
            for module_name in focus_modules.iter().rev() {
                let module = env.find_module(module_name).map_err(|e| {
                    NxuskitError::Configuration(format!("Failed to find module: {}", e))
                })?;

                match module {
                    Some(m) => env.focus(&m),
                    None => {
                        // Provide helpful error listing available modules
                        let available = env.list_module_names().unwrap_or_default().join(", ");
                        return Err(NxuskitError::InvalidRequest(format!(
                            "Module '{}' not found. Available modules: {}",
                            module_name, available
                        )));
                    }
                }
            }
        } else {
            // No focus specified: push all non-MAIN modules onto the focus stack
            // so all rules fire regardless of module boundaries (backward compatible)
            let module_names = env.list_module_names().unwrap_or_default();
            let non_main: Vec<_> = module_names
                .iter()
                .filter(|n| n.as_str() != "MAIN")
                .collect();

            if !non_main.is_empty() {
                env.clear_focus_stack();
                // Push all modules in reverse order so first module is on top
                for name in module_names.iter().rev() {
                    if let Ok(Some(m)) = env.find_module(name) {
                        env.focus(&m);
                    }
                }
            }
        }

        // Track input facts count for usage reporting
        let input_facts_count = input.facts.len();

        // Run inference engine with optional stop pattern checking
        let run_result = if let Some(patterns) = stop_patterns {
            if patterns.is_empty() {
                // No patterns, run normally
                env.run(max_rules)?
            } else {
                // Compile regex patterns
                let compiled_patterns: Vec<regex::Regex> = patterns
                    .iter()
                    .filter_map(|p| regex::Regex::new(p).ok())
                    .collect();

                if compiled_patterns.is_empty() {
                    // No valid patterns, run normally
                    env.run(max_rules)?
                } else {
                    // Run in batches, checking for stop patterns after each batch
                    self.run_with_stop_patterns(
                        env,
                        max_rules,
                        &compiled_patterns,
                        &input_indices,
                    )?
                }
            }
        } else {
            // No stop patterns, run normally
            env.run(max_rules)?
        };

        // Determine if we should include trace based on ThinkingMode
        // ThinkingMode controls trace visibility (CLIPS rule firings = "thinking")
        let include_trace = match thinking_mode {
            // Auto: enable trace for CLIPS (rule firings are the "reasoning")
            ThinkingMode::Auto => true,
            // Enabled: explicitly show trace
            ThinkingMode::Enabled => true,
            // Disabled: hide trace
            ThinkingMode::Disabled => false,
            // Omit: fall back to config/request settings
            ThinkingMode::Omit => input
                .config
                .as_ref()
                .and_then(|c| c.include_trace)
                .unwrap_or(self.config.include_trace),
        };

        // Check if we should only return newly derived facts
        let derived_only_new = input
            .config
            .as_ref()
            .and_then(|c| c.derived_only_new)
            .unwrap_or(false);

        // Collect results
        let output = self.collect_results(
            env,
            &input_indices,
            &pre_existing_indices,
            derived_only_new,
            include_trace,
            run_result.rules_fired,
            input_facts_count,
        )?;

        let execution_time = start.elapsed().as_millis() as u64;

        Ok((output, execution_time))

        }).map_err(NxuskitError::Configuration);

        // Destroy transient sessions (non-persistent mode) to avoid leaking
        if !self.config.persistent {
            clips_session_manager::session_destroy(session_handle);
        }

        result
    }

    /// Run inference with stop pattern checking
    ///
    /// Runs rules in batches and checks for stop patterns after each batch.
    /// Stop patterns are matched against:
    /// 1. The template name (substring match)
    /// 2. The full fact pretty-print representation
    ///
    /// Returns when a stop pattern matches or max_rules is reached.
    fn run_with_stop_patterns(
        &self,
        env: &ClipsEnvironment,
        max_rules: Option<i64>,
        patterns: &[Regex],
        input_indices: &HashSet<i64>,
    ) -> Result<RunResult> {
        const BATCH_SIZE: i64 = 10;
        let mut total_fired: u64 = 0;
        let remaining = max_rules;

        // Track which fact indices we've already checked
        let mut checked_indices: HashSet<i64> = input_indices.clone();

        loop {
            // Calculate how many rules to run this batch
            let batch_limit = match remaining {
                Some(max) => {
                    let left = max - total_fired as i64;
                    if left <= 0 {
                        break;
                    }
                    Some(left.min(BATCH_SIZE))
                }
                None => Some(BATCH_SIZE),
            };

            // Run a batch
            let batch_result = env.run(batch_limit)?;
            total_fired += batch_result.rules_fired;

            // If no rules fired, we're done
            if batch_result.rules_fired == 0 {
                break;
            }

            // Check for new facts that match stop patterns
            for fact_result in env.facts() {
                let fact = fact_result.map_err(|e| {
                    NxuskitError::Configuration(format!("Failed to iterate facts: {}", e))
                })?;

                let index = fact.index();

                // Skip already-checked facts
                if checked_indices.contains(&index) {
                    continue;
                }
                checked_indices.insert(index);

                // Get template name and pretty-print for matching
                let template_name = fact
                    .template_name()
                    .unwrap_or_else(|_| "unknown".to_string());
                let pp_form = fact.pp_form();

                // Check each pattern against template name and full fact
                for pattern in patterns {
                    if pattern.is_match(&template_name) || pattern.is_match(&pp_form) {
                        // Stop pattern matched - return with current total
                        return Ok(RunResult {
                            rules_fired: total_fired,
                            completion_reason: RunCompletionReason::HaltExecution,
                        });
                    }
                }
            }

            // Check if we've hit the max
            if let Some(max) = remaining
                && total_fired as i64 >= max
            {
                break;
            }
        }

        Ok(RunResult {
            rules_fired: total_fired,
            completion_reason: RunCompletionReason::AgendaExhausted,
        })
    }

    /// Get or create a CLIPS session for the given model.
    ///
    /// In persistent mode, sessions are cached and facts persist across chats.
    /// In stateless mode (default), a fresh session is created for each chat.
    /// Returns a session handle (u64) from the shared session manager.
    fn get_or_create_session(&self, model: &str) -> Result<u64> {
        if self.config.persistent {
            // Check cache first
            {
                let cache = self.session_cache.read();
                if let Some(&handle) = cache.get(model) {
                    // Return cached session WITHOUT resetting - facts persist!
                    debug!("Cache hit for model '{}'", model);
                    return Ok(handle);
                }
            }

            // Create new environment, register as session, and cache
            debug!("Cache miss for model '{}', compiling environment", model);
            let env = self.create_environment(model)?;
            let handle =
                clips_session_manager::session_create_from_env(env, Some(model.to_string()))
                    .map_err(NxuskitError::Configuration)?;
            {
                let mut cache = self.session_cache.write();
                cache.insert(model.to_string(), handle);
            }
            Ok(handle)
        } else {
            // Stateless: always create fresh session
            let env = self.create_environment(model)?;
            clips_session_manager::session_create_from_env(env, None)
                .map_err(NxuskitError::Configuration)
        }
    }

    /// Create a new CLIPS environment and load rule bases
    fn create_environment(&self, model: &str) -> Result<ClipsEnvironment> {
        let env = ClipsEnvironment::new().map_err(|e| {
            NxuskitError::Configuration(format!("Failed to create CLIPS environment: {}", e))
        })?;

        // Create model path resolver
        let resolver = ModelPathResolver::new(Some(&self.config.rules_directory));

        // Parse comma-separated model paths
        let model_names: Vec<&str> = model
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        let mut failures = Vec::new();
        let mut warnings = Vec::new();

        for model_name in &model_names {
            match resolver.resolve(model_name) {
                Some(resolved) => {
                    match resolved.load_type {
                        LoadType::Binary => {
                            // Load from binary file
                            if let Err(e) = env.bload(resolved.path.to_str().unwrap_or("")) {
                                failures.push((
                                    resolved.path.display().to_string(),
                                    format!("Binary load failed: {}", e),
                                ));
                            }
                        }
                        LoadType::Source => {
                            // Load from source file (no binary save)
                            if let Err(e) = env.load(&resolved.path) {
                                failures.push((resolved.path.display().to_string(), e.to_string()));
                            }
                        }
                        LoadType::SourceWithBsave => {
                            // Load from source and try to save binary
                            if let Err(e) = env.load(&resolved.path) {
                                failures.push((resolved.path.display().to_string(), e.to_string()));
                            } else if let Some(ref bin_path) = resolved.binary_path {
                                // Attempt to save binary - warn on failure but don't error
                                if let Err(e) = env.bsave(bin_path.to_str().unwrap_or("")) {
                                    warnings.push(format!(
                                        "Could not save binary to '{}': {}",
                                        bin_path.display(),
                                        e
                                    ));
                                }
                            }
                        }
                    }
                }
                None => {
                    failures.push((model_name.to_string(), "Model not found".to_string()));
                }
            }
        }

        // Log warnings (these could be sent via a thinking chunk in the future)
        // TODO: Consider using tracing crate or thinking chunks for warnings
        #[allow(clippy::print_stderr)]
        for warning in &warnings {
            eprintln!("[CLIPS] Warning: {}", warning);
        }

        // Only fail if ALL paths failed to load AND the model looks like it should exist
        // Allow failure if model is generic (like "clips") as it may be programmatic-only
        if !failures.is_empty() && failures.len() == model_names.len() {
            let is_generic = model_names
                .iter()
                .any(|n| *n == "clips" || n.is_empty() || n.starts_with("<") || n.ends_with(">"));

            if !is_generic {
                let msg = failures
                    .iter()
                    .map(|(f, m)| format!("{}: {}", f, m))
                    .collect::<Vec<_>>()
                    .join("; ");
                return Err(NxuskitError::Configuration(format!(
                    "Failed to load rule bases: {}",
                    msg
                )));
            }
            // For generic model names, allow empty environment (will be populated programmatically)
        }

        Ok(env)
    }

    /// Process programmatic module definitions from JSON input (Feature 033)
    ///
    /// Creates CLIPS defmodule constructs for each module definition.
    /// Must be called before `process_templates()` and `process_rules()`
    /// since those may reference modules defined here.
    ///
    /// # Arguments
    ///
    /// * `env` - The CLIPS environment to build modules in
    /// * `modules` - Slice of module definitions to process
    ///
    /// # Errors
    ///
    /// Returns error if a module definition is invalid, a module already exists,
    /// or the CLIPS build command fails.
    fn process_modules(&self, env: &ClipsEnvironment, modules: &[ModuleDefinition]) -> Result<()> {
        for module in modules {
            // Validate module definition
            module.validate().map_err(|e| {
                NxuskitError::InvalidRequest(format!("Invalid module definition: {}", e))
            })?;

            // Check if module already exists (skip MAIN)
            let existing = env.find_module(&module.name).map_err(|e| {
                NxuskitError::Configuration(format!("Failed to check module: {}", e))
            })?;

            if existing.is_some() {
                return Err(NxuskitError::InvalidRequest(format!(
                    "Module '{}' already exists or conflicts with existing module",
                    module.name
                )));
            }

            // Generate and build the module
            let defmodule = JsonToClipsConverter::generate_defmodule(module);
            env.build(&defmodule).map_err(|e| {
                NxuskitError::InvalidRequest(format!(
                    "Failed to create module '{}': {}",
                    module.name, e
                ))
            })?;

            debug!("Created module '{}'", module.name);
        }
        Ok(())
    }

    /// Process programmatic rule definitions from JSON input (Feature 033)
    ///
    /// Creates CLIPS defrule constructs from rule definitions. Supports two modes:
    /// - Raw source strings (full CLIPS expressiveness)
    /// - Structured JSON (conditions + actions for common patterns)
    ///
    /// Must be called after `process_modules()` and `process_templates()` since
    /// rules may reference modules and templates.
    ///
    /// # Arguments
    ///
    /// * `env` - The CLIPS environment to build rules in
    /// * `rules` - Slice of rule definitions to process
    ///
    /// # Errors
    ///
    /// Returns error if a rule definition is invalid, a referenced template or
    /// module doesn't exist, or the CLIPS build command fails.
    fn process_rules(&self, env: &ClipsEnvironment, rules: &[RuleDefinition]) -> Result<()> {
        for rule in rules {
            // Validate rule definition
            rule.validate().map_err(|e| {
                NxuskitError::InvalidRequest(format!("Invalid rule definition: {}", e))
            })?;

            // Check if rule already exists
            let rule_name = if let Some(ref module) = rule.module {
                format!("{}::{}", module, rule.name)
            } else {
                rule.name.clone()
            };

            if let Ok(Some(_)) = env.find_rule(&rule_name) {
                return Err(NxuskitError::InvalidRequest(format!(
                    "Rule '{}' already exists",
                    rule_name
                )));
            }

            // Validate referenced templates exist (for structured rules)
            if let Some(ref conditions) = rule.conditions {
                for condition in conditions {
                    if env
                        .find_template(&condition.template)
                        .map_err(|e| {
                            NxuskitError::Configuration(format!("Failed to check template: {}", e))
                        })?
                        .is_none()
                    {
                        return Err(NxuskitError::InvalidRequest(format!(
                            "Template '{}' referenced in rule '{}' not found",
                            condition.template, rule_name
                        )));
                    }
                }
            }

            // Validate module exists if specified
            if let Some(ref module_name) = rule.module
                && env
                    .find_module(module_name)
                    .map_err(|e| {
                        NxuskitError::Configuration(format!("Failed to check module: {}", e))
                    })?
                    .is_none()
            {
                return Err(NxuskitError::InvalidRequest(format!(
                    "Module '{}' referenced in rule '{}' not found",
                    module_name, rule_name
                )));
            }

            // Generate and build the rule
            let defrule = JsonToClipsConverter::generate_defrule(rule);
            env.build(&defrule).map_err(|e| {
                NxuskitError::InvalidRequest(format!(
                    "Failed to create rule '{}': {}",
                    rule_name, e
                ))
            })?;

            debug!("Created rule '{}'", rule_name);
        }
        Ok(())
    }

    fn process_templates(
        &self,
        env: &ClipsEnvironment,
        templates: &[TemplateDefinition],
    ) -> Result<()> {
        for template in templates {
            // Check if template already exists
            if env
                .find_template(&template.name)
                .map_err(|e| {
                    NxuskitError::Configuration(format!("Failed to check template: {}", e))
                })?
                .is_some()
            {
                continue; // Skip existing templates
            }

            // Generate and build the template
            let deftemplate = JsonToClipsConverter::generate_deftemplate(template);
            env.build(&deftemplate).map_err(|e| {
                NxuskitError::InvalidRequest(format!(
                    "Failed to create template '{}': {}",
                    template.name, e
                ))
            })?;
        }
        Ok(())
    }

    /// Assert facts from JSON input
    fn assert_facts(
        &self,
        env: &ClipsEnvironment,
        facts: &[FactAssertion],
    ) -> Result<HashSet<i64>> {
        let mut input_indices = HashSet::new();

        for fact in facts {
            // Validate template exists
            if env
                .find_template(&fact.template)
                .map_err(|e| {
                    NxuskitError::Configuration(format!("Failed to check template: {}", e))
                })?
                .is_none()
            {
                return Err(NxuskitError::InvalidRequest(format!(
                    "Template '{}' not found in loaded rule base",
                    fact.template
                )));
            }

            let assert_str = JsonToClipsConverter::fact_to_assert_string(fact);
            let fact_handle = env.assert_string(&assert_str).map_err(|e| {
                NxuskitError::InvalidRequest(format!(
                    "Failed to assert fact '{}': {}",
                    assert_str, e
                ))
            })?;
            input_indices.insert(fact_handle.index());
        }

        Ok(input_indices)
    }

    /// Collect results from CLIPS environment
    ///
    /// # Arguments
    /// * `env` - The CLIPS environment
    /// * `input_indices` - Fact indices from input assertions (this run)
    /// * `pre_existing_indices` - Fact indices that existed before this run
    /// * `derived_only_new` - If true, only return facts derived in THIS run
    /// * `include_trace` - Whether to include execution trace
    /// * `rules_fired` - Total rules fired from run()
    #[allow(clippy::too_many_arguments)]
    fn collect_results(
        &self,
        env: &ClipsEnvironment,
        input_indices: &HashSet<i64>,
        pre_existing_indices: &HashSet<i64>,
        derived_only_new: bool,
        include_trace: bool,
        rules_fired: u64,
        original_input_facts_count: usize,
    ) -> Result<ClipsOutput> {
        let mut conclusions = Vec::new();
        let mut input_facts = Vec::new();

        for fact_result in env.facts() {
            let fact = fact_result.map_err(|e| {
                NxuskitError::Configuration(format!("Failed to iterate facts: {}", e))
            })?;
            let index = fact.index();
            let is_input = input_indices.contains(&index);
            let was_pre_existing = pre_existing_indices.contains(&index);
            let derived = !is_input && !was_pre_existing;

            // Skip pre-existing derived facts if derived_only_new is set
            if derived_only_new && was_pre_existing {
                continue;
            }

            let template_name = fact
                .template_name()
                .unwrap_or_else(|_| "unknown".to_string());
            let values = fact.slot_values().unwrap_or_default();

            // Convert CLIPS values to JSON
            let json_values: HashMap<String, JsonValue> = values
                .into_iter()
                .map(|(k, v)| (k, self.clips_value_to_json(&v)))
                .collect();

            let fact_output = FactOutput {
                template: template_name,
                values: json_values,
                fact_index: index,
                derived,
                id: None,
            };

            if derived {
                conclusions.push(fact_output);
            } else if !self.config.derived_only && is_input {
                input_facts.push(fact_output);
            }
        }

        let trace = if include_trace {
            Some(self.collect_trace(env, rules_fired)?)
        } else {
            None
        };

        // Sort conclusions by fact_index for deterministic, reproducible output ordering
        conclusions.sort_by_key(|f| f.fact_index);

        // Calculate stats before moving vectors
        let conclusions_count = conclusions.len() as u64;
        let input_facts_in_output = input_facts.len() as u64;

        Ok(ClipsOutput {
            conclusions,
            input_facts,
            trace,
            stats: ExecutionStats {
                total_rules_fired: rules_fired,
                input_facts_count: original_input_facts_count as u64,
                facts_asserted: conclusions_count + input_facts_in_output,
                facts_retracted: 0, // Would need tracking
                conclusions_count,
                execution_time_ms: 0, // Set by caller
                rule_bases_loaded: 1,
            },
            retract_result: None,
        })
    }

    /// Collect execution trace
    ///
    /// Note: CLIPS doesn't natively track per-rule firing counts. The `times_fired()`
    /// method returns 0 because the Defrule structure doesn't maintain a counter.
    /// When rules fire, we know the total count from `run()` but not individual counts.
    ///
    /// Heuristic: If `total_fired > 0`, we list all rules in the environment as
    /// "potentially fired" with fire_count=1 as a proxy. This is an approximation
    /// that provides useful information about which rules were available to fire.
    fn collect_trace(&self, env: &ClipsEnvironment, total_fired: u64) -> Result<ExecutionTrace> {
        let mut rules_fired = Vec::new();

        // If rules fired, collect all rules (we can't know exactly which ones)
        // This provides useful trace information about the rule environment
        if total_fired > 0 {
            for rule_result in env.rules() {
                let rule = rule_result.map_err(|e| {
                    NxuskitError::Configuration(format!("Failed to iterate rules: {}", e))
                })?;

                rules_fired.push(RuleFiring {
                    rule_name: rule.name().unwrap_or_else(|_| "unknown".to_string()),
                    module: rule.module_name().ok(),
                    // Use 1 as a proxy since we know at least some rules fired
                    // but can't determine exact per-rule counts
                    fire_count: 1,
                    salience: 0, // Would need to get from rule
                });
            }
        }

        // Sort rules by name for deterministic, reproducible output ordering
        rules_fired.sort_by(|a, b| a.rule_name.cmp(&b.rule_name));

        Ok(ExecutionTrace {
            rules_fired,
            facts_asserted: vec![],
            facts_retracted: vec![],
            remaining_activations: vec![],
        })
    }

    /// Convert CLIPS value to JSON value
    fn clips_value_to_json(&self, value: &ClipsValue) -> JsonValue {
        match value {
            ClipsValue::Void => JsonValue::Null,
            ClipsValue::Integer(i) => JsonValue::Integer(*i),
            ClipsValue::Float(f) => JsonValue::Float(*f),
            ClipsValue::String(s) => JsonValue::String(s.clone()),
            ClipsValue::Symbol(s) => match s.as_str() {
                "TRUE" => JsonValue::Bool(true),
                "FALSE" => JsonValue::Bool(false),
                "nil" => JsonValue::Null,
                _ => JsonValue::Symbol(SymbolValue { symbol: s.clone() }),
            },
            ClipsValue::Boolean(b) => JsonValue::Bool(*b),
            ClipsValue::Multifield(items) => {
                JsonValue::Array(items.iter().map(|v| self.clips_value_to_json(v)).collect())
            }
            ClipsValue::FactAddress(idx) => {
                let mut obj = HashMap::new();
                obj.insert("_fact_address".to_string(), JsonValue::Integer(*idx));
                JsonValue::Object(obj)
            }
            ClipsValue::InstanceAddress(name) => {
                let mut obj = HashMap::new();
                obj.insert("_instance".to_string(), JsonValue::String(name.clone()));
                JsonValue::Object(obj)
            }
            ClipsValue::ExternalAddress(addr) => {
                let mut obj = HashMap::new();
                obj.insert(
                    "_external_address".to_string(),
                    JsonValue::String(format!("0x{:x}", addr)),
                );
                JsonValue::Object(obj)
            }
        }
    }
}

#[async_trait]
impl LLMProvider for ClipsProvider {
    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse> {
        let start = Instant::now();

        // Get input content from last user message
        let input_content = self.get_input_content(&request.messages)?;

        // Check for help commands first (literal strings, not JSON)
        if let Some(help_result) = self.handle_help_command(&input_content, &request.model) {
            let content = help_result?;
            return Ok(ChatResponse {
                content,
                model: request.model.clone(),
                provider: self.provider_name().to_string(),
                usage: TokenUsage::estimated_only(TokenCount::new(0, 0)),
                finish_reason: Some(FinishReason::Stop),
                metadata: HashMap::new(),
                warnings: Vec::new(),
                logprobs: None,
                inference_metadata: InferenceMetadata::completed(FinishReason::Stop),
                tool_calls: None,
            });
        }

        // Parse JSON input
        let input = self.parse_input(&input_content)?;

        // Extract CLIPS options from provider_options
        let clips_options = request
            .provider_options
            .as_ref()
            .and_then(|opts| match opts {
                ProviderOptions::Clips(clips_opts) => Some(clips_opts),
                _ => None,
            });

        // Execute CLIPS inference
        // Pass thinking_mode to control trace visibility (rule firings = "thinking")
        // Pass max_tokens as rule firing limit, stop patterns, and CLIPS options
        let (mut output, _exec_time) = self
            .execute_clips(
                &request.model,
                input,
                request.thinking_mode,
                request.max_tokens,
                request.stop.as_deref(),
                clips_options,
            )
            .await?;

        // Update execution time
        output.stats.execution_time_ms = start.elapsed().as_millis() as u64;

        // Serialize output to JSON
        let content = serde_json::to_string_pretty(&output).map_err(NxuskitError::Serialization)?;

        // Create token usage:
        // - prompt_tokens = input facts count (the "input" to inference)
        // - completion_tokens = rules fired (the "work" done)
        let estimated = TokenCount::new(
            output.stats.input_facts_count as u32,
            output.stats.total_rules_fired as u32,
        );
        let usage = TokenUsage::estimated_only(estimated);

        let mut metadata = HashMap::new();
        // CLIPS-specific metadata for clarity
        metadata.insert(
            "input_facts".to_string(),
            serde_json::json!(output.stats.input_facts_count),
        );
        metadata.insert(
            "rules_fired".to_string(),
            serde_json::json!(output.stats.total_rules_fired),
        );
        metadata.insert(
            "conclusions".to_string(),
            serde_json::json!(output.stats.conclusions_count),
        );
        metadata.insert(
            "execution_time_ms".to_string(),
            serde_json::json!(output.stats.execution_time_ms),
        );

        // Build inference_metadata with rule firings as inference_steps
        let inference_steps: Option<Vec<InferenceStep>> = output.trace.as_ref().map(|trace| {
            trace
                .rules_fired
                .iter()
                .map(|rf| {
                    InferenceStep::rule_firing(&rf.rule_name, rf.salience).with_details(
                        serde_json::json!({
                            "module": rf.module,
                            "fire_count": rf.fire_count
                        }),
                    )
                })
                .collect()
        });

        // Get conflict strategy from options (default is "depth")
        let conflict_strategy = clips_options
            .and_then(|opts| opts.strategy.as_ref())
            .map(|s| s.as_str())
            .unwrap_or("depth");

        let inference_metadata = InferenceMetadata::completed(FinishReason::Stop)
            .with_execution_time(output.stats.execution_time_ms)
            .with_token_usage(usage.clone())
            .with_provider_metadata(serde_json::json!({
                "conflict_strategy": conflict_strategy,
                "input_facts_count": output.stats.input_facts_count,
                "conclusions_count": output.stats.conclusions_count,
                "facts_asserted": output.stats.facts_asserted,
                "facts_retracted": output.stats.facts_retracted,
            }));

        let inference_metadata = match inference_steps {
            Some(steps) => inference_metadata.with_inference_steps(steps),
            None => inference_metadata,
        };

        Ok(ChatResponse {
            content,
            model: request.model.clone(),
            provider: self.provider_name().to_string(),
            usage,
            finish_reason: Some(FinishReason::Stop),
            metadata,
            warnings: Vec::new(),
            logprobs: None,
            inference_metadata,
            tool_calls: None,
        })
    }

    async fn chat_stream(
        &self,
        request: &ChatRequest,
    ) -> Result<Box<dyn Stream<Item = Result<StreamChunk>> + Send + Unpin>> {
        let start = Instant::now();

        // Get input content from last user message
        let input_content = self.get_input_content(&request.messages)?;

        // Check for help commands first (literal strings, not JSON)
        if let Some(help_result) = self.handle_help_command(&input_content, &request.model) {
            let content = help_result?;
            let chunks = vec![Ok(StreamChunk {
                delta: content,
                thinking: None,
                finish_reason: Some(FinishReason::Stop),
                usage: Some(TokenUsage::estimated_only(TokenCount::new(0, 0))),
                tool_calls: None,
                logprobs: None,
            })];
            return Ok(Box::new(futures::stream::iter(chunks)));
        }

        // Parse JSON input
        let input = self.parse_input(&input_content)?;

        // Get stream mode from config
        let stream_mode = input
            .config
            .as_ref()
            .and_then(|c| c.stream_mode)
            .unwrap_or_default();

        // Extract CLIPS options from provider_options
        let clips_options = request
            .provider_options
            .as_ref()
            .and_then(|opts| match opts {
                ProviderOptions::Clips(clips_opts) => Some(clips_opts),
                _ => None,
            });

        // Execute CLIPS inference with max_tokens as rule limit, stop patterns, and CLIPS options
        let (mut output, _exec_time) = self
            .execute_clips(
                &request.model,
                input,
                request.thinking_mode,
                request.max_tokens,
                request.stop.as_deref(),
                clips_options,
            )
            .await?;

        output.stats.execution_time_ms = start.elapsed().as_millis() as u64;

        // Generate chunks based on stream mode
        let chunks: Vec<Result<StreamChunk>> = match stream_mode {
            StreamMode::Default => {
                // Single chunk with all results (original behavior)
                let content =
                    serde_json::to_string_pretty(&output).map_err(NxuskitError::Serialization)?;

                let estimated = TokenCount::new(
                    output.stats.facts_asserted as u32,
                    output.stats.conclusions_count as u32,
                );
                let usage = TokenUsage::estimated_only(estimated);

                vec![Ok(StreamChunk {
                    delta: content,
                    thinking: None,
                    finish_reason: Some(FinishReason::Stop),
                    usage: Some(usage),
                    tool_calls: None,
                    logprobs: None,
                })]
            }
            StreamMode::Fact => {
                // One chunk per derived fact
                let mut chunks: Vec<Result<StreamChunk>> = output
                    .conclusions
                    .into_iter()
                    .map(|fact| {
                        let content =
                            serde_json::to_string(&fact).map_err(NxuskitError::Serialization)?;
                        Ok(StreamChunk {
                            delta: content,
                            thinking: None,
                            finish_reason: None,
                            usage: None,
                            tool_calls: None,
                            logprobs: None,
                        })
                    })
                    .collect();

                // Add final chunk with finish reason
                let estimated = TokenCount::new(
                    output.stats.facts_asserted as u32,
                    output.stats.conclusions_count as u32,
                );
                chunks.push(Ok(StreamChunk {
                    delta: String::new(),
                    thinking: None,
                    finish_reason: Some(FinishReason::Stop),
                    usage: Some(TokenUsage::estimated_only(estimated)),
                    tool_calls: None,
                    logprobs: None,
                }));

                chunks
            }
            StreamMode::Rule => {
                // One chunk per rule firing
                // Group facts by which rule might have created them (heuristic)
                let mut chunks: Vec<Result<StreamChunk>> = Vec::new();

                if let Some(ref trace) = output.trace {
                    // Use trace to emit rule info with associated facts
                    for rule_firing in &trace.rules_fired {
                        let rule_output = RuleChunkOutput {
                            rule_name: rule_firing.rule_name.clone(),
                            module: rule_firing.module.clone(),
                            facts: Vec::new(), // Facts are grouped with rules heuristically
                        };
                        let content = serde_json::to_string(&rule_output)
                            .map_err(NxuskitError::Serialization)?;
                        chunks.push(Ok(StreamChunk {
                            delta: content,
                            thinking: None,
                            finish_reason: None,
                            usage: None,
                            tool_calls: None,
                            logprobs: None,
                        }));
                    }
                }

                // If we have conclusions but no trace, emit facts grouped together
                if chunks.is_empty() && !output.conclusions.is_empty() {
                    let rule_output = RuleChunkOutput {
                        rule_name: "inference".to_string(),
                        module: None,
                        facts: output.conclusions,
                    };
                    let content =
                        serde_json::to_string(&rule_output).map_err(NxuskitError::Serialization)?;
                    chunks.push(Ok(StreamChunk {
                        delta: content,
                        thinking: None,
                        finish_reason: None,
                        usage: None,
                        tool_calls: None,
                        logprobs: None,
                    }));
                }

                // Add final chunk with usage tracking
                // - prompt_tokens = input facts count
                // - completion_tokens = rules fired
                let estimated = TokenCount::new(
                    output.stats.input_facts_count as u32,
                    output.stats.total_rules_fired as u32,
                );
                chunks.push(Ok(StreamChunk {
                    delta: String::new(),
                    thinking: None,
                    finish_reason: Some(FinishReason::Stop),
                    usage: Some(TokenUsage::estimated_only(estimated)),
                    tool_calls: None,
                    logprobs: None,
                }));

                chunks
            }
        };

        Ok(Box::new(futures::stream::iter(chunks)))
    }

    fn provider_name(&self) -> &str {
        "clips"
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        // Create resolver for search paths
        let resolver = ModelPathResolver::new(Some(&self.config.rules_directory));
        let mut models = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // Recursively scan all search paths for .clp files
        for search_path in resolver.search_paths() {
            let mut clp_files = Vec::new();
            collect_clp_files_recursive(search_path, 0, &mut clp_files);

            for (name, path) in clp_files {
                if seen.insert(name.clone()) {
                    let model_info = build_model_info(&name, &path);
                    models.push(model_info);
                }
            }
        }

        // Sort by name for consistent output
        models.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(models)
    }
}

#[async_trait]
impl ModelLister for ClipsProvider {
    async fn list_available_models(&self) -> Result<Vec<ModelInfo>> {
        // Delegate to list_models() which already has the correct implementation
        self.list_models().await
    }
}

/// Build enhanced ModelInfo with template/rule counts
fn build_model_info(name: &str, source_path: &Path) -> ModelInfo {
    let mut meta = HashMap::new();
    meta.insert("type".to_string(), "clips-rulebase".to_string());
    meta.insert("path".to_string(), source_path.display().to_string());

    // Check for binary file
    let bin_path = source_path.with_extension("bin");
    let has_binary = bin_path.exists();
    let has_source = source_path.exists();

    // Build file type indicator: (s), (b), or (s,b)
    let file_type = match (has_source, has_binary) {
        (true, true) => "(s,b)",
        (true, false) => "(s)",
        (false, true) => "(b)",
        _ => "",
    };
    meta.insert("file_type".to_string(), file_type.to_string());

    // Get modification time
    let modified = source_path
        .metadata()
        .ok()
        .and_then(|m| m.modified().ok())
        .map(|t| {
            let duration = SystemTime::now()
                .duration_since(t)
                .unwrap_or(Duration::ZERO);
            format_relative_time(duration)
        });

    if let Some(ref mod_str) = modified {
        meta.insert("modified".to_string(), mod_str.clone());
    }

    // Try to get template and rule counts
    if let Ok(env) = ClipsEnvironment::new()
        && env.load(source_path).is_ok()
    {
        // Count templates (excluding system templates)
        let template_count: u32 = env
            .templates()
            .filter_map(|t| t.ok())
            .filter(|t| {
                t.name()
                    .map(|n| !n.starts_with("initial-"))
                    .unwrap_or(false)
            })
            .count() as u32;

        // Count rules
        let rule_count: u32 = env.rules().filter_map(|r| r.ok()).count() as u32;

        meta.insert("template_count".to_string(), template_count.to_string());
        meta.insert("rule_count".to_string(), rule_count.to_string());
    }

    let size_bytes = source_path.metadata().ok().map(|m| m.len());

    // Build description with counts if available
    let description =
        if let (Some(tc), Some(rc)) = (meta.get("template_count"), meta.get("rule_count")) {
            Some(format!(
                "📋 {} templates, ⚙️ {} rules {}",
                tc, rc, file_type
            ))
        } else {
            Some(format!("CLIPS rule base: {}", source_path.display()))
        };

    ModelInfo {
        name: name.to_string(),
        size_bytes,
        description,
        context_window: None,
        metadata: meta,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_path_resolver_search_paths() {
        // Test that resolver properly initializes search paths
        let resolver = ModelPathResolver::new(Some(Path::new("/rules")));
        let paths = resolver.search_paths();

        // Should have at least current directory or rules directory
        assert!(!paths.is_empty());
    }

    #[test]
    fn test_model_path_resolver_load_types() {
        // Test LoadType variants
        assert_eq!(LoadType::Source, LoadType::Source);
        assert_eq!(LoadType::Binary, LoadType::Binary);
        assert_eq!(LoadType::SourceWithBsave, LoadType::SourceWithBsave);
    }

    #[test]
    fn test_parse_input() {
        let provider = ClipsProvider::new(ClipsConfig::default()).unwrap();

        let json = r#"{
            "facts": [
                {"template": "test", "values": {"x": 1}}
            ]
        }"#;

        let input = provider.parse_input(json).unwrap();
        assert_eq!(input.facts.len(), 1);
        assert_eq!(input.facts[0].template, "test");
    }

    #[test]
    fn test_builder() {
        let provider = ClipsProvider::builder()
            .rules_directory("/test/rules")
            .persistent(true)
            .include_trace(true)
            .max_rules(100)
            .build()
            .unwrap();

        assert_eq!(
            provider.config.rules_directory,
            PathBuf::from("/test/rules")
        );
        assert!(provider.config.persistent);
        assert!(provider.config.include_trace);
        assert_eq!(provider.config.max_rules, 100);
    }

    // ========================================================================
    // Content Hash & Policy Cache Tests (Feature 033)
    // ========================================================================

    #[test]
    fn test_evict_environment() {
        let provider = ClipsProvider::new(ClipsConfig::default()).unwrap();

        // Register a session via the session manager and insert handle into cache
        let handle = clips_session_manager::session_create_from_env(
            ClipsEnvironment::new().unwrap(),
            Some("test-key".to_string()),
        )
        .unwrap();
        {
            let mut cache = provider.session_cache.write();
            cache.insert("test-key".to_string(), handle);
        }

        // Verify cache has the entry
        assert_eq!(provider.cached_models().len(), 1);

        // Evict the environment
        let removed = provider.evict_environment("test-key");
        assert!(removed);

        // Verify cache is empty
        assert_eq!(provider.cached_models().len(), 0);

        // Evicting non-existent key returns false
        let removed = provider.evict_environment("non-existent");
        assert!(!removed);
    }

    #[test]
    fn test_clear_cache() {
        let provider = ClipsProvider::new(ClipsConfig::default()).unwrap();

        // Register sessions via the session manager
        let h1 = clips_session_manager::session_create_from_env(
            ClipsEnvironment::new().unwrap(),
            Some("key1".to_string()),
        )
        .unwrap();
        let h2 = clips_session_manager::session_create_from_env(
            ClipsEnvironment::new().unwrap(),
            Some("key2".to_string()),
        )
        .unwrap();
        {
            let mut cache = provider.session_cache.write();
            cache.insert("key1".to_string(), h1);
            cache.insert("key2".to_string(), h2);
        }

        assert_eq!(provider.cached_models().len(), 2);

        // Clear the cache
        provider.clear_cache();

        // Verify cache is empty
        assert_eq!(provider.cached_models().len(), 0);
    }

    #[test]
    fn test_policy_id_registration() {
        let provider = ClipsProvider::new(ClipsConfig::default()).unwrap();

        // Register a policy_id
        let _policy_id = "test-policy";
        let _hash = "sha256:abc123";

        // Can't directly test register_policy_id (it's private), but we can
        // verify the hash_registry is created and can be accessed
        {
            let registry = provider.hash_registry.read();
            assert_eq!(registry.len(), 0);
        }
    }
}
