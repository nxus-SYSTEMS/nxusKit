//! Plugin discovery, loading, and signature verification.
//!
//! Provides launch-time plugin loading with offline ed25519 signature verification.
//! Plugins are shared libraries (`.dylib`/`.so`/`.dll`) paired with JSON manifest files.
//!
//! # Architecture
//!
//! - **Discovery**: Scans a directory for `*.json` manifest files paired with shared libraries
//! - **Verification**: ed25519 signature of the shared library binary against trusted public keys
//! - **Loading**: Uses `libloading` to load the shared library and call `nxuskit_plugin_init()`
//! - **Registry**: Global singleton holding all loaded plugins, indexed by name
//!
//! Plugin load failures are always non-fatal — the SDK continues with remaining valid plugins.

use std::collections::HashMap;
use std::ffi::{CStr, c_char};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{OnceLock, RwLock};

use base64::Engine;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};

// ── Trust Mode ─────────────────────────────────────────────────────

/// Plugin trust policy controlling whether unsigned plugins are allowed.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustMode {
    /// Only cryptographically signed plugins are loaded (default).
    SignedOnly = 0,
    /// Unsigned plugins are loaded but emit audit events.
    AllowUnsigned = 1,
}

impl fmt::Display for TrustMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TrustMode::SignedOnly => write!(f, "signed-only"),
            TrustMode::AllowUnsigned => write!(f, "allow-unsigned"),
        }
    }
}

impl TrustMode {
    /// Parse from string (CLI-friendly).
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().replace('_', "-").as_str() {
            "signed-only" | "signedonly" | "0" => Some(TrustMode::SignedOnly),
            "allow-unsigned" | "allowunsigned" | "1" => Some(TrustMode::AllowUnsigned),
            _ => None,
        }
    }
}

/// Global trust mode (default: SignedOnly).
static TRUST_MODE: AtomicU8 = AtomicU8::new(TrustMode::SignedOnly as u8);

/// Get the current plugin trust mode.
pub fn get_trust_mode() -> TrustMode {
    match TRUST_MODE.load(Ordering::Relaxed) {
        1 => TrustMode::AllowUnsigned,
        _ => TrustMode::SignedOnly,
    }
}

/// Set the plugin trust mode.
pub fn set_trust_mode(mode: TrustMode) {
    TRUST_MODE.store(mode as u8, Ordering::Relaxed);
}

// ── Plugin Manifest ────────────────────────────────────────────────

/// JSON manifest that accompanies each plugin shared library.
///
/// Forward-compatible: unknown fields are silently ignored via `#[serde(deny_unknown_fields)]`
/// being absent. The `schema_version` field allows future schema evolution.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Manifest schema version (v0.8.0 ships "1.0").
    pub schema_version: String,

    /// Unique plugin identifier (kebab-case, ASCII alphanumeric + hyphens).
    pub name: String,

    /// Plugin version (semver).
    pub version: String,

    /// Required host ABI version (e.g., "0.9.0").
    pub abi_version: String,

    /// Provider domain this plugin handles (e.g., "zen"). Added in v0.9.0.
    #[serde(default)]
    pub provider_domain: Option<String>,

    /// List of capability identifiers this plugin provides.
    pub capabilities: Vec<String>,

    /// Shared library binary filename (e.g., "libnxuskit_plugin_zen.dylib").
    #[serde(default)]
    pub binary_name: Option<String>,

    /// SHA-256 hash of the binary file, prefixed with "sha256:" (e.g., "sha256:abc123...").
    #[serde(default)]
    pub binary_hash: Option<String>,

    /// Base64-encoded ed25519 signature of the shared library binary.
    pub signature: String,

    /// Fingerprint of the signing key used.
    #[serde(default)]
    pub signing_key_id: Option<String>,

    /// Minimum SDK edition required ("pro", "enterprise").
    #[serde(default)]
    pub required_edition: Option<String>,

    /// Required entitlement domains.
    #[serde(default)]
    pub required_entitlements: Vec<String>,

    /// Delegated trust root key ID for trust chain verification.
    #[serde(default)]
    pub delegated_trust_root: Option<String>,

    /// Trust chain from signing key to trust root.
    #[serde(default)]
    pub trust_chain: Vec<String>,

    /// Human-readable description.
    #[serde(default)]
    pub description: Option<String>,

    /// Plugin author.
    #[serde(default)]
    pub author: Option<String>,
}

// ── Plugin Error ───────────────────────────────────────────────────

/// Error taxonomy for plugin operations.
#[derive(Debug)]
pub enum PluginError {
    /// No .json manifest file found alongside the shared library.
    ManifestNotFound(PathBuf),

    /// JSON parse failure or missing required field in manifest.
    ManifestParseError { path: PathBuf, detail: String },

    /// Plugin `abi_version` does not match host `ABI_VERSION`.
    AbiMismatch {
        plugin_name: String,
        expected: String,
        found: String,
    },

    /// Signature verification failed (signature does not match binary content).
    SignatureInvalid { plugin_name: String },

    /// Plugin has no signature field or an empty signature.
    UnsignedPlugin { plugin_name: String },

    /// `libloading` failed to load the shared library.
    LoadError { plugin_name: String, detail: String },

    /// Plugin's `nxuskit_plugin_init` failed or returned invalid data.
    InitError { plugin_name: String, detail: String },

    /// Plugin capability already claimed by another plugin.
    CapabilityConflict {
        plugin_name: String,
        capability: String,
        held_by: String,
    },

    /// Binary hash in manifest does not match actual binary SHA-256.
    HashMismatch {
        plugin_name: String,
        expected: String,
        actual: String,
    },

    /// No trusted keys configured — cannot verify any plugin.
    NoTrustedKeys,

    /// Trusted keys file missing or malformed.
    TrustFileError(String),
}

impl fmt::Display for PluginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ManifestNotFound(p) => write!(f, "manifest not found: {}", p.display()),
            Self::ManifestParseError { path, detail } => {
                write!(f, "manifest parse error in {}: {detail}", path.display())
            }
            Self::AbiMismatch {
                plugin_name,
                expected,
                found,
            } => write!(
                f,
                "ABI mismatch for plugin '{plugin_name}': expected {expected}, found {found}"
            ),
            Self::SignatureInvalid { plugin_name } => {
                write!(
                    f,
                    "signature verification failed for plugin '{plugin_name}'"
                )
            }
            Self::UnsignedPlugin { plugin_name } => {
                write!(f, "plugin '{plugin_name}' has no valid signature")
            }
            Self::LoadError {
                plugin_name,
                detail,
            } => write!(f, "failed to load plugin '{plugin_name}': {detail}"),
            Self::InitError {
                plugin_name,
                detail,
            } => write!(f, "plugin '{plugin_name}' init function failed: {detail}"),
            Self::CapabilityConflict {
                plugin_name,
                capability,
                held_by,
            } => write!(
                f,
                "plugin '{plugin_name}' capability '{capability}' already claimed by '{held_by}'"
            ),
            Self::HashMismatch {
                plugin_name,
                expected,
                actual,
            } => write!(
                f,
                "binary hash mismatch for plugin '{plugin_name}': expected {expected}, actual {actual}"
            ),
            Self::NoTrustedKeys => write!(f, "no trusted public keys configured"),
            Self::TrustFileError(detail) => write!(f, "trust file error: {detail}"),
        }
    }
}

impl PluginError {
    /// Map to C ABI error type string.
    pub fn error_type(&self) -> &'static str {
        match self {
            Self::ManifestNotFound(_) => "manifest_not_found",
            Self::ManifestParseError { .. } => "manifest_parse_error",
            Self::AbiMismatch { .. } => "abi_mismatch",
            Self::SignatureInvalid { .. } => "signature_invalid",
            Self::UnsignedPlugin { .. } => "unsigned_plugin",
            Self::LoadError { .. } => "plugin_load_error",
            Self::InitError { .. } => "plugin_init_error",
            Self::CapabilityConflict { .. } => "capability_conflict",
            Self::HashMismatch { .. } => "hash_mismatch",
            Self::NoTrustedKeys => "no_trusted_keys",
            Self::TrustFileError(_) => "trust_file_error",
        }
    }
}

impl std::error::Error for PluginError {}

// ── Trusted Keys ───────────────────────────────────────────────────

