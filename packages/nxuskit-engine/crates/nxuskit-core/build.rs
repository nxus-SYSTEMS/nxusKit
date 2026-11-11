// Build script — panics are intentional for fail-fast at compile time.
#![allow(clippy::panic)]

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};

// ── Product catalog YAML schema (build-time only) ────────────────────

/// Top-level catalog file. Extra fields (schema_version, last_updated, global)
/// are silently ignored by serde's default behavior.
#[derive(Deserialize)]
struct CatalogFile {
    #[serde(default)]
    products: HashMap<String, Product>,
}

/// Product definition. Extra fields (display_name, description, status)
/// are silently ignored.
#[derive(Deserialize)]
struct Product {
    #[serde(default)]
    editions: HashMap<String, EditionDef>,
}

/// Edition definition. Only `features`, `limits`, and `inherits` are extracted.
/// All other fields (trial_allowed, trial_edition, grace_period_days, status,
/// display_name, description, price_model) are silently ignored.
#[derive(Deserialize)]
struct EditionDef {
    #[serde(default)]
    features: Vec<String>,
    #[serde(default)]
    limits: HashMap<String, serde_yaml_ng::Value>,
    #[serde(default)]
    inherits: Option<String>,
}

/// Resolve the product catalog YAML path, if available.
///
/// Public CE builds do not import private product catalogs. The fallback catalog
/// below is intentionally community-only.
fn find_catalog_path(crate_dir: &Path) -> Option<PathBuf> {
    let _ = crate_dir;
    None
}

/// Parse a YAML limit value to an Option<u64> Rust literal.
fn limit_to_rust(val: &serde_yaml_ng::Value) -> String {
    match val {
        serde_yaml_ng::Value::Number(n, _) => {
            if let Some(u) = n.as_u64() {
                format!("Some({u})")
            } else {
                "None".to_string()
            }
        }
        serde_yaml_ng::Value::String(s, _) if s == "unlimited" => "None".to_string(),
        _ => "None".to_string(),
    }
}

/// Resolve edition inheritance, returning flattened feature lists.
fn resolve_features(editions: &HashMap<String, EditionDef>) -> HashMap<String, Vec<String>> {
    let mut resolved: HashMap<String, Vec<String>> = HashMap::new();

    fn resolve_one(
        name: &str,
        editions: &HashMap<String, EditionDef>,
        resolved: &mut HashMap<String, Vec<String>>,
    ) -> Vec<String> {
        if let Some(cached) = resolved.get(name) {
            return cached.clone();
        }
        let ed = &editions[name];
        let mut features = if let Some(ref parent) = ed.inherits {
            resolve_one(parent, editions, resolved)
        } else {
            Vec::new()
        };
        for f in &ed.features {
            if !features.contains(f) {
                features.push(f.clone());
            }
        }
        resolved.insert(name.to_string(), features.clone());
        features
    }

    for name in editions.keys() {
        resolve_one(name, editions, &mut resolved);
    }
    resolved
}

/// Generate Rust source code for the catalog.
fn generate_catalog_code(_catalog_path: &Path, out_dir: &Path) {
    generate_fallback_catalog_code(out_dir);
}

/// Generate hardcoded fallback catalog when the YAML file is not available.
///
/// Public CE fallback data is community-only. Pro and Enterprise metadata are
/// intentionally not embedded in public CE source builds.
fn generate_fallback_catalog_code(out_dir: &Path) {
    let code = r#"// Auto-generated public CE fallback catalog

/// Return the feature list for the given edition (inheritance resolved).
pub fn catalog_features(edition: &str) -> &'static [&'static str] {
    match edition {
        "community" => &["llm_cloud", "llm_local", "clips", "bayesian", "auth", "tool_calling"],
        _ => &[],
    }
}

