// Build script — panics are intentional for fail-fast at compile time.
#![allow(clippy::panic)]

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

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
        serde_yaml_ng::Value::Number(n) => {
            if let Some(u) = n.as_u64() {
                format!("Some({u})")
            } else {
                "None".to_string()
            }
        }
        serde_yaml_ng::Value::String(s) if s == "unlimited" => "None".to_string(),
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

const PRODUCTION_PUBKEY_RELATIVE_COMPONENTS: &[&str] = &[
    "..",
    "DevOps",
    "sharedData",
    "keys",
    "es256-production-pubkey.pem",
];

const DEV_FALLBACK_ES256_PUBLIC_KEY_PEM: &str = "\
-----BEGIN PUBLIC KEY-----\n\
MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEtVZb9c5IG8tk8XX9jXTZXN5gTVD6\n\
fxJff/reMNBUVQ93zPoKwVomqCRvUcGRoT55ROyhkiaZKzLf9odouShJ9g==\n\
-----END PUBLIC KEY-----\n";

fn repo_root(crate_dir: &Path) -> Option<&Path> {
    crate_dir
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
}

fn production_key_path(crate_dir: &Path) -> PathBuf {
    if let Ok(path) = env::var("NXUSKIT_PRODUCTION_PUBKEY_PATH") {
        return PathBuf::from(path);
    }

    repo_root(crate_dir)
        .and_then(|root| root.parent())
        .map(|parent| parent.join("DevOps/sharedData/keys/es256-production-pubkey.pem"))
        .unwrap_or_else(|| {
            PRODUCTION_PUBKEY_RELATIVE_COMPONENTS.iter().fold(
                PathBuf::new(),
                |mut path, component| {
                    path.push(component);
                    path
                },
            )
        })
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

fn generate_license_key_code(crate_dir: &Path, out_dir: &Path) {
    let key_path = production_key_path(crate_dir);
    let profile = env::var("PROFILE").unwrap_or_default();
    let allow_release_fallback = env::var("NXUSKIT_ALLOW_DEV_KEY_IN_RELEASE").as_deref() == Ok("1");
    let force_production_key = env::var("NXUSKIT_REQUIRE_PRODUCTION_KEY").as_deref() == Ok("1");
    let require_production_key =
        force_production_key || (!allow_release_fallback && profile == "release");

    let (pem, source) = if key_path.exists() {
        (
            fs::read_to_string(&key_path).unwrap_or_else(|e| {
                panic!(
                    "Failed to read production ES256 public key at {}: {e}",
                    key_path.display()
                )
            }),
            key_path.display().to_string(),
        )
    } else if require_production_key {
        panic!(
            "Production ES256 public key is required for release builds at {}. \
             Ensure CI checks out DevOps/sharedData/keys/es256-production-pubkey.pem \
             or set NXUSKIT_PRODUCTION_PUBKEY_PATH.",
            key_path.display()
        );
    } else {
        if profile == "release" {
            println!(
                "cargo:warning=Production ES256 public key not found at {}. \
                 Using dev/test fallback key because NXUSKIT_ALLOW_DEV_KEY_IN_RELEASE=1.",
                key_path.display()
            );
        } else {
            println!(
                "cargo:warning=Production ES256 public key not found at {}. \
                 Using dev/test fallback key for non-release build only.",
                key_path.display()
            );
        }
        (
            DEV_FALLBACK_ES256_PUBLIC_KEY_PEM.to_string(),
            "dev-test-fallback".to_string(),
        )
    };

    let code = format!(
        "pub const ES256_PUBLIC_KEY_PEM: &str = {pem:?};\n\
         pub const ES256_PUBLIC_KEY_SOURCE: &str = {source:?};\n\
         pub const ES256_PUBLIC_KEY_KID: &str = \"es256-v1\";\n"
    );
    let out_file = out_dir.join("license_key_generated.rs");
    fs::write(&out_file, code).unwrap_or_else(|e| {
        panic!(
            "Failed to write generated license key metadata to {}: {e}",
            out_file.display()
        )
    });

    println!("cargo:rerun-if-env-changed=NXUSKIT_PRODUCTION_PUBKEY_PATH");
    println!("cargo:rerun-if-env-changed=NXUSKIT_REQUIRE_PRODUCTION_KEY");
    println!("cargo:rerun-if-env-changed=NXUSKIT_ALLOW_DEV_KEY_IN_RELEASE");
    if key_path.exists() {
        println!("cargo:rerun-if-changed={}", key_path.display());
    }
}

#[allow(clippy::panic)] // Intentional: fail-fast at compile time for invalid edition
fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let crate_path = PathBuf::from(&crate_dir);
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    generate_license_key_code(&crate_path, &out_dir);

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
            bindings.write_to_file(&out_path);
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
}