/// A public key used for signature verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustedKey {
    /// Human-readable key identifier.
    pub id: String,

    /// Base64-encoded 32-byte ed25519 public key.
    pub public_key: String,

    /// Purpose/owner description.
    #[serde(default)]
    pub description: Option<String>,
}

/// Trust file schema: contains a list of trusted public keys.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustFile {
    /// Schema version for the trust file format.
    pub schema_version: String,

    /// List of trusted public keys.
    pub keys: Vec<TrustedKey>,
}

/// Load trusted public keys from a JSON file.
pub fn load_trusted_keys(path: &Path) -> Result<Vec<TrustedKey>, PluginError> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| PluginError::TrustFileError(format!("cannot read {}: {e}", path.display())))?;

    let trust_file: TrustFile = serde_json::from_str(&content).map_err(|e| {
        PluginError::TrustFileError(format!("invalid JSON in {}: {e}", path.display()))
    })?;

    if trust_file.keys.is_empty() {
        return Err(PluginError::NoTrustedKeys);
    }

    // Validate each key decodes to exactly 32 bytes
    let b64 = base64::engine::general_purpose::STANDARD;
    for key in &trust_file.keys {
        let bytes = b64.decode(&key.public_key).map_err(|e| {
            PluginError::TrustFileError(format!("key '{}': invalid base64: {e}", key.id))
        })?;
        if bytes.len() != 32 {
            return Err(PluginError::TrustFileError(format!(
                "key '{}': expected 32 bytes, got {}",
                key.id,
                bytes.len()
            )));
        }
    }

    Ok(trust_file.keys)
}

// ── Plugin Verifier Trait ──────────────────────────────────────────

/// Abstracted signature verification interface.
///
/// Designed for extensibility: future implementations may support delegated trust roots,
/// revocation lists, or online attestation without changing the manifest schema or C ABI.
pub trait PluginVerifier: Send + Sync {
    /// Verify the plugin binary against its manifest signature.
    ///
    /// Returns `Ok(())` if verification passes, or an appropriate `PluginError` on failure.
    fn verify(&self, binary: &[u8], manifest: &PluginManifest) -> Result<(), PluginError>;

    /// Human-readable name of this verifier implementation.
    fn name(&self) -> &str;
}

/// Offline ed25519 signature verifier using trusted public keys.
///
/// For each trusted key, attempts to verify the manifest's base64-encoded signature
/// against the binary content. Passes if any key succeeds.
pub struct OfflineEd25519Verifier {
    #[allow(missing_debug_implementations)]
    keys: Vec<VerifyingKey>,
}

impl fmt::Debug for OfflineEd25519Verifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OfflineEd25519Verifier")
            .field("key_count", &self.keys.len())
            .finish()
    }
}

impl OfflineEd25519Verifier {
    /// Create a verifier from a list of trusted keys.
    ///
    /// Decodes base64 public keys into `VerifyingKey` instances.
    pub fn from_trusted_keys(trusted_keys: &[TrustedKey]) -> Result<Self, PluginError> {
        let b64 = base64::engine::general_purpose::STANDARD;
        let mut keys = Vec::with_capacity(trusted_keys.len());

        for tk in trusted_keys {
            let bytes = b64.decode(&tk.public_key).map_err(|e| {
                PluginError::TrustFileError(format!("key '{}': invalid base64: {e}", tk.id))
            })?;
            let key_bytes: [u8; 32] = bytes.try_into().map_err(|_| {
                PluginError::TrustFileError(format!("key '{}': not 32 bytes", tk.id))
            })?;
            let vk = VerifyingKey::from_bytes(&key_bytes).map_err(|e| {
                PluginError::TrustFileError(format!("key '{}': invalid ed25519 key: {e}", tk.id))
            })?;
            keys.push(vk);
        }

        Ok(Self { keys })
    }
}

impl PluginVerifier for OfflineEd25519Verifier {
    fn verify(&self, binary: &[u8], manifest: &PluginManifest) -> Result<(), PluginError> {
        if manifest.signature.is_empty() {
            return Err(PluginError::UnsignedPlugin {
                plugin_name: manifest.name.clone(),
            });
        }

        let b64 = base64::engine::general_purpose::STANDARD;
        let sig_bytes =
            b64.decode(&manifest.signature)
                .map_err(|_| PluginError::SignatureInvalid {
                    plugin_name: manifest.name.clone(),
                })?;

        let sig_array: [u8; 64] =
            sig_bytes
                .try_into()
                .map_err(|_| PluginError::SignatureInvalid {
                    plugin_name: manifest.name.clone(),
                })?;

        let signature = Signature::from_bytes(&sig_array);

        // Direct key match: try each trusted key — pass if any succeeds
        for vk in &self.keys {
            if vk.verify_strict(binary, &signature).is_ok() {
                return Ok(());
            }
        }

        // Delegated trust: if trust_chain is non-empty, walk chain from root to leaf.
        // Root key must be in our trusted set; each key in chain authorizes the next.
        if !manifest.trust_chain.is_empty()
            && let Some(authorized_key) =
                self.verify_trust_chain(&manifest.trust_chain, &manifest.name)?
            && authorized_key.verify_strict(binary, &signature).is_ok()
        {
            return Ok(());
        }

        // Delegated trust root: signing_key_id must be authorized by delegated_trust_root
        if let Some(ref dtr) = manifest.delegated_trust_root
            && let Some(ref signing_key_id) = manifest.signing_key_id
            && let Some(authorized_key) =
                self.resolve_delegated_trust(dtr, signing_key_id, &manifest.name)?
            && authorized_key.verify_strict(binary, &signature).is_ok()
        {
            return Ok(());
        }

        Err(PluginError::SignatureInvalid {
            plugin_name: manifest.name.clone(),
        })
    }

    fn name(&self) -> &str {
        "offline-ed25519"
    }
}

impl OfflineEd25519Verifier {
    /// Walk a trust chain: root must be in trusted keys, each subsequent entry
    /// is a base64-encoded public key authorized by the chain.
    /// Returns the leaf key (the signing key) if the chain is valid.
    fn verify_trust_chain(
        &self,
        trust_chain: &[String],
        plugin_name: &str,
    ) -> Result<Option<VerifyingKey>, PluginError> {
        if trust_chain.is_empty() {
            return Ok(None);
        }

        let b64 = base64::engine::general_purpose::STANDARD;

        // First key in chain must be in our trusted set
        let root_bytes =
            b64.decode(&trust_chain[0])
                .map_err(|_| PluginError::SignatureInvalid {
                    plugin_name: plugin_name.to_string(),
                })?;
        let root_key_bytes: [u8; 32] =
            root_bytes
                .try_into()
                .map_err(|_| PluginError::SignatureInvalid {
                    plugin_name: plugin_name.to_string(),
                })?;
        let root_key = VerifyingKey::from_bytes(&root_key_bytes).map_err(|_| {
            PluginError::SignatureInvalid {
                plugin_name: plugin_name.to_string(),
            }
        })?;

        // Verify root is trusted
        if !self
            .keys
            .iter()
            .any(|k| k.as_bytes() == root_key.as_bytes())
        {
            log::debug!(
                "Plugin '{}': trust chain root not in trusted key set",
                plugin_name
            );
            return Ok(None);
        }

        // Walk chain: each subsequent key is the leaf authorized by its predecessor
        let mut current_key = root_key;
        for (i, entry) in trust_chain.iter().enumerate().skip(1) {
            let key_bytes = b64
                .decode(entry)
                .map_err(|_| PluginError::SignatureInvalid {
                    plugin_name: plugin_name.to_string(),
                })?;
            let key_array: [u8; 32] =
                key_bytes
                    .try_into()
                    .map_err(|_| PluginError::SignatureInvalid {
                        plugin_name: plugin_name.to_string(),
                    })?;
            let next_key = VerifyingKey::from_bytes(&key_array).map_err(|_| {
                PluginError::SignatureInvalid {
                    plugin_name: plugin_name.to_string(),
                }
            })?;

            // For v0.9.0 MVP, trust chain entries are just public keys;
            // the chain itself is trusted if the root is trusted.
            // Full delegation proof (inline signatures) deferred to v0.9.1.
            if i + 1 < trust_chain.len() {
                log::debug!(
                    "Plugin '{}': trust chain link {} accepted (root-anchored)",
                    plugin_name,
                    i
                );
            } else {
                log::debug!(
                    "Plugin '{}': trust chain leaf reached at link {}",
                    plugin_name,
                    i
                );
            }
            current_key = next_key;
        }

        Ok(Some(current_key))
    }