/// Return the numerical limits for the given edition.
pub fn catalog_limits(edition: &str) -> EditionLimits {
    match edition {
        "community" => EditionLimits {
            max_sessions: Some(16),
            max_cached_rulebases: Some(8),
            max_rules_per_session: Some(500),
            max_facts_per_session: Some(10000),
            max_bayesian_nodes: Some(50),
            max_solver_constraints: None,
            seats: None,
        },
        _ => EditionLimits::default(),
    }
}
"#;

    let out_file = out_dir.join("catalog_generated.rs");
    fs::write(&out_file, code).unwrap_or_else(|e| {
        panic!(
            "Failed to write generated catalog to {}: {e}",
            out_file.display()
        )
    });
}

/// Strip any `\r` bytes preceding `\n`, collapsing CRLF to LF.
///
/// Any lone `\r` not immediately followed by `\n` is left untouched, since
/// only checkout-introduced CRLF pairs (never present in the cbindgen AST
/// output itself) are the drift source being corrected here.
fn normalize_to_lf(bytes: &[u8]) -> Vec<u8> {
    let mut normalized = Vec::with_capacity(bytes.len());
    let mut iterator = bytes.iter().copied().peekable();
    while let Some(byte) = iterator.next() {
        if byte == b'\r' && iterator.peek() == Some(&b'\n') {
            continue;
        }
        normalized.push(byte);
    }
    normalized
}

/// Regenerate the header from a synthetically CRLF-mutated `cbindgen.toml`
/// and confirm the LF-normalized output is byte-identical to `expected`.
///
/// This is a deterministic, non-tautological regression check for the
/// Windows-only header drift: it reproduces the exact checkout condition
/// that caused `after_includes` bytes to diverge (a `-text`-unprotected
/// `cbindgen.toml` checked out with CRLF line endings) without requiring a
/// native Windows checkout to observe the drift.
fn assert_header_generation_is_crlf_invariant(crate_path: &Path, out_dir: &Path, expected: &[u8]) {
    let config_path = crate_path.join("cbindgen.toml");
    let config_bytes = fs::read(&config_path).unwrap_or_else(|error| {
        panic!(
            "Failed to read {} for CRLF-invariance check: {error}",
            config_path.display()
        )
    });
    // Inject `\r` before every bare `\n` (idempotent: an already-CRLF
    // config is left unchanged) to simulate a Windows CRLF checkout.
    let lf_only = normalize_to_lf(&config_bytes);
    let mut crlf_bytes = Vec::with_capacity(lf_only.len());
    for &byte in &lf_only {
        if byte == b'\n' {
            crlf_bytes.push(b'\r');
        }
        crlf_bytes.push(byte);
    }
    let crlf_config_path = out_dir.join("cbindgen.crlf-check.toml");
    fs::write(&crlf_config_path, &crlf_bytes).unwrap_or_else(|error| {
        panic!(
            "Failed to write CRLF-invariance probe config {}: {error}",
            crlf_config_path.display()
        )
    });

    let crlf_config = cbindgen::Config::from_file(&crlf_config_path)
        .unwrap_or_else(|error| panic!("Failed to parse CRLF-invariance probe config: {error}"));
    let bindings = cbindgen::Builder::new()
        .with_config(crlf_config)
        .with_src(crate_path.join("src").join("lib.rs"))
        .generate()
        .unwrap_or_else(|error| panic!("CRLF-invariance probe generation failed: {error}"));
    let mut generated = Vec::new();
    bindings.write(&mut generated);
    let normalized = normalize_to_lf(&generated);

    if normalized != expected {
        panic!(
            "generated header is not invariant to cbindgen.toml checkout line endings: \
             a CRLF-checked-out cbindgen.toml produced different normalized header bytes"
        );
    }
}

/// Write the generated header only when its bytes differ from the existing
/// file, mirroring `cbindgen::Bindings::write_to_file`'s change-detection
/// behavior for the post-normalization bytes.
fn write_generated_header(path: &Path, bytes: &[u8]) {
    if fs::read(path)
        .map(|existing| existing == bytes)
        .unwrap_or(false)
    {
        return;
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap_or_else(|e| {
            panic!(
                "Failed to create header directory {}: {e}",
                parent.display()
            )
        });
    }
    fs::write(path, bytes)
        .unwrap_or_else(|e| panic!("Failed to write generated header {}: {e}", path.display()));
}