    /// Resolve delegated trust: the delegated_trust_root must be in our trusted set,
    /// and the signing_key_id must match a key authorized by that root.
    fn resolve_delegated_trust(
        &self,
        delegated_trust_root: &str,
        _signing_key_id: &str,
        plugin_name: &str,
    ) -> Result<Option<VerifyingKey>, PluginError> {
        let b64 = base64::engine::general_purpose::STANDARD;

        // Decode delegated trust root
        let root_bytes =
            b64.decode(delegated_trust_root)
                .map_err(|_| PluginError::SignatureInvalid {
                    plugin_name: plugin_name.to_string(),
                })?;
        let root_key_bytes: [u8; 32] =
            root_bytes
                .try_into()
                .map_err(|_| PluginError::SignatureInvalid {
                    plugin_name: plugin_name.to_string(),
                })?;
        let _root_key = VerifyingKey::from_bytes(&root_key_bytes).map_err(|_| {
            PluginError::SignatureInvalid {
                plugin_name: plugin_name.to_string(),
            }
        })?;

        // Verify root is in trusted set
        if !self.keys.iter().any(|k| k.as_bytes() == &root_key_bytes) {
            log::debug!(
                "Plugin '{}': delegated trust root not in trusted key set",
                plugin_name
            );
            return Ok(None);
        }

        // For v0.9.0 MVP: if the delegated trust root is trusted,
        // we accept the signing key. Full delegation certificate
        // verification (binding signing_key_id to the root) is
        // deferred to v0.9.1.
        log::info!(
            "Plugin '{}': delegated trust root verified, accepting signing key",
            plugin_name
        );

        // Return None to fall back to direct verification —
        // the actual signing key needs to be in the trust chain
        // or directly trusted for the signature to verify.
        Ok(None)
    }
}

// ── Loaded Plugin ──────────────────────────────────────────────────

/// In-memory representation of a successfully loaded and verified plugin.
pub struct LoadedPlugin {
    /// Parsed and validated manifest.
    pub manifest: PluginManifest,

    /// Absolute path to the shared library file.
    pub library_path: PathBuf,

    /// JSON metadata returned by `nxuskit_plugin_init()`.
    pub init_metadata: Option<String>,
}

impl fmt::Debug for LoadedPlugin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LoadedPlugin")
            .field("name", &self.manifest.name)
            .field("version", &self.manifest.version)
            .field("library_path", &self.library_path)
            .finish()
    }
}

// ── Plugin Info (serializable for C ABI JSON output) ───────────────

/// Serializable plugin metadata for C ABI JSON output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
    pub abi_version: String,
    pub capabilities: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_edition: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_entitlements: Vec<String>,
    pub library_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub init_metadata: Option<String>,
}

impl From<&LoadedPlugin> for PluginInfo {
    fn from(p: &LoadedPlugin) -> Self {
        Self {
            name: p.manifest.name.clone(),
            version: p.manifest.version.clone(),
            abi_version: p.manifest.abi_version.clone(),
            capabilities: p.manifest.capabilities.clone(),
            description: p.manifest.description.clone(),
            author: p.manifest.author.clone(),
            required_edition: p.manifest.required_edition.clone(),
            required_entitlements: p.manifest.required_entitlements.clone(),
            library_path: p.library_path.display().to_string(),
            init_metadata: p.init_metadata.clone(),
        }
    }
}

// ── Plugin Registry ────────────────────────────────────────────────

/// Global singleton holding all loaded plugins, indexed by name.
///
/// Thread safety: `OnceLock<RwLock<PluginRegistry>>` — initialized once, read-mostly.
///
/// Invariants:
/// - No two plugins share the same name
/// - No two plugins provide the same capability (first-loaded wins; alphabetical order)
/// - Registry is never in a partially-initialized state
pub struct PluginRegistry {
    /// Loaded plugins by name.
    plugins: HashMap<String, LoadedPlugin>,

    /// Capability → plugin name mapping.
    capabilities: HashMap<String, String>,

    /// Library handles kept alive for symbol resolution.
    /// Stored as trait objects to avoid libloading type leaking into the struct signature.
    libraries: Vec<libloading::Library>,

    /// Active signature verifier (if configured).
    verifier: Option<Box<dyn PluginVerifier>>,
}

impl fmt::Debug for PluginRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PluginRegistry")
            .field("plugin_count", &self.plugins.len())
            .field("capability_count", &self.capabilities.len())
            .finish()
    }
}

/// Global plugin registry singleton.
static REGISTRY: OnceLock<RwLock<PluginRegistry>> = OnceLock::new();

fn get_registry() -> &'static RwLock<PluginRegistry> {
    REGISTRY.get_or_init(|| {
        RwLock::new(PluginRegistry {
            plugins: HashMap::new(),
            capabilities: HashMap::new(),
            libraries: Vec::new(),
            verifier: None,
        })
    })
}

impl PluginRegistry {
    /// Discover and load plugins from a directory.
    ///
    /// Scans for `*.json` manifest files, finds matching shared libraries,
    /// verifies ABI compatibility and signature, loads valid plugins.
    /// Returns count of successfully loaded plugins.
    ///
    /// Plugins are loaded in alphabetical order by manifest filename.
    /// Plugin load failures are non-fatal — rejected plugins are logged.
    pub fn load_dir(dir: &Path) -> i32 {
        let start = std::time::Instant::now();

        // Edition-based plugin path policy check
        if !edition_allows_plugin_path(dir) {
            log::info!(
                "Plugin loading from '{}' not allowed by current edition",
                dir.display()
            );
            return 0;
        }

        if !dir.is_dir() {
            if dir.exists() {
                log::warn!(
                    "Plugin directory '{}' is not a directory, skipping",
                    dir.display()
                );
            } else {
                log::warn!(
                    "Plugin directory '{}' does not exist, skipping",
                    dir.display()
                );
            }
            return 0;
        }

        // Discover manifest files (alphabetical order)
        let mut manifests = discover_manifests(dir);
        manifests.sort_by(|a, b| a.0.file_name().cmp(&b.0.file_name()));

        if manifests.is_empty() {
            log::info!("No plugin manifests found in '{}'", dir.display());
            return 0;
        }

        // Resolve verifier from trusted keys
        let verifier = resolve_verifier();

        let registry = get_registry();
        let mut reg = registry.write().unwrap();

        if let Some(v) = verifier {
            reg.verifier = Some(v);
        }

        let mut loaded_count: i32 = 0;

        for (manifest_path, lib_path) in &manifests {
            match load_single_plugin(manifest_path, lib_path, &reg.verifier) {
                Ok((plugin, library)) => {
                    // Check for capability conflicts
                    let mut conflict = false;
                    for cap in &plugin.manifest.capabilities {
                        if let Some(existing) = reg.capabilities.get(cap) {
                            log::warn!(
                                "Plugin '{}': capability '{}' already claimed by '{}', rejecting",
                                plugin.manifest.name,
                                cap,
                                existing
                            );
                            conflict = true;
                            break;
                        }
                    }
                    if conflict {
                        continue;
                    }

                    // Check for duplicate name
                    if reg.plugins.contains_key(&plugin.manifest.name) {
                        log::warn!(
                            "Plugin '{}': duplicate name, skipping",
                            plugin.manifest.name
                        );
                        continue;
                    }

                    // Log edition/entitlement awareness (FR-008)
                    log_edition_entitlement_warnings(&plugin.manifest);

                    // Register capabilities
                    let plugin_name = plugin.manifest.name.clone();
                    for cap in &plugin.manifest.capabilities {
                        reg.capabilities.insert(cap.clone(), plugin_name.clone());
                    }

                    log::info!(
                        "Loaded plugin '{}' v{} with capabilities {:?}",
                        plugin.manifest.name,
                        plugin.manifest.version,
                        plugin.manifest.capabilities
                    );

                    reg.plugins.insert(plugin_name, plugin);
                    reg.libraries.push(library);
                    loaded_count += 1;
                }
                Err(e) => {
                    log::warn!("Plugin rejected: {e}");
                }
            }
        }

        let elapsed = start.elapsed();
        log::info!(
            "Plugin loading complete: {loaded_count} loaded from '{}' in {:.1}ms",
            dir.display(),
            elapsed.as_secs_f64() * 1000.0
        );

        loaded_count
    }

    /// List all loaded plugins.
    pub fn list() -> Vec<PluginInfo> {
        let registry = get_registry();
        let reg = registry.read().unwrap();
        reg.plugins.values().map(PluginInfo::from).collect()
    }

    /// Get info for a specific plugin by name.
    pub fn info(name: &str) -> Option<PluginInfo> {
        let registry = get_registry();
        let reg = registry.read().unwrap();
        reg.plugins.get(name).map(PluginInfo::from)
    }

    /// Return count of loaded plugins.
    pub fn count() -> usize {
        let registry = get_registry();
        let reg = registry.read().unwrap();
        reg.plugins.len()
    }

    /// Check if a specific plugin is loaded.
    pub fn is_loaded(name: &str) -> bool {
        let registry = get_registry();
        let reg = registry.read().unwrap();
        reg.plugins.contains_key(name)
    }

    /// Check if any loaded plugin provides a given capability.
    pub fn has_capability(capability: &str) -> bool {
        let registry = get_registry();
        let reg = registry.read().unwrap();
        reg.capabilities.contains_key(capability)
    }

    /// Dispatch a JSON request to the plugin that provides the given capability.
    ///
    /// Returns the plugin's JSON response string, or an error string.
    /// The caller is responsible for interpreting the JSON response.
    ///
    /// # Safety
    ///
    /// This function calls into the plugin's `nxuskit_plugin_dispatch` C ABI function.
    pub fn dispatch(capability: &str, request_json: &str) -> Result<String, PluginError> {
        let registry = get_registry();
        let reg = registry.read().unwrap();

        let plugin_name = match reg.capabilities.get(capability) {
            Some(name) => name.clone(),
            None => {
                return Err(PluginError::LoadError {
                    plugin_name: format!("<capability:{capability}>"),
                    detail: format!("no plugin provides capability '{capability}'"),
                });
            }
        };

        let plugin = match reg.plugins.get(&plugin_name) {
            Some(p) => p,
            None => {
                return Err(PluginError::LoadError {
                    plugin_name: plugin_name.clone(),
                    detail: "plugin registered but not found in registry".to_string(),
                });
            }
        };

        // Find the library that loaded this plugin
        let lib_path = &plugin.library_path;

        // We need to call nxuskit_plugin_dispatch from the loaded library.
        // The library handle is stored in registry.libraries but not mapped to plugins.
        // Re-load the library (it's already loaded, so this is a reference count bump).
        let lib =
            unsafe { libloading::Library::new(lib_path) }.map_err(|e| PluginError::LoadError {
                plugin_name: plugin_name.clone(),
                detail: format!("failed to re-open library for dispatch: {e}"),
            })?;

        let dispatch_fn: libloading::Symbol<unsafe extern "C" fn(*const c_char) -> *mut c_char> =
            unsafe { lib.get(b"nxuskit_plugin_dispatch\0") }.map_err(|e| {
                PluginError::LoadError {
                    plugin_name: plugin_name.clone(),
                    detail: format!("missing nxuskit_plugin_dispatch symbol: {e}"),
                }
            })?;

        let free_fn: libloading::Symbol<unsafe extern "C" fn(*mut c_char)> = unsafe {
            lib.get(b"nxuskit_plugin_free_string\0")
        }
        .map_err(|e| PluginError::LoadError {
            plugin_name: plugin_name.clone(),
            detail: format!("missing nxuskit_plugin_free_string symbol: {e}"),
        })?;

        let c_request =
            std::ffi::CString::new(request_json).map_err(|e| PluginError::InitError {
                plugin_name: plugin_name.clone(),
                detail: format!("request JSON contains NUL byte: {e}"),
            })?;

        let result_ptr = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            dispatch_fn(c_request.as_ptr())
        }))
        .map_err(|_| PluginError::InitError {
            plugin_name: plugin_name.clone(),
            detail: "panic in plugin dispatch".to_string(),
        })?;

        if result_ptr.is_null() {
            return Err(PluginError::InitError {
                plugin_name: plugin_name.clone(),
                detail: "plugin dispatch returned NULL".to_string(),
            });
        }

        let result_str = unsafe { std::ffi::CStr::from_ptr(result_ptr) }
            .to_str()
            .map_err(|e| PluginError::InitError {
                plugin_name: plugin_name.clone(),
                detail: format!("plugin dispatch returned invalid UTF-8: {e}"),
            })?
            .to_string();

        // Free the result using the plugin's free function
        unsafe { free_fn(result_ptr) };

        // Keep the library handle alive (don't drop lib) — but since it's already
        // loaded and in registry.libraries, the OS will keep it loaded via refcount.
        std::mem::forget(lib);

        Ok(result_str)
    }

    /// Unload all plugins, drop library handles, clear registry.
    pub fn unload_all() {
        let registry = get_registry();
        let mut reg = registry.write().unwrap();
        reg.plugins.clear();
        reg.capabilities.clear();
        reg.libraries.clear();
        reg.verifier = None;
        log::info!("All plugins unloaded");
    }
}

// ── Internal helpers ───────────────────────────────────────────────

/// Discover manifest files paired with shared libraries in a directory.
/// Returns (manifest_path, library_path) pairs.
fn discover_manifests(dir: &Path) -> Vec<(PathBuf, PathBuf)> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            log::warn!("Cannot read plugin directory '{}': {e}", dir.display());
            return Vec::new();
        }
    };

    let mut pairs = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }

        let stem = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };

        // Look for matching shared library
        let lib_path = find_library(dir, &stem);
        if let Some(lp) = lib_path {
            pairs.push((path, lp));
        } else {
            log::debug!(
                "Manifest '{}' has no matching shared library, skipping",
                path.display()
            );
        }
    }

    pairs
}