const PRODUCTION_VERIFIER_BUNDLE_MANIFEST_SHA256: &str =
    "ac569d0499cd994728f20c5f68e61af167dc2229648b770460a4e694e9410931";
const PRODUCTION_VERIFIER_BUNDLE_SCHEMA_SHA256: &str =
    "fb95abeb2cc3f4af3433d4d39ff4453b1d6c70599ed18fc1b6d65a045006c50f";

#[derive(Deserialize)]
struct ProductionVerifierBundle {
    schema_version: String,
    bundle_id: String,
    keys: Vec<ProductionVerifierBundleKey>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductionVerifierBundleKey {
    kid: String,
    pem_path: String,
    pem_sha256: String,
    state: String,
    algorithm: String,
    protection_level: String,
}

fn infer_license_environment(license_server: &str) -> &'static str {
    let normalized = license_server.to_ascii_lowercase();
    if normalized.contains("localhost")
        || normalized.contains("127.0.0.1")
        || normalized.contains("dev.nxus.systems")
    {
        "development"
    } else if normalized.contains("staging") {
        "staging"
    } else {
        "production"
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn production_verifier_bundle_dir() -> PathBuf {
    PathBuf::from(
        env::var("CARGO_MANIFEST_DIR")
            .expect("CARGO_MANIFEST_DIR is required for verifier-bundle input"),
    )
    .join("resources/licensing")
}

fn generate_license_key_code(out_dir: &Path) {
    let bundle_dir = production_verifier_bundle_dir();
    let manifest_path = bundle_dir.join("production-verifier-bundle-v1.json");
    let schema_path = bundle_dir.join("production-verifier-bundle-v1.schema.json");
    let manifest_bytes = fs::read(&manifest_path).unwrap_or_else(|error| {
        panic!(
            "immutable production verifier-bundle manifest is required at {}: {error}",
            manifest_path.display()
        )
    });
    if sha256_hex(&manifest_bytes) != PRODUCTION_VERIFIER_BUNDLE_MANIFEST_SHA256 {
        panic!("immutable production verifier-bundle manifest hash mismatch");
    }
    let schema_bytes = fs::read(&schema_path).unwrap_or_else(|error| {
        panic!(
            "immutable production verifier-bundle schema is required at {}: {error}",
            schema_path.display()
        )
    });
    if sha256_hex(&schema_bytes) != PRODUCTION_VERIFIER_BUNDLE_SCHEMA_SHA256 {
        panic!("immutable production verifier-bundle schema hash mismatch");
    }
    let bundle: ProductionVerifierBundle =
        serde_json::from_slice(&manifest_bytes).unwrap_or_else(|error| {
            panic!("immutable production verifier-bundle must be JSON: {error}")
        });
    if bundle.schema_version != "nxus-licensing-production-verifier-bundle/v1"
        || bundle.bundle_id != "nxus-licensing-production-verifier-bundle-v1"
        || bundle.keys.is_empty()
    {
        panic!("immutable production verifier-bundle has an invalid identity or no keys");
    }

    let mut entries = Vec::with_capacity(bundle.keys.len());
    let mut seen_kids = std::collections::HashSet::with_capacity(bundle.keys.len());
    for key in bundle.keys {
        let relative_pem_path = Path::new(&key.pem_path);
        if key.kid.trim().is_empty()
            || relative_pem_path.is_absolute()
            || relative_pem_path
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
            || key.state != "ENABLED"
            || key.algorithm != "EC_SIGN_P256_SHA256"
            || key.protection_level != "SOFTWARE"
        {
            panic!("immutable production verifier-bundle contains an invalid key entry");
        }
        if !seen_kids.insert(key.kid.clone()) {
            panic!("immutable production verifier-bundle contains a duplicate exact kid");
        }
        let pem_path = bundle_dir.join(relative_pem_path);
        let pem = fs::read_to_string(&pem_path).unwrap_or_else(|error| {
            panic!(
                "immutable production verifier PEM is required at {}: {error}",
                pem_path.display()
            )
        });
        if sha256_hex(pem.as_bytes()) != key.pem_sha256 {
            panic!(
                "immutable production verifier PEM hash mismatch for {}",
                key.kid
            );
        }
        entries.push((key.kid, pem));
        println!("cargo:rerun-if-changed={}", pem_path.display());
    }

    let bundle_code = entries
        .iter()
        .map(|(kid, pem)| format!("    ({kid:?}, {pem:?}),"))
        .collect::<Vec<_>>()
        .join("\n");
    let source = manifest_path.display().to_string();
    let code = format!(
        "pub const ES256_PUBLIC_KEY_BUNDLE: &[(&str, &str)] = &[\n{bundle_code}\n];\n\
         pub const ES256_PUBLIC_KEY_SOURCE: &str = {source:?};\n",
    );
    let out_file = out_dir.join("license_key_generated.rs");
    fs::write(&out_file, code).unwrap_or_else(|e| {
        panic!(
            "Failed to write generated license key metadata to {}: {e}",
            out_file.display()
        )
    });

    println!("cargo:rerun-if-changed={}", manifest_path.display());
    println!("cargo:rerun-if-changed={}", schema_path.display());
}

#[allow(clippy::panic)] // Intentional: fail-fast at compile time for invalid edition
fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let crate_path = PathBuf::from(&crate_dir);
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    generate_license_key_code(&out_dir);

    // Public CE source builds are OSS-only. Pro/Enterprise binaries are built
    // from the internal release pipeline, not by toggling public CE metadata.
    let edition = env::var("NXUSKIT_EDITION").unwrap_or_else(|_| "oss".to_string());
    if edition != "oss" {
        panic!("Public CE builds only support NXUSKIT_EDITION=oss");
    }
    println!("cargo:rustc-env=NXUSKIT_EDITION={edition}");
    println!("cargo:rerun-if-env-changed=NXUSKIT_EDITION");

    // Emit NXUSKIT_LICENSE_SERVER_DEFAULT for build-time configurable license server URL.
    // Default: production URL. Override via NXUSKIT_LICENSE_SERVER_DEFAULT env var for dev/staging.
    let license_server = env::var("NXUSKIT_LICENSE_SERVER_DEFAULT")
        .unwrap_or_else(|_| "https://nxus.systems/licensing-api/v1".to_string());
    println!("cargo:rustc-env=NXUSKIT_LICENSE_SERVER_DEFAULT={license_server}");
    println!("cargo:rerun-if-env-changed=NXUSKIT_LICENSE_SERVER_DEFAULT");

    let license_environment = env::var("NXUSKIT_LICENSE_ENVIRONMENT_DEFAULT")
        .unwrap_or_else(|_| infer_license_environment(&license_server).to_string());
    println!("cargo:rustc-env=NXUSKIT_LICENSE_ENVIRONMENT_DEFAULT={license_environment}");
    println!("cargo:rerun-if-env-changed=NXUSKIT_LICENSE_ENVIRONMENT_DEFAULT");

    // Emit NXUSKIT_DEPLOYMENT_TOKEN for build-time embedded deployment token.
    // Empty by default. Pro developers set this when building redistributable apps.
    let deployment_token = env::var("NXUSKIT_DEPLOYMENT_TOKEN").unwrap_or_default();
    println!("cargo:rustc-env=NXUSKIT_DEPLOYMENT_TOKEN={deployment_token}");
    println!("cargo:rerun-if-env-changed=NXUSKIT_DEPLOYMENT_TOKEN");

    // Expose TARGET triple for nxuskit_build_info()
    let target = env::var("TARGET").unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=TARGET={target}");

    let out_path = crate_path.join("include").join("nxuskit.h");

    // Generate C header from Rust source via cbindgen.
    // Use with_src() instead of with_crate() to avoid `cargo metadata` running
    // in the nested workspace context (which doesn't include nxuskit-core).
    let config = cbindgen::Config::from_file(crate_path.join("cbindgen.toml")).unwrap_or_default();

    match cbindgen::Builder::new()
        .with_config(config)
        .with_src(crate_path.join("src").join("lib.rs"))
        .generate()
    {
        Ok(bindings) => {
            // cbindgen writes `after_includes`/`header`/`trailer` config
            // strings byte-for-byte. If `cbindgen.toml` was checked out with
            // CRLF line endings (e.g. on Windows without a `-text` rule),
            // those bytes leak into an otherwise LF-only generated header.
            // Normalize to LF here so the committed header is deterministic
            // regardless of the source checkout's line-ending environment.
            let mut generated = Vec::new();
            bindings.write(&mut generated);
            let normalized = normalize_to_lf(&generated);
            assert_header_generation_is_crlf_invariant(&crate_path, &out_dir, &normalized);
            write_generated_header(&out_path, &normalized);
        }
        Err(e) => {
            println!("cargo:warning=cbindgen failed: {e}. Header not regenerated.");
        }
    }

    // Platform-specific linker flags for symbol visibility
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    match target_os.as_str() {
        "linux" => {
            let script = crate_path.join("symbols.map");
            println!(
                "cargo:rustc-cdylib-link-arg=-Wl,--version-script={}",
                script.display()
            );
        }
        "macos" => {
            let script = crate_path.join("symbols.exp");
            println!(
                "cargo:rustc-cdylib-link-arg=-Wl,-exported_symbols_list,{}",
                script.display()
            );
        }
        "windows" => {
            let def = crate_path.join("nxuskit.def");
            println!("cargo:rustc-cdylib-link-arg=/DEF:{}", def.display());
        }
        _ => {}
    }

    // ── Product catalog generation ───────────────────────────────────
    match find_catalog_path(&crate_path) {
        Some(catalog_path) => {
            generate_catalog_code(&catalog_path, &out_dir);
            println!("cargo:rerun-if-changed={}", catalog_path.display());
        }
        None => {
            let profile = env::var("PROFILE").unwrap_or_default();
            let allow_release_fallback =
                env::var("NXUSKIT_ALLOW_FALLBACK_CATALOG_IN_RELEASE").as_deref() == Ok("1");
            let force_catalog = env::var("NXUSKIT_REQUIRE_PRODUCT_CATALOG").as_deref() == Ok("1");
            let require_catalog =
                force_catalog || (!allow_release_fallback && profile == "release");
            if require_catalog {
                panic!(
                    "Product catalog is required for release builds. \
                     Ensure CI checks out DevOps/sharedData/product-catalog-v1.yaml \
                     or set NXUSKIT_CATALOG_PATH."
                );
            }
            println!(
                "cargo:warning=Product catalog not found. Using hardcoded fallback defaults. \
                 Set NXUSKIT_CATALOG_PATH or ensure DevOps/ is a sibling repo for full catalog."
            );
            generate_fallback_catalog_code(&out_dir);
        }
    }
    println!("cargo:rerun-if-env-changed=NXUSKIT_CATALOG_PATH");
    println!("cargo:rerun-if-env-changed=NXUSKIT_REQUIRE_PRODUCT_CATALOG");
    println!("cargo:rerun-if-env-changed=NXUSKIT_ALLOW_FALLBACK_CATALOG_IN_RELEASE");

    // ── ES256 test public key generation ────────────────────────────
    println!("cargo:rerun-if-changed=src/");
    println!("cargo:rerun-if-changed=cbindgen.toml");
    println!("cargo:rerun-if-changed=symbols.map");
    println!("cargo:rerun-if-changed=symbols.exp");
    println!("cargo:rerun-if-changed=nxuskit.def");
}