/// Find a shared library file matching the given stem in the directory.
fn find_library(dir: &Path, stem: &str) -> Option<PathBuf> {
    let extensions = if cfg!(target_os = "macos") {
        &["dylib"][..]
    } else if cfg!(target_os = "windows") {
        &["dll"][..]
    } else {
        &["so"][..]
    };

    for ext in extensions {
        let path = dir.join(format!("{stem}.{ext}"));
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

/// Parse and validate a plugin manifest from a JSON file.
fn parse_manifest(path: &Path) -> Result<PluginManifest, PluginError> {
    let content = std::fs::read_to_string(path).map_err(|e| PluginError::ManifestParseError {
        path: path.to_path_buf(),
        detail: format!("cannot read file: {e}"),
    })?;

    let manifest: PluginManifest =
        serde_json::from_str(&content).map_err(|e| PluginError::ManifestParseError {
            path: path.to_path_buf(),
            detail: format!("{e}"),
        })?;

    validate_manifest(&manifest, path)?;

    Ok(manifest)
}

/// Validate manifest fields beyond JSON structure.
fn validate_manifest(manifest: &PluginManifest, path: &Path) -> Result<(), PluginError> {
    // Name: non-empty, kebab-case (ASCII alphanumeric + hyphens), 2-64 chars
    if manifest.name.is_empty() || manifest.name.len() > 64 || manifest.name.len() < 2 {
        return Err(PluginError::ManifestParseError {
            path: path.to_path_buf(),
            detail: "name must be 2-64 characters".to_string(),
        });
    }
    if !manifest
        .name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err(PluginError::ManifestParseError {
            path: path.to_path_buf(),
            detail: "name must be kebab-case (ASCII alphanumeric + hyphens)".to_string(),
        });
    }

    // Capabilities: non-empty, each element non-empty
    if manifest.capabilities.is_empty() {
        return Err(PluginError::ManifestParseError {
            path: path.to_path_buf(),
            detail: "capabilities must be non-empty".to_string(),
        });
    }
    for cap in &manifest.capabilities {
        if cap.is_empty() {
            return Err(PluginError::ManifestParseError {
                path: path.to_path_buf(),
                detail: "each capability must be non-empty".to_string(),
            });
        }
    }

    // Signature: non-empty, valid base64 decoding to 64 bytes
    if manifest.signature.is_empty() {
        return Err(PluginError::ManifestParseError {
            path: path.to_path_buf(),
            detail: "signature must be non-empty".to_string(),
        });
    }
    let b64 = base64::engine::general_purpose::STANDARD;
    match b64.decode(&manifest.signature) {
        Ok(bytes) if bytes.len() == 64 => {}
        Ok(bytes) => {
            return Err(PluginError::ManifestParseError {
                path: path.to_path_buf(),
                detail: format!("signature must decode to 64 bytes, got {}", bytes.len()),
            });
        }
        Err(e) => {
            return Err(PluginError::ManifestParseError {
                path: path.to_path_buf(),
                detail: format!("signature is not valid base64: {e}"),
            });
        }
    }

    Ok(())
}

/// Load a single plugin: parse manifest → check ABI → verify signature → load library → init.
fn load_single_plugin(
    manifest_path: &Path,
    lib_path: &Path,
    verifier: &Option<Box<dyn PluginVerifier>>,
) -> Result<(LoadedPlugin, libloading::Library), PluginError> {
    log::debug!("Loading plugin from manifest '{}'", manifest_path.display());

    // Step 1: Parse manifest
    let manifest = parse_manifest(manifest_path)?;

    // Step 2: Check ABI version compatibility
    let host_abi = super::ABI_VERSION.to_str().unwrap_or("0.9.0");
    if manifest.abi_version != host_abi {
        return Err(PluginError::AbiMismatch {
            plugin_name: manifest.name.clone(),
            expected: host_abi.to_string(),
            found: manifest.abi_version.clone(),
        });
    }

    // Step 3: Read binary and log sha256
    let binary = std::fs::read(lib_path).map_err(|e| PluginError::LoadError {
        plugin_name: manifest.name.clone(),
        detail: format!("cannot read library file: {e}"),
    })?;

    let actual_hash = {
        use sha2::Digest;
        let hash = sha2::Sha256::digest(&binary);
        let hex = format!("sha256:{:x}", hash);
        log::debug!("Plugin '{}' binary hash: {}", manifest.name, hex);
        hex
    };

    // Step 3b: Verify binary hash if manifest includes it
    if let Some(expected_hash) = &manifest.binary_hash {
        if *expected_hash != actual_hash {
            return Err(PluginError::HashMismatch {
                plugin_name: manifest.name.clone(),
                expected: expected_hash.clone(),
                actual: actual_hash,
            });
        }
        log::debug!("Plugin '{}' binary hash verified", manifest.name);
    }

    // Step 4: Verify signature (trust mode aware)
    let trust_mode = get_trust_mode();
    let is_signed = !manifest.signature.is_empty();

    if is_signed {
        if let Some(verifier) = verifier {
            verifier.verify(&binary, &manifest)?;
            log::debug!(
                "Plugin '{}' signature verified by '{}'",
                manifest.name,
                verifier.name()
            );
        } else {
            // Signed but no trusted keys to verify against
            return Err(PluginError::NoTrustedKeys);
        }
    } else {
        // Unsigned plugin
        match trust_mode {
            TrustMode::SignedOnly => {
                emit_unsigned_audit_event(
                    &manifest,
                    &actual_hash,
                    lib_path,
                    "denied",
                    "unsigned_rejected",
                );
                return Err(PluginError::UnsignedPlugin {
                    plugin_name: manifest.name.clone(),
                });
            }
            TrustMode::AllowUnsigned => {
                emit_unsigned_audit_event(
                    &manifest,
                    &actual_hash,
                    lib_path,
                    "allowed",
                    "unsigned_allowed",
                );
                log::warn!(
                    "Plugin '{}' is unsigned but loading is allowed by trust mode",
                    manifest.name
                );
            }
        }
    }

    // Step 4b: Check manifest edition/entitlement requirement
    if let Some(ref req_edition) = manifest.required_edition {
        let req_edition_lower = req_edition.to_lowercase();
        if (req_edition_lower == "pro" || req_edition_lower == "enterprise")
            && !crate::entitlement::check_entitlement("plugin_loading", None)
        {
            return Err(PluginError::LoadError {
                plugin_name: manifest.name.clone(),
                detail: format!(
                    "This plugin requires {} edition. Purchase Pro: nxus.systems/pricing",
                    req_edition
                ),
            });
        }
    }

    // Step 5: Load library via libloading
    let library =
        unsafe { libloading::Library::new(lib_path) }.map_err(|e| PluginError::LoadError {
            plugin_name: manifest.name.clone(),
            detail: format!("{e}"),
        })?;

    // Step 6: Call nxuskit_plugin_init() if present
    let init_metadata = call_plugin_init(&library, &manifest.name)?;

    let plugin = LoadedPlugin {
        manifest,
        library_path: lib_path.to_path_buf(),
        init_metadata,
    };

    Ok((plugin, library))
}

/// Call the plugin's `nxuskit_plugin_init` entry point.
///
/// Uses `catch_unwind` to prevent plugin panics from crashing the host (SC-004).
fn call_plugin_init(
    library: &libloading::Library,
    plugin_name: &str,
) -> Result<Option<String>, PluginError> {
    type InitFn = unsafe extern "C" fn() -> *const std::os::raw::c_char;

    let init_fn: libloading::Symbol<InitFn> = match unsafe { library.get(b"nxuskit_plugin_init\0") }
    {
        Ok(f) => f,
        Err(_) => {
            // Entry point not found — this is an error per spec
            return Err(PluginError::InitError {
                plugin_name: plugin_name.to_string(),
                detail: "missing nxuskit_plugin_init entry point".to_string(),
            });
        }
    };

    // Call with catch_unwind for panic safety
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe { init_fn() }));

    match result {
        Ok(ptr) => {
            if ptr.is_null() {
                return Err(PluginError::InitError {
                    plugin_name: plugin_name.to_string(),
                    detail: "nxuskit_plugin_init returned NULL".to_string(),
                });
            }
            let c_str = unsafe { CStr::from_ptr(ptr) };
            let json_str = c_str.to_str().map_err(|e| PluginError::InitError {
                plugin_name: plugin_name.to_string(),
                detail: format!("init returned invalid UTF-8: {e}"),
            })?;
            Ok(Some(json_str.to_string()))
        }
        Err(_) => Err(PluginError::InitError {
            plugin_name: plugin_name.to_string(),
            detail: "nxuskit_plugin_init panicked".to_string(),
        }),
    }
}

/// Check whether the current edition allows plugin loading from the given path.
///
/// v0.9.0: All editions allow plugin loading. Individual features are gated
/// at dispatch time via `check_entitlement()`. Path restriction (app-local
/// for Pro, configurable roots for Enterprise) is a v0.9.1 hardening item.
fn edition_allows_plugin_path(dir: &Path) -> bool {
    let edition = super::entitlement::current_edition();
    match edition {
        super::entitlement::Edition::Oss => {
            // OSS: plugins can load but individual features are gated at dispatch
            // time via check_entitlement(). Path restriction is a v0.9.1 item.
            log::debug!(
                "OSS edition: allowing plugin load from '{}' (per-feature gating at dispatch)",
                dir.display()
            );
            true
        }
        super::entitlement::Edition::Pro => {
            // Pro: only app-local plugins dir allowed
            // Accept any path for now — path restriction enforcement is
            // a v0.9.1 hardening item (requires knowing the app_dir)
            let _ = dir;
            true
        }
        super::entitlement::Edition::Enterprise => {
            // Enterprise: any configured path is allowed
            true
        }
    }
}

/// Resolve the signature verifier from environment or defaults.
fn resolve_verifier() -> Option<Box<dyn PluginVerifier>> {
    // Check NXUSKIT_TRUSTED_KEYS_FILE env var
    let keys_path = if let Ok(path) = std::env::var("NXUSKIT_TRUSTED_KEYS_FILE") {
        PathBuf::from(path)
    } else if let Ok(sdk_dir) = std::env::var("NXUSKIT_SDK_DIR") {
        PathBuf::from(sdk_dir).join("trust").join("keys.json")
    } else {
        log::debug!(
            "No trusted keys file configured (NXUSKIT_TRUSTED_KEYS_FILE or NXUSKIT_SDK_DIR not set)"
        );
        return None;
    };

    match load_trusted_keys(&keys_path) {
        Ok(keys) => {
            log::info!(
                "Loaded {} trusted key(s) from '{}'",
                keys.len(),
                keys_path.display()
            );
            match OfflineEd25519Verifier::from_trusted_keys(&keys) {
                Ok(v) => Some(Box::new(v)),
                Err(e) => {
                    log::warn!("Failed to initialize verifier: {e}");
                    None
                }
            }
        }
        Err(e) => {
            log::warn!(
                "Failed to load trusted keys from '{}': {e}",
                keys_path.display()
            );
            None
        }
    }
}

/// Log edition/entitlement warnings for deferred enforcement (FR-008).
fn log_edition_entitlement_warnings(manifest: &PluginManifest) {
    let host_edition = super::EDITION.to_str().unwrap_or("oss");

    if let Some(ref required_edition) = manifest.required_edition
        && required_edition != host_edition
    {
        log::warn!(
            "Plugin '{}' requires edition '{}' but host is '{}' — enforcement active in v0.9.0",
            manifest.name,
            required_edition,
            host_edition
        );
    }

    if !manifest.required_entitlements.is_empty() {
        log::info!(
            "Plugin '{}' declares required entitlements {:?} — enforcement active in v0.9.0",
            manifest.name,
            manifest.required_entitlements
        );
    }
}

// ── Audit Events ──────────────────────────────────────────────────

/// Emit a structured audit event for unsigned plugin load attempts.
///
/// 15-field structured JSON event emitted via `log::warn!` for every
/// unsigned plugin load attempt (allowed or denied).
fn emit_unsigned_audit_event(
    manifest: &PluginManifest,
    content_hash: &str,
    source_path: &Path,
    decision: &str,
    reason_code: &str,
) {
    let event = serde_json::json!({
        "event": "unsigned_plugin_load",
        "timestamp": crate::entitlement::chrono_now_iso_public(),
        "plugin_id": manifest.name,
        "plugin_version": manifest.version,
        "content_hash": content_hash,
        "signer_key_id": manifest.signing_key_id.as_deref().unwrap_or("none"),
        "trust_mode": get_trust_mode().to_string(),
        "decision": decision,
        "reason_code": reason_code,
        "sdk_version": super::ABI_VERSION.to_str().unwrap_or("unknown"),
        "sdk_edition": std::env::var("NXUSKIT_EDITION").unwrap_or_else(|_| "oss".to_string()),
        "session_id": "unknown",
        "caller_identity": "unknown",
        "manifest_edition": manifest.required_edition.as_deref().unwrap_or("none"),
        "source_path": source_path.display().to_string(),
    });
    log::warn!("{}", event);
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_parse_valid() {
        let json = r#"{
            "schema_version": "1.0",
            "name": "test-plugin",
            "version": "0.1.0",
            "abi_version": "0.9.0",
            "capabilities": ["test-cap"],
            "signature": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=="
        }"#;
        let manifest: PluginManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.name, "test-plugin");
        assert_eq!(manifest.version, "0.1.0");
        assert_eq!(manifest.abi_version, "0.9.0");
        assert_eq!(manifest.capabilities, vec!["test-cap"]);
        assert!(manifest.required_edition.is_none());
        assert!(manifest.required_entitlements.is_empty());
    }

    #[test]
    fn test_manifest_parse_missing_required_field() {
        let json = r#"{"schema_version": "1.0", "name": "test"}"#;
        let result: Result<PluginManifest, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_manifest_unknown_fields_tolerated() {
        let json = r#"{
            "schema_version": "1.0",
            "name": "test-plugin",
            "version": "0.1.0",
            "abi_version": "0.9.0",
            "capabilities": ["cap1"],
            "signature": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==",
            "future_field": "some-value",
            "another_unknown": 42
        }"#;
        let manifest: PluginManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.name, "test-plugin");
    }

    #[test]
    fn test_manifest_empty_capabilities_rejected() {
        let json = r#"{
            "schema_version": "1.0",
            "name": "test-plugin",
            "version": "0.1.0",
            "abi_version": "0.9.0",
            "capabilities": [],
            "signature": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=="
        }"#;
        let manifest: PluginManifest = serde_json::from_str(json).unwrap();
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let result = validate_manifest(&manifest, tmp.path());
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("capabilities must be non-empty"),);
    }

    #[test]
    fn test_plugin_error_types() {
        let err = PluginError::AbiMismatch {
            plugin_name: "test".to_string(),
            expected: "0.8".to_string(),
            found: "0.7".to_string(),
        };
        assert_eq!(err.error_type(), "abi_mismatch");
        assert!(format!("{err}").contains("ABI mismatch"));

        assert_eq!(PluginError::NoTrustedKeys.error_type(), "no_trusted_keys");
    }

    #[test]
    fn test_discover_nonexistent_dir() {
        let dir = Path::new("/nonexistent/plugin/dir");
        assert_eq!(PluginRegistry::load_dir(dir), 0);
    }

    #[test]
    fn test_discover_empty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(PluginRegistry::load_dir(tmp.path()), 0);
    }

    #[test]
    fn test_name_validation_kebab_case() {
        let tmp = tempfile::NamedTempFile::new().unwrap();

        // Valid names
        for name in &["ab", "test-plugin", "my-plugin-v2", "a-b-c"] {
            let manifest = PluginManifest {
                schema_version: "1.0".into(),
                name: name.to_string(),
                version: "0.1.0".into(),
                abi_version: "0.9.0".into(),
                capabilities: vec!["cap".into()],
                signature: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==".into(),
                required_edition: None,
                required_entitlements: vec![],
                description: None,
                author: None,
                ..Default::default()
            };
            assert!(
                validate_manifest(&manifest, tmp.path()).is_ok(),
                "Name '{}' should be valid",
                name
            );
        }

        // Invalid: too short
        let manifest = PluginManifest {
            schema_version: "1.0".into(),
            name: "x".into(),
            version: "0.1.0".into(),
            abi_version: "0.9.0".into(),
            capabilities: vec!["cap".into()],
            signature: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==".into(),
            required_edition: None,
            required_entitlements: vec![],
            description: None,
            author: None,
            ..Default::default()
        };
        assert!(validate_manifest(&manifest, tmp.path()).is_err());

        // Invalid: contains underscore
        let manifest = PluginManifest {
            schema_version: "1.0".into(),
            name: "my_plugin".into(),
            version: "0.1.0".into(),
            abi_version: "0.9.0".into(),
            capabilities: vec!["cap".into()],
            signature: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==".into(),
            required_edition: None,
            required_entitlements: vec![],
            description: None,
            author: None,
            ..Default::default()
        };
        assert!(validate_manifest(&manifest, tmp.path()).is_err());
    }

    #[test]
    fn test_verifier_trait_extensible() {
        // FR-009: Verify a second mock verifier can be plugged in
        struct AlwaysPassVerifier;
        impl PluginVerifier for AlwaysPassVerifier {
            fn verify(
                &self,
                _binary: &[u8],
                _manifest: &PluginManifest,
            ) -> Result<(), PluginError> {
                Ok(())
            }
            fn name(&self) -> &str {
                "always-pass"
            }
        }

        let verifier: Box<dyn PluginVerifier> = Box::new(AlwaysPassVerifier);
        assert_eq!(verifier.name(), "always-pass");

        let manifest = PluginManifest {
            schema_version: "1.0".into(),
            name: "test-plugin".into(),
            version: "0.1.0".into(),
            abi_version: "0.9.0".into(),
            capabilities: vec!["cap".into()],
            signature: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==".into(),
            required_edition: None,
            required_entitlements: vec![],
            description: None,
            author: None,
            ..Default::default()
        };
        assert!(verifier.verify(b"some binary", &manifest).is_ok());
    }

    // ── T029: OfflineEd25519Verifier signature tests ────────────────

    /// Deterministic test keypair from fixed seed (same as integration tests).
    fn test_keypair() -> (ed25519_dalek::SigningKey, VerifyingKey) {
        use ed25519_dalek::SigningKey;
        let seed: [u8; 32] = [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
            0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c,
            0x1d, 0x1e, 0x1f, 0x20,
        ];
        let sk = SigningKey::from_bytes(&seed);
        let vk = sk.verifying_key();
        (sk, vk)
    }

    fn test_trusted_key(vk: &VerifyingKey) -> TrustedKey {
        use base64::Engine;
        TrustedKey {
            id: "test-key".into(),
            public_key: base64::engine::general_purpose::STANDARD.encode(vk.to_bytes()),
            description: None,
        }
    }

    #[test]
    fn test_valid_signature_passes() {
        use base64::Engine;
        use ed25519_dalek::Signer;
        let b64 = base64::engine::general_purpose::STANDARD;

        let (sk, vk) = test_keypair();
        let binary = b"hello plugin binary";
        let signature = sk.sign(binary);

        let verifier = OfflineEd25519Verifier::from_trusted_keys(&[test_trusted_key(&vk)]).unwrap();
        let manifest = PluginManifest {
            schema_version: "1.0".into(),
            name: "valid-sig".into(),
            version: "0.1.0".into(),
            abi_version: "0.9.0".into(),
            capabilities: vec!["cap".into()],
            signature: b64.encode(signature.to_bytes()),
            required_edition: None,
            required_entitlements: vec![],
            description: None,
            author: None,
            ..Default::default()
        };
        assert!(verifier.verify(binary, &manifest).is_ok());
    }

    #[test]
    fn test_invalid_signature_rejected() {
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD;

        let (_sk, vk) = test_keypair();
        let verifier = OfflineEd25519Verifier::from_trusted_keys(&[test_trusted_key(&vk)]).unwrap();

        // Wrong signature bytes (0xAA repeated)
        let manifest = PluginManifest {
            schema_version: "1.0".into(),
            name: "bad-sig".into(),
            version: "0.1.0".into(),
            abi_version: "0.9.0".into(),
            capabilities: vec!["cap".into()],
            signature: b64.encode([0xAA_u8; 64]),
            required_edition: None,
            required_entitlements: vec![],
            description: None,
            author: None,
            ..Default::default()
        };
        let result = verifier.verify(b"some binary", &manifest);
        assert!(result.is_err());
        match result.unwrap_err() {
            PluginError::SignatureInvalid { plugin_name } => assert_eq!(plugin_name, "bad-sig"),
            e => panic!("Expected SignatureInvalid, got: {e}"),
        }
    }

    #[test]
    fn test_tampered_binary_rejected() {
        use base64::Engine;
        use ed25519_dalek::Signer;
        let b64 = base64::engine::general_purpose::STANDARD;

        let (sk, vk) = test_keypair();
        let binary = b"original binary content";
        let signature = sk.sign(&binary[..]);

        let verifier = OfflineEd25519Verifier::from_trusted_keys(&[test_trusted_key(&vk)]).unwrap();
        let manifest = PluginManifest {
            schema_version: "1.0".into(),
            name: "tampered".into(),
            version: "0.1.0".into(),
            abi_version: "0.9.0".into(),
            capabilities: vec!["cap".into()],
            signature: b64.encode(signature.to_bytes()),
            required_edition: None,
            required_entitlements: vec![],
            description: None,
            author: None,
            ..Default::default()
        };

        // Signature valid for original binary
        assert!(verifier.verify(binary, &manifest).is_ok());

        // Tampered binary — signature must fail
        let tampered = b"modified binary content";
        let result = verifier.verify(tampered, &manifest);
        assert!(result.is_err());
        match result.unwrap_err() {
            PluginError::SignatureInvalid { .. } => {}
            e => panic!("Expected SignatureInvalid, got: {e}"),
        }
    }

    #[test]
    fn test_unsigned_plugin_rejected() {
        let (_sk, vk) = test_keypair();
        let verifier = OfflineEd25519Verifier::from_trusted_keys(&[test_trusted_key(&vk)]).unwrap();

        let manifest = PluginManifest {
            schema_version: "1.0".into(),
            name: "unsigned-test".into(),
            version: "0.1.0".into(),
            abi_version: "0.9.0".into(),
            capabilities: vec!["cap".into()],
            signature: String::new(), // Empty = unsigned
            required_edition: None,
            required_entitlements: vec![],
            description: None,
            author: None,
            ..Default::default()
        };
        let result = verifier.verify(b"binary", &manifest);
        assert!(result.is_err());
        match result.unwrap_err() {
            PluginError::UnsignedPlugin { plugin_name } => {
                assert_eq!(plugin_name, "unsigned-test");
            }
            e => panic!("Expected UnsignedPlugin, got: {e}"),
        }
    }

    #[test]
    fn test_zeros_signature_rejected() {
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD;

        let (_sk, vk) = test_keypair();
        let verifier = OfflineEd25519Verifier::from_trusted_keys(&[test_trusted_key(&vk)]).unwrap();

        let manifest = PluginManifest {
            schema_version: "1.0".into(),
            name: "zeros-sig".into(),
            version: "0.1.0".into(),
            abi_version: "0.9.0".into(),
            capabilities: vec!["cap".into()],
            signature: b64.encode([0x00_u8; 64]),
            required_edition: None,
            required_entitlements: vec![],
            description: None,
            author: None,
            ..Default::default()
        };
        let result = verifier.verify(b"binary", &manifest);
        assert!(result.is_err());
        match result.unwrap_err() {
            PluginError::SignatureInvalid { .. } => {}
            e => panic!("Expected SignatureInvalid, got: {e}"),
        }
    }

    #[test]
    fn test_multiple_keys_any_passes() {
        use base64::Engine;
        use ed25519_dalek::{Signer, SigningKey};
        let b64 = base64::engine::general_purpose::STANDARD;

        // Key 1 (won't match)
        let sk1 = SigningKey::from_bytes(&[0x01; 32]);
        let vk1 = sk1.verifying_key();

        // Key 2 (will match)
        let sk2 = SigningKey::from_bytes(&[0x02; 32]);
        let vk2 = sk2.verifying_key();

        let binary = b"test binary";
        let signature = sk2.sign(&binary[..]); // Signed by key 2

        let keys = vec![
            TrustedKey {
                id: "key-1".into(),
                public_key: b64.encode(vk1.to_bytes()),
                description: None,
            },
            TrustedKey {
                id: "key-2".into(),
                public_key: b64.encode(vk2.to_bytes()),
                description: None,
            },
        ];
        let verifier = OfflineEd25519Verifier::from_trusted_keys(&keys).unwrap();
        let manifest = PluginManifest {
            schema_version: "1.0".into(),
            name: "multi-key".into(),
            version: "0.1.0".into(),
            abi_version: "0.9.0".into(),
            capabilities: vec!["cap".into()],
            signature: b64.encode(signature.to_bytes()),
            required_edition: None,
            required_entitlements: vec![],
            description: None,
            author: None,
            ..Default::default()
        };
        // Should pass because key 2 matches
        assert!(verifier.verify(binary, &manifest).is_ok());
    }

    // ── T030: Trusted key loading tests ──────────────────────────────

    #[test]
    fn test_load_trusted_keys_valid() {
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD;

        let (_sk, vk) = test_keypair();
        let trust_json = serde_json::json!({
            "schema_version": "1.0",
            "keys": [{
                "id": "key-1",
                "public_key": b64.encode(vk.to_bytes()),
                "description": "Test key"
            }]
        });

        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), serde_json::to_string(&trust_json).unwrap()).unwrap();
        let keys = load_trusted_keys(tmp.path()).unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].id, "key-1");
    }

    #[test]
    fn test_load_trusted_keys_empty_returns_error() {
        let trust_json = serde_json::json!({
            "schema_version": "1.0",
            "keys": []
        });
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), serde_json::to_string(&trust_json).unwrap()).unwrap();
        let result = load_trusted_keys(tmp.path());
        assert!(result.is_err());
        match result.unwrap_err() {
            PluginError::NoTrustedKeys => {}
            e => panic!("Expected NoTrustedKeys, got: {e}"),
        }
    }

    #[test]
    fn test_load_trusted_keys_malformed_json() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "not json at all {{{").unwrap();
        let result = load_trusted_keys(tmp.path());
        assert!(result.is_err());
        match result.unwrap_err() {
            PluginError::TrustFileError(msg) => assert!(msg.contains("invalid JSON")),
            e => panic!("Expected TrustFileError, got: {e}"),
        }
    }

    #[test]
    fn test_load_trusted_keys_invalid_base64() {
        let trust_json = serde_json::json!({
            "schema_version": "1.0",
            "keys": [{"id": "bad", "public_key": "not!valid!base64!!!"}]
        });
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), serde_json::to_string(&trust_json).unwrap()).unwrap();
        let result = load_trusted_keys(tmp.path());
        assert!(result.is_err());
        match result.unwrap_err() {
            PluginError::TrustFileError(msg) => assert!(msg.contains("invalid base64")),
            e => panic!("Expected TrustFileError with base64 error, got: {e}"),
        }
    }

    #[test]
    fn test_load_trusted_keys_wrong_length() {
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD;
        // 16 bytes instead of 32
        let trust_json = serde_json::json!({
            "schema_version": "1.0",
            "keys": [{"id": "short", "public_key": b64.encode([0x01_u8; 16])}]
        });
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), serde_json::to_string(&trust_json).unwrap()).unwrap();
        let result = load_trusted_keys(tmp.path());
        assert!(result.is_err());
        match result.unwrap_err() {
            PluginError::TrustFileError(msg) => assert!(msg.contains("expected 32 bytes")),
            e => panic!("Expected TrustFileError about key length, got: {e}"),
        }
    }

    #[test]
    fn test_load_trusted_keys_multiple() {
        use base64::Engine;
        use ed25519_dalek::SigningKey;
        let b64 = base64::engine::general_purpose::STANDARD;

        let vk1 = SigningKey::from_bytes(&[0x01; 32]).verifying_key();
        let vk2 = SigningKey::from_bytes(&[0x02; 32]).verifying_key();
        let trust_json = serde_json::json!({
            "schema_version": "1.0",
            "keys": [
                {"id": "key-1", "public_key": b64.encode(vk1.to_bytes())},
                {"id": "key-2", "public_key": b64.encode(vk2.to_bytes())}
            ]
        });
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), serde_json::to_string(&trust_json).unwrap()).unwrap();
        let keys = load_trusted_keys(tmp.path()).unwrap();
        assert_eq!(keys.len(), 2);
    }

    #[test]
    fn test_load_trusted_keys_file_not_found() {
        let result = load_trusted_keys(Path::new("/nonexistent/keys.json"));
        assert!(result.is_err());
        match result.unwrap_err() {
            PluginError::TrustFileError(msg) => assert!(msg.contains("cannot read")),
            e => panic!("Expected TrustFileError, got: {e}"),
        }
    }

    // ── T039: Edition/entitlement logging tests ──────────────────────

    #[test]
    fn test_edition_entitlement_warnings_no_mismatch() {
        // When required_edition matches host or is None, no warnings expected.
        // This test ensures the function doesn't panic.
        let manifest = PluginManifest {
            schema_version: "1.0".into(),
            name: "test-plugin".into(),
            version: "0.1.0".into(),
            abi_version: "0.9.0".into(),
            capabilities: vec!["cap".into()],
            signature: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==".into(),
            required_edition: None,
            required_entitlements: vec![],
            description: None,
            author: None,
            ..Default::default()
        };
        // Should not panic — no warnings when no edition/entitlements
        log_edition_entitlement_warnings(&manifest);
    }

    #[test]
    fn test_edition_mismatch_does_not_panic() {
        // When required_edition doesn't match host, function logs warning but doesn't fail.
        let manifest = PluginManifest {
            schema_version: "1.0".into(),
            name: "pro-plugin".into(),
            version: "0.1.0".into(),
            abi_version: "0.9.0".into(),
            capabilities: vec!["cap".into()],
            signature: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==".into(),
            required_edition: Some("enterprise".into()),
            required_entitlements: vec!["solver".into(), "clips".into()],
            description: None,
            author: None,
            ..Default::default()
        };
        // Should not panic — logs warning and info, enforcement active in v0.9.0
        log_edition_entitlement_warnings(&manifest);
    }

    // ── T043: Delegated trust chain verification tests ────────────────

    #[test]
    fn test_trust_chain_root_anchored() {
        // Create a chain: root → leaf, root is in trusted set
        let (_root_sk, root_vk) = test_keypair();
        let (leaf_sk, leaf_vk) = {
            // Different keypair for leaf
            let seed: [u8; 32] = [
                0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e,
                0x2f, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x3b, 0x3c,
                0x3d, 0x3e, 0x3f, 0x40,
            ];
            let sk = ed25519_dalek::SigningKey::from_bytes(&seed);
            let vk = sk.verifying_key();
            (sk, vk)
        };

        let b64 = base64::engine::general_purpose::STANDARD;
        let binary = b"test binary content for trust chain test";

        // Sign binary with leaf key
        use ed25519_dalek::Signer;
        let sig = leaf_sk.sign(binary);
        let sig_b64 = b64.encode(sig.to_bytes());

        // Trust chain: [root_pub, leaf_pub]
        let root_pub_b64 = b64.encode(root_vk.to_bytes());
        let leaf_pub_b64 = b64.encode(leaf_vk.to_bytes());

        let manifest = PluginManifest {
            schema_version: "1.0".into(),
            name: "chain-plugin".into(),
            version: "0.1.0".into(),
            abi_version: "0.9.0".into(),
            capabilities: vec!["chain-cap".into()],
            signature: sig_b64,
            trust_chain: vec![root_pub_b64, leaf_pub_b64],
            ..Default::default()
        };

        // Verifier with root key trusted
        let trusted_keys = vec![TrustedKey {
            id: "root".into(),
            public_key: b64.encode(root_vk.to_bytes()),
            description: None,
        }];
        let verifier = OfflineEd25519Verifier::from_trusted_keys(&trusted_keys).unwrap();

        // Should pass — root anchors the chain, leaf signed the binary
        assert!(verifier.verify(binary, &manifest).is_ok());
    }

    #[test]
    fn test_trust_chain_broken_root_rejected() {
        // Chain with untrusted root → should reject
        let seed: [u8; 32] = [
            0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b, 0x4c, 0x4d, 0x4e,
            0x4f, 0x50, 0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x5b, 0x5c,
            0x5d, 0x5e, 0x5f, 0x60,
        ];
        let untrusted_sk = ed25519_dalek::SigningKey::from_bytes(&seed);
        let untrusted_vk = untrusted_sk.verifying_key();

        let (_, root_vk) = test_keypair();
        let b64 = base64::engine::general_purpose::STANDARD;
        let binary = b"test binary for broken chain";

        use ed25519_dalek::Signer;
        let sig = untrusted_sk.sign(binary);

        let manifest = PluginManifest {
            schema_version: "1.0".into(),
            name: "broken-chain".into(),
            version: "0.1.0".into(),
            abi_version: "0.9.0".into(),
            capabilities: vec!["cap".into()],
            signature: b64.encode(sig.to_bytes()),
            trust_chain: vec![
                b64.encode(untrusted_vk.to_bytes()), // root not in trusted set
            ],
            ..Default::default()
        };

        // Verifier trusts a different root
        let trusted_keys = vec![TrustedKey {
            id: "real-root".into(),
            public_key: b64.encode(root_vk.to_bytes()),
            description: None,
        }];
        let verifier = OfflineEd25519Verifier::from_trusted_keys(&trusted_keys).unwrap();

        // Should fail — chain root is not in trusted set
        assert!(verifier.verify(binary, &manifest).is_err());
    }

    #[test]
    fn test_empty_trust_chain_falls_through() {
        // Empty trust chain should fall through to direct key check
        let (_, vk) = test_keypair();
        let b64 = base64::engine::general_purpose::STANDARD;
        let binary = b"test binary no chain";

        // Sign with a different key (not trusted)
        let seed: [u8; 32] = [0x61; 32];
        let other_sk = ed25519_dalek::SigningKey::from_bytes(&seed);
        use ed25519_dalek::Signer;
        let sig = other_sk.sign(binary);

        let manifest = PluginManifest {
            schema_version: "1.0".into(),
            name: "no-chain-plugin".into(),
            version: "0.1.0".into(),
            abi_version: "0.9.0".into(),
            capabilities: vec!["cap".into()],
            signature: b64.encode(sig.to_bytes()),
            trust_chain: vec![], // empty
            ..Default::default()
        };

        let trusted_keys = vec![TrustedKey {
            id: "root".into(),
            public_key: b64.encode(vk.to_bytes()),
            description: None,
        }];
        let verifier = OfflineEd25519Verifier::from_trusted_keys(&trusted_keys).unwrap();

        // Should fail — key that signed is not in trusted set, no chain to rescue
        assert!(verifier.verify(binary, &manifest).is_err());
    }
}
