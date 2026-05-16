//! CLIPS Session API — C ABI functions.
//!
//! This module replaces the legacy `clips_sdk.rs` direct-access API with a
//! session-based API providing 86 operations across 14 categories. Sessions
//! use opaque integer handles (slotmap generational keys), JSON-serialized
//! value exchange, and single-owner concurrency.
//!
//! All functions use the `nxuskit_clips_session_` / `nxuskit_clips_fact_` /
//! `nxuskit_clips_rule_` etc. prefix and follow the established error pattern:
//! return NULL/-1/false on error, with details via `nxuskit_last_error()`.

use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use lru::LruCache;
use parking_lot::{Mutex, RwLock};
use slotmap::{SlotMap, new_key_type};

use clips_sys::{ClipsEnvironment, Strategy};

use crate::error;

// ── Session Key ────────────────────────────────────────────────────────

new_key_type! {
    /// Opaque session handle. The generational index prevents use-after-free:
    /// a destroyed session's key will not match any future session.
    struct SessionKey;
}

/// Serialize a `SessionKey` to `u64` for the C ABI.
fn key_to_u64(key: SessionKey) -> u64 {
    let data = key.0;
    // slotmap::KeyData::as_ffi() returns u64 in slotmap 1.x
    data.as_ffi()
}

/// Deserialize a `u64` from the C ABI back to a `SessionKey`.
fn u64_to_key(handle: u64) -> SessionKey {
    let data = slotmap::KeyData::from_ffi(handle);
    SessionKey::from(data)
}

// ── Session Entry ──────────────────────────────────────────────────────

/// An active CLIPS inference session managed by the session registry.
struct SessionEntry {
    /// The underlying CLIPS environment.
    environment: ClipsEnvironment,
    /// Optional human-readable name (set for cached sessions).
    #[allow(dead_code)]
    name: Option<String>,
    /// Timestamp of session creation.
    #[allow(dead_code)]
    created_at: Instant,
    /// Thread-safe halt signal for `session_halt` (FR-070).
    halt_flag: AtomicBool,
    /// Single-owner enforcement (FR-069). Operations acquire with `try_lock()`.
    lock: Mutex<()>,
}

impl SessionEntry {
    fn new(environment: ClipsEnvironment, name: Option<String>) -> Self {
        Self {
            environment,
            name,
            created_at: Instant::now(),
            halt_flag: AtomicBool::new(false),
            lock: Mutex::new(()),
        }
    }

    /// Fact count via CLIPS fact iterator.
    fn fact_count(&self) -> u64 {
        self.environment.facts().filter_map(|f| f.ok()).count() as u64
    }

    /// Rule count via CLIPS defrule iterator.
    fn rule_count(&self) -> u64 {
        self.environment.rules().count() as u64
    }
}

// ── Session Registry ───────────────────────────────────────────────────

/// Default maximum concurrent sessions.
const DEFAULT_MAX_SESSIONS: usize = 64;

/// Global registry of all active sessions.
struct SessionRegistry {
    sessions: SlotMap<SessionKey, SessionEntry>,
    max_sessions: usize,
}

impl SessionRegistry {
    fn new(max_sessions: usize) -> Self {
        Self {
            sessions: SlotMap::with_key(),
            max_sessions,
        }
    }

    /// Number of active sessions.
    fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Create a new session. Returns the key or an error string.
    fn create(
        &mut self,
        env: ClipsEnvironment,
        name: Option<String>,
    ) -> Result<SessionKey, String> {
        if self.sessions.len() >= self.max_sessions {
            return Err(format!(
                "Maximum concurrent sessions ({}) reached",
                self.max_sessions
            ));
        }
        let entry = SessionEntry::new(env, name);
        Ok(self.sessions.insert(entry))
    }

    /// Destroy a session by key. Returns true if it existed.
    fn destroy(&mut self, key: SessionKey) -> bool {
        self.sessions.remove(key).is_some()
    }

    /// Check if a session exists.
    #[allow(dead_code)]
    fn contains(&self, key: SessionKey) -> bool {
        self.sessions.contains_key(key)
    }

    /// Get a reference to a session entry.
    fn get(&self, key: SessionKey) -> Option<&SessionEntry> {
        self.sessions.get(key)
    }
}

/// Global session registry instance, protected by RwLock.
fn registry() -> &'static RwLock<SessionRegistry> {
    static REGISTRY: OnceLock<RwLock<SessionRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(|| RwLock::new(SessionRegistry::new(DEFAULT_MAX_SESSIONS)))
}

// ── Session Cache ──────────────────────────────────────────────────────

/// Default maximum cached sessions.
#[allow(dead_code)]
const DEFAULT_MAX_CACHED: usize = 32;

/// A preloaded rulebase stored in the cache.
#[allow(dead_code)]
struct CachedRulebase {
    /// The original rules configuration (JSON or CLIPS source).
    rules_config: String,
    /// SHA-256 content hash for deduplication.
    #[allow(dead_code)]
    content_hash: String,
    /// Cache insertion timestamp.
    #[allow(dead_code)]
    created_at: Instant,
}

/// LRU cache of preloaded session templates.
#[allow(dead_code)]
struct SessionCache {
    cache: LruCache<String, CachedRulebase>,
    /// Content-hash → name mapping for dedup.
    #[allow(dead_code)]
    hash_registry: HashMap<String, String>,
}

impl SessionCache {
    fn new(capacity: usize) -> Self {
        Self {
            cache: LruCache::new(
                std::num::NonZeroUsize::new(capacity).expect("capacity must be > 0"),
            ),
            hash_registry: HashMap::new(),
        }
    }
}

/// Global session cache instance, protected by Mutex.
#[allow(dead_code)]
fn session_cache() -> &'static Mutex<SessionCache> {
    static CACHE: OnceLock<Mutex<SessionCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(SessionCache::new(DEFAULT_MAX_CACHED)))
}

// ── ClipsValue JSON helpers ────────────────────────────────────────────

/// Convert a clips_sys::ClipsValue to type-tagged JSON string.
fn clips_value_to_json(value: &clips_sys::ClipsValue) -> String {
    match value {
        clips_sys::ClipsValue::Integer(i) => {
            format!(r#"{{"type":"integer","value":{i}}}"#)
        }
        clips_sys::ClipsValue::Float(f) => {
            format!(r#"{{"type":"float","value":{f}}}"#)
        }
        clips_sys::ClipsValue::String(s) => {
            let escaped = escape_json_value(s);
            format!(r#"{{"type":"string","value":"{escaped}"}}"#)
        }
        clips_sys::ClipsValue::Symbol(s) => {
            let escaped = escape_json_value(s);
            format!(r#"{{"type":"symbol","value":"{escaped}"}}"#)
        }
        clips_sys::ClipsValue::Boolean(b) => {
            let sym = if *b { "TRUE" } else { "FALSE" };
            format!(r#"{{"type":"symbol","value":"{sym}"}}"#)
        }
        clips_sys::ClipsValue::Multifield(items) => {
            let elements: Vec<String> = items.iter().map(clips_value_to_json).collect();
            format!(
                r#"{{"type":"multifield","value":[{}]}}"#,
                elements.join(",")
            )
        }
        clips_sys::ClipsValue::Void => r#"{"type":"void","value":null}"#.to_string(),
        clips_sys::ClipsValue::FactAddress(idx) => {
            format!(r#"{{"type":"fact_address","value":{idx}}}"#)
        }
        clips_sys::ClipsValue::InstanceAddress(name) => {
            let escaped = escape_json_value(name);
            format!(r#"{{"type":"instance_name","value":"{escaped}"}}"#)
        }
        clips_sys::ClipsValue::ExternalAddress(addr) => {
            format!(r#"{{"type":"external_address","value":{addr}}}"#)
        }
    }
}

/// Parse a type-tagged JSON value string back to clips_sys::ClipsValue.
fn json_to_clips_value(json: &str) -> Result<clips_sys::ClipsValue, String> {
    let parsed: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("Invalid JSON: {e}"))?;

    let obj = parsed
        .as_object()
        .ok_or_else(|| "Expected JSON object".to_string())?;

    let type_str = obj
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing 'type' field".to_string())?;

    let value = obj.get("value");

    match type_str {
        "integer" => {
            let n = value
                .and_then(|v| v.as_i64())
                .ok_or_else(|| "Invalid integer value".to_string())?;
            Ok(clips_sys::ClipsValue::Integer(n))
        }
        "float" => {
            let n = value
                .and_then(|v| v.as_f64())
                .ok_or_else(|| "Invalid float value".to_string())?;
            Ok(clips_sys::ClipsValue::Float(n))
        }
        "string" => {
            let s = value
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Invalid string value".to_string())?;
            Ok(clips_sys::ClipsValue::String(s.to_string()))
        }
        "symbol" => {
            let s = value
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Invalid symbol value".to_string())?;
            Ok(clips_sys::ClipsValue::Symbol(s.to_string()))
        }
        "multifield" => {
            let arr = value
                .and_then(|v| v.as_array())
                .ok_or_else(|| "Invalid multifield value".to_string())?;
            let items: Result<Vec<_>, _> = arr
                .iter()
                .map(|v| {
                    let s = serde_json::to_string(v).map_err(|e| format!("JSON error: {e}"))?;
                    json_to_clips_value(&s)
                })
                .collect();
            Ok(clips_sys::ClipsValue::Multifield(items?))
        }
        "void" => Ok(clips_sys::ClipsValue::Void),
        "fact_address" => {
            let n = value
                .and_then(|v| v.as_i64())
                .ok_or_else(|| "Invalid fact_address value".to_string())?;
            Ok(clips_sys::ClipsValue::FactAddress(n))
        }
        "instance_name" => {
            let s = value
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Invalid instance_name value".to_string())?;
            Ok(clips_sys::ClipsValue::InstanceAddress(s.to_string()))
        }
        other => Err(format!("Unknown type: {other}")),
    }
}

/// Convert a slot values map to a JSON object string.
fn slot_values_to_json(slots: &HashMap<String, clips_sys::ClipsValue>) -> String {
    let entries: Vec<String> = slots
        .iter()
        .map(|(name, value)| {
            let escaped_name = escape_json_value(name);
            let value_json = clips_value_to_json(value);
            format!(r#""{escaped_name}":{value_json}"#)
        })
        .collect();
    format!("{{{}}}", entries.join(","))
}

/// Minimal JSON string escaping.
fn escape_json_value(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// Helper to safely extract a `&str` from a `*const c_char`.
/// Returns `None` and sets an error if the pointer is null or not valid UTF-8.
unsafe fn c_str_to_str<'a>(ptr: *const c_char, param_name: &str) -> Option<&'a str> {
    if ptr.is_null() {
        error::set_last_error("invalid_argument", &format!("{param_name} is NULL"), None);
        return None;
    }
    let c_str = unsafe { CStr::from_ptr(ptr) };
    match c_str.to_str() {
        Ok(s) => Some(s),
        Err(e) => {
            error::set_last_error(
                "invalid_argument",
                &format!("{param_name} is not valid UTF-8: {e}"),
                None,
            );
            None
        }
    }
}

/// Helper to return a heap-allocated C string. Caller must free with `nxuskit_free_string`.
fn to_c_string_or_null(s: &str) -> *mut c_char {
    match CString::new(s) {
        Ok(cs) => cs.into_raw(),
        Err(_) => {
            error::set_last_error("internal_error", "String contains null byte", None);
            std::ptr::null_mut()
        }
    }
}

/// Helper macro for session operations: acquires the registry read lock,
/// looks up the session, acquires the session's single-owner lock, and
/// executes the body. Sets appropriate errors and returns $err_val on failure.
macro_rules! with_session {
    ($handle:expr, $err_val:expr, |$entry:ident, $env:ident| $body:block) => {{
        let key = u64_to_key($handle);
        let reg = registry().read();
        let $entry = match reg.get(key) {
            Some(entry) => entry,
            None => {
                error::set_last_error(
                    "invalid_argument",
                    "Invalid or destroyed session handle",
                    None,
                );
                return $err_val;
            }
        };
        // Single-owner enforcement: try to lock without blocking
        let _guard = match $entry.lock.try_lock() {
            Some(guard) => guard,
            None => {
                error::set_last_error(
                    "clips_error",
                    "Session is in use by another thread (single-owner violation)",
                    None,
                );
                return $err_val;
            }
        };
        let $env = &$entry.environment;
        $body
    }};
}

// ── Session Lifecycle C ABI (5 functions) ──────────────────────────────

/// Create a new isolated CLIPS session.
/// Returns session handle (>0) or 0 on error.
///
/// Enforces `max_sessions` limit from the product catalog based on the
/// current edition and license token.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_clips_session_create() -> u64 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let start = Instant::now();

        // Enforce catalog session limit
        let limits = crate::entitlement::effective_limits(
            crate::entitlement::current_license_key().as_deref(),
        );
        let reg_read = registry().read();
        let current_count = reg_read.session_count() as u64;
        drop(reg_read);
        let tier = env!("NXUSKIT_EDITION");
        if !crate::entitlement::check_limit(
            limits.max_sessions,
            current_count,
            "Concurrent CLIPS sessions",
            tier,
        ) {
            return 0;
        }

        let env = match ClipsEnvironment::new() {
            Ok(e) => e,
            Err(e) => {
                error::set_last_error(
                    "clips_error",
                    &format!("Failed to create CLIPS environment: {e}"),
                    None,
                );
                return 0;
            }
        };
        let mut reg = registry().write();
        match reg.create(env, None) {
            Ok(key) => {
                let handle = key_to_u64(key);
                log::debug!(
                    "Session created: handle={handle}, duration={:?}",
                    start.elapsed()
                );
                handle
            }
            Err(msg) => {
                error::set_last_error("clips_error", &msg, None);
                0
            }
        }
    }));
    result.unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in session_create", None);
        0
    })
}

/// Destroy a session and free resources. Invalidates the handle.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_clips_session_destroy(session: u64) {
    error::clear_last_error();
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let key = u64_to_key(session);
        let mut reg = registry().write();
        if reg.destroy(key) {
            log::debug!("Session destroyed: handle={session}");
        } else {
            error::set_last_error(
                "invalid_argument",
                "Invalid or already destroyed session handle",
                None,
            );
        }
    }));
}

/// Reset session: retract all facts, restore initial state, preserve rules.
/// Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_clips_session_reset(session: u64) -> i32 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        with_session!(session, -1, |_entry, env| {
            match env.reset() {
                Ok(()) => 0,
                Err(e) => {
                    error::set_last_error("clips_error", &format!("Reset failed: {e}"), None);
                    -1
                }
            }
        })
    }));
    result.unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in session_reset", None);
        -1
    })
}

/// Clear session: remove all constructs (rules, templates, facts, modules).
/// Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_clips_session_clear(session: u64) -> i32 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        with_session!(session, -1, |_entry, env| {
            match env.clear() {
                Ok(()) => 0,
                Err(e) => {
                    error::set_last_error("clips_error", &format!("Clear failed: {e}"), None);
                    -1
                }
            }
        })
    }));
    result.unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in session_clear", None);
        -1
    })
}

/// Get session metadata as JSON string. Caller frees with `nxuskit_free_string`.
/// Returns NULL on error.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_clips_session_info(session: u64) -> *mut c_char {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        with_session!(session, std::ptr::null_mut(), |_entry, env| {
            // Gather session info from the environment
            let fact_count = env.facts().filter_map(|f| f.ok()).count() as u64;
            let rule_count = env.rules().filter_map(|r| r.ok()).count() as u64;
            let template_count = env.templates().filter_map(|t| t.ok()).count() as u64;
            let module_names = env.list_module_names().unwrap_or_default();
            let global_count = 0u64; // COOL/global introspection not yet available
            let class_count = 0u64; // COOL class introspection not yet available
            let agenda_size = env.agenda_size() as u64;
            let current_module = env
                .get_focus()
                .and_then(|m| m.name().ok())
                .unwrap_or_else(|| "MAIN".to_string());

            let modules_json: Vec<String> = module_names
                .iter()
                .map(|n| format!("\"{}\"", escape_json_value(n)))
                .collect();

            let json = format!(
                r#"{{"fact_count":{fact_count},"rule_count":{rule_count},"template_count":{template_count},"module_names":[{}],"global_count":{global_count},"class_count":{class_count},"agenda_size":{agenda_size},"current_module":"{}"}}"#,
                modules_json.join(","),
                escape_json_value(&current_module),
            );
            to_c_string_or_null(&json)
        })
    }));
    result.unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in session_info", None);
        std::ptr::null_mut()
    })
}

// ── Construct Loading C ABI (7 functions) ──────────────────────────────

/// Load CLIPS constructs from a .clp file.
/// Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_clips_session_load_file(session: u64, path: *const c_char) -> i32 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let path_str = match unsafe { c_str_to_str(path, "path") } {
            Some(s) => s,
            None => return -1,
        };
        let start = Instant::now();
        with_session!(session, -1, |_entry, env| {
            match env.load(path_str) {
                Ok(()) => {
                    log::debug!(
                        "Session {session}: loaded file '{}', duration={:?}",
                        path_str,
                        start.elapsed()
                    );
                    0
                }
                Err(e) => {
                    error::set_last_error("clips_error", &format!("Load file failed: {e}"), None);
                    -1
                }
            }
        })
    }));
    result.unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in session_load_file", None);
        -1
    })
}

/// Load CLIPS constructs from a string.
/// Returns 0 on success, -1 on error.
///
/// Enforces `max_rules_per_session` from the product catalog (checked
/// after loading, since one string may define multiple rules).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_clips_session_load_string(
    session: u64,
    constructs: *const c_char,
) -> i32 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let constructs_str = match unsafe { c_str_to_str(constructs, "constructs") } {
            Some(s) => s,
            None => return -1,
        };
        with_session!(session, -1, |entry, env| {
            // Check rule limit before loading
            let limits = crate::entitlement::effective_limits(
                crate::entitlement::current_license_key().as_deref(),
            );
            if !crate::entitlement::check_limit(
                limits.max_rules_per_session,
                entry.rule_count(),
                "Rules per session",
                env!("NXUSKIT_EDITION"),
            ) {
                return -1;
            }
            match env.load_from_string(constructs_str) {
                Ok(()) => 0,
                Err(e) => {
                    error::set_last_error("clips_error", &format!("Load string failed: {e}"), None);
                    -1
                }
            }
        })
    }));
    result.unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in session_load_string", None);
        -1
    })
}

/// Load pre-compiled binary constructs from a file.
/// Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_clips_session_load_binary(
    _session: u64,
    path: *const c_char,
) -> i32 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let path_str = match unsafe { c_str_to_str(path, "path") } {
            Some(s) => s,
            None => return -1,
        };
        let _ = path_str; // suppress unused warning
        error::set_last_error(
            "clips_error",
            "Binary load (bload) is not yet available in this CLIPS binding",
            None,
        );
        -1
    }));
    result.unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in session_load_binary", None);
        -1
    })
}

/// Save compiled constructs to a binary file.
/// Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_clips_session_save_binary(
    _session: u64,
    path: *const c_char,
) -> i32 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let path_str = match unsafe { c_str_to_str(path, "path") } {
            Some(s) => s,
            None => return -1,
        };
        let _ = path_str; // suppress unused warning
        error::set_last_error(
            "clips_error",
            "Binary save (bsave) is not yet available in this CLIPS binding",
            None,
        );
        -1
    }));
    result.unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in session_save_binary", None);
        -1
    })
}

/// Build a single CLIPS construct from a string.
/// Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_clips_session_build(
    session: u64,
    construct: *const c_char,
) -> i32 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let construct_str = match unsafe { c_str_to_str(construct, "construct") } {
            Some(s) => s,
            None => return -1,
        };
        with_session!(session, -1, |_entry, env| {
            match env.build(construct_str) {
                Ok(()) => 0,
                Err(e) => {
                    error::set_last_error("clips_error", &format!("Build failed: {e}"), None);
                    -1
                }
            }
        })
    }));
    result.unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in session_build", None);
        -1
    })
}

/// Execute a CLIPS batch file.
/// Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_clips_session_batch(_session: u64, path: *const c_char) -> i32 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let path_str = match unsafe { c_str_to_str(path, "path") } {
            Some(s) => s,
            None => return -1,
        };
        let _ = path_str; // suppress unused warning
        error::set_last_error(
            "clips_error",
            "Batch execution is not yet available in this CLIPS binding",
            None,
        );
        -1
    }));
    result.unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in session_batch", None);
        -1
    })
}

// ── Fact Operations C ABI (11 functions) ───────────────────────────────

/// Assert a fact from a CLIPS string. Returns fact index or -1 on error.
///
/// Enforces `max_facts_per_session` from the product catalog.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_clips_fact_assert_string(
    session: u64,
    fact_string: *const c_char,
) -> i64 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let fact_str = match unsafe { c_str_to_str(fact_string, "fact_string") } {
            Some(s) => s,
            None => return -1,
        };
        with_session!(session, -1, |entry, env| {
            // Enforce fact limit
            let limits = crate::entitlement::effective_limits(
                crate::entitlement::current_license_key().as_deref(),
            );
            if !crate::entitlement::check_limit(
                limits.max_facts_per_session,
                entry.fact_count(),
                "Facts per session",
                env!("NXUSKIT_EDITION"),
            ) {
                return -1;
            }
            match env.assert_string(fact_str) {
                Ok(handle) => handle.index(),
                Err(e) => {
                    error::set_last_error("clips_error", &format!("Assert failed: {e}"), None);
                    -1
                }
            }
        })
    }));
    result.unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in fact_assert_string", None);
        -1
    })
}

/// Assert a structured fact. Returns fact index or -1 on error.
/// `slots_json` is a JSON object mapping slot names to type-tagged values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_clips_fact_assert_structured(
    session: u64,
    template_name: *const c_char,
    slots_json: *const c_char,
) -> i64 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let tmpl = match unsafe { c_str_to_str(template_name, "template_name") } {
            Some(s) => s,
            None => return -1,
        };
        let slots_str = match unsafe { c_str_to_str(slots_json, "slots_json") } {
            Some(s) => s,
            None => return -1,
        };

        // Parse the slots JSON object
        let slots_obj: serde_json::Value = match serde_json::from_str(slots_str) {
            Ok(v) => v,
            Err(e) => {
                error::set_last_error(
                    "invalid_argument",
                    &format!("Invalid slots JSON: {e}"),
                    None,
                );
                return -1;
            }
        };

        let obj = match slots_obj.as_object() {
            Some(o) => o,
            None => {
                error::set_last_error("invalid_argument", "slots_json must be a JSON object", None);
                return -1;
            }
        };

        // Build a CLIPS fact assertion string from the structured data
        let mut slot_strings = Vec::new();
        for (name, value) in obj {
            let value_str = serde_json::to_string(value).unwrap_or_default();
            match json_to_clips_value(&value_str) {
                Ok(cv) => {
                    slot_strings.push(format!("({name} {})", cv.to_clips_string()));
                }
                Err(e) => {
                    error::set_last_error(
                        "invalid_argument",
                        &format!("Invalid value for slot '{name}': {e}"),
                        None,
                    );
                    return -1;
                }
            }
        }

        let fact_str = format!("({tmpl} {})", slot_strings.join(" "));

        with_session!(session, -1, |_entry, env| {
            match env.assert_string(&fact_str) {
                Ok(handle) => handle.index(),
                Err(e) => {
                    error::set_last_error(
                        "clips_error",
                        &format!("Assert structured failed: {e}"),
                        None,
                    );
                    -1
                }
            }
        })
    }));
    result.unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in fact_assert_structured", None);
        -1
    })
}

/// Retract a fact by index. Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_clips_fact_retract(session: u64, fact_index: i64) -> i32 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        with_session!(session, -1, |_entry, env| {
            // Find the fact by iterating, then retract it
            let fact = env
                .facts()
                .filter_map(|f| f.ok())
                .find(|f| f.index() == fact_index);
            match fact {
                Some(f) => match f.retract() {
                    Ok(()) => 0,
                    Err(e) => {
                        error::set_last_error("clips_error", &format!("Retract failed: {e}"), None);
                        -1
                    }
                },
                None => {
                    error::set_last_error(
                        "clips_error",
                        &format!("Fact with index {fact_index} not found"),
                        None,
                    );
                    -1
                }
            }
        })
    }));
    result.unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in fact_retract", None);
        -1
    })
}

/// Retract all facts of a template. Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_clips_fact_retract_by_template(
    session: u64,
    template_name: *const c_char,
) -> i32 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let tmpl = match unsafe { c_str_to_str(template_name, "template_name") } {
            Some(s) => s,
            None => return -1,
        };
        with_session!(session, -1, |_entry, env| {
            match env.retract_by_template(tmpl) {
                Ok(count) => count as i32,
                Err(e) => {
                    error::set_last_error(
                        "clips_error",
                        &format!("Retract by template failed: {e}"),
                        None,
                    );
                    -1
                }
            }
        })
    }));
    result.unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in fact_retract_by_template", None);
        -1
    })
}

/// Check if a fact exists. Returns true if it does.
/// On error (invalid session), returns false; check nxuskit_last_error().
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_clips_fact_exists(session: u64, fact_index: i64) -> bool {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        with_session!(session, false, |_entry, env| {
            env.facts()
                .filter_map(|f| f.ok())
                .any(|f| f.index() == fact_index)
        })
    }));
    result.unwrap_or(false)
}

/// Get a single slot value as type-tagged JSON. Caller frees.
/// Returns NULL on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_clips_fact_get_slot(
    session: u64,
    fact_index: i64,
    slot_name: *const c_char,
) -> *mut c_char {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let slot = match unsafe { c_str_to_str(slot_name, "slot_name") } {
            Some(s) => s,
            None => return std::ptr::null_mut(),
        };
        with_session!(session, std::ptr::null_mut(), |_entry, env| {
            let fact = env
                .facts()
                .filter_map(|f| f.ok())
                .find(|f| f.index() == fact_index);
            match fact {
                Some(f) => match f.get_slot(slot) {
                    Ok(value) => to_c_string_or_null(&clips_value_to_json(&value)),
                    Err(e) => {
                        error::set_last_error(
                            "clips_error",
                            &format!("Get slot failed: {e}"),
                            None,
                        );
                        std::ptr::null_mut()
                    }
                },
                None => {
                    error::set_last_error(
                        "clips_error",
                        &format!("Fact with index {fact_index} not found"),
                        None,
                    );
                    std::ptr::null_mut()
                }
            }
        })
    }));
    result.unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in fact_get_slot", None);
        std::ptr::null_mut()
    })
}

/// Get all slot values as JSON object. Caller frees.
/// Returns NULL on error.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_clips_fact_slot_values(session: u64, fact_index: i64) -> *mut c_char {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        with_session!(session, std::ptr::null_mut(), |_entry, env| {
            let fact = env
                .facts()
                .filter_map(|f| f.ok())
                .find(|f| f.index() == fact_index);
            match fact {
                Some(f) => match f.slot_values() {
                    Ok(slots) => to_c_string_or_null(&slot_values_to_json(&slots)),
                    Err(e) => {
                        error::set_last_error(
                            "clips_error",
                            &format!("Slot values failed: {e}"),
                            None,
                        );
                        std::ptr::null_mut()
                    }
                },
                None => {
                    error::set_last_error(
                        "clips_error",
                        &format!("Fact with index {fact_index} not found"),
                        None,
                    );
                    std::ptr::null_mut()
                }
            }
        })
    }));
    result.unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in fact_slot_values", None);
        std::ptr::null_mut()
    })
}

/// Get pretty-print form of a fact. Caller frees.
/// Returns NULL on error.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_clips_fact_pp_form(session: u64, fact_index: i64) -> *mut c_char {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        with_session!(session, std::ptr::null_mut(), |_entry, env| {
            let fact = env
                .facts()
                .filter_map(|f| f.ok())
                .find(|f| f.index() == fact_index);
            match fact {
                Some(f) => to_c_string_or_null(&f.pp_form()),
                None => {
                    error::set_last_error(
                        "clips_error",
                        &format!("Fact with index {fact_index} not found"),
                        None,
                    );
                    std::ptr::null_mut()
                }
            }
        })
    }));
    result.unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in fact_pp_form", None);
        std::ptr::null_mut()
    })
}

/// Get fact index (identity operation, useful after iteration).
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_clips_fact_index(session: u64, fact_index: i64) -> i64 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        with_session!(session, -1, |_entry, _env| {
            // This is an identity operation — the fact index IS the identifier
            fact_index
        })
    }));
    result.unwrap_or(-1)
}

/// Get all fact indices as JSON array. Caller frees.
/// Returns NULL on error.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_clips_facts_list(session: u64) -> *mut c_char {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        with_session!(session, std::ptr::null_mut(), |_entry, env| {
            let indices: Vec<i64> = env
                .facts()
                .filter_map(|f| f.ok())
                .map(|f| f.index())
                .collect();
            let json = format!(
                "[{}]",
                indices
                    .iter()
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            );
            to_c_string_or_null(&json)
        })
    }));
    result.unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in facts_list", None);
        std::ptr::null_mut()
    })
}

/// Get facts by template as JSON array of indices. Caller frees.
/// Returns NULL on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_clips_facts_by_template(
    session: u64,
    template_name: *const c_char,
) -> *mut c_char {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let tmpl = match unsafe { c_str_to_str(template_name, "template_name") } {
            Some(s) => s,
            None => return std::ptr::null_mut(),
        };
        with_session!(session, std::ptr::null_mut(), |_entry, env| {
            match env.find_template(tmpl) {
                Ok(Some(template)) => {
                    let indices: Vec<i64> = template
                        .facts()
                        .filter_map(|f| f.ok())
                        .map(|f| f.index())
                        .collect();
                    let json = format!(
                        "[{}]",
                        indices
                            .iter()
                            .map(|i| i.to_string())
                            .collect::<Vec<_>>()
                            .join(",")
                    );
                    to_c_string_or_null(&json)
                }
                Ok(None) => {
                    error::set_last_error(
                        "clips_error",
                        &format!("Template '{tmpl}' not found"),
                        None,
                    );
                    std::ptr::null_mut()
                }
                Err(e) => {
                    error::set_last_error(
                        "clips_error",
                        &format!("Facts by template failed: {e}"),
                        None,
                    );
                    std::ptr::null_mut()
                }
            }
        })
    }));
    result.unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in facts_by_template", None);
        std::ptr::null_mut()
    })
}

// ── Template Operations C ABI (6 functions) ────────────────────────────

/// Check if a template exists. Returns true/false.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_clips_template_exists(session: u64, name: *const c_char) -> bool {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let tmpl_name = match unsafe { c_str_to_str(name, "name") } {
            Some(s) => s,
            None => return false,
        };
        with_session!(session, false, |_entry, env| {
            env.find_template(tmpl_name)
                .map(|opt| opt.is_some())
                .unwrap_or(false)
        })
    }));
    result.unwrap_or(false)
}

/// Get list of all template names as JSON array. Caller frees.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_clips_template_list(session: u64) -> *mut c_char {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        with_session!(session, std::ptr::null_mut(), |_entry, env| {
            let names: Vec<String> = env
                .templates()
                .filter_map(|t| t.ok())
                .filter_map(|t| t.name().ok())
                .collect();
            let json_names: Vec<String> = names
                .iter()
                .map(|n| format!("\"{}\"", escape_json_value(n)))
                .collect();
            to_c_string_or_null(&format!("[{}]", json_names.join(",")))
        })
    }));
    result.unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in template_list", None);
        std::ptr::null_mut()
    })
}

/// Get slot names for a template as JSON array. Caller frees.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_clips_template_slot_names(
    session: u64,
    template_name: *const c_char,
) -> *mut c_char {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let tmpl = match unsafe { c_str_to_str(template_name, "template_name") } {
            Some(s) => s,
            None => return std::ptr::null_mut(),
        };
        with_session!(session, std::ptr::null_mut(), |_entry, env| {
            match env.find_template(tmpl) {
                Ok(Some(template)) => match template.slot_names() {
                    Ok(names) => {
                        let json_names: Vec<String> = names
                            .iter()
                            .map(|n| format!("\"{}\"", escape_json_value(n)))
                            .collect();
                        to_c_string_or_null(&format!("[{}]", json_names.join(",")))
                    }
                    Err(e) => {
                        error::set_last_error(
                            "clips_error",
                            &format!("Template slot names failed: {e}"),
                            None,
                        );
                        std::ptr::null_mut()
                    }
                },
                Ok(None) => {
                    error::set_last_error(
                        "clips_error",
                        &format!("Template '{tmpl}' not found"),
                        None,
                    );
                    std::ptr::null_mut()
                }
                Err(e) => {
                    error::set_last_error(
                        "clips_error",
                        &format!("Template slot names failed: {e}"),
                        None,
                    );
                    std::ptr::null_mut()
                }
            }
        })
    }));
    result.unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in template_slot_names", None);
        std::ptr::null_mut()
    })
}

/// Get slot info for a template as JSON array. Caller frees.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_clips_template_slot_info(
    session: u64,
    template_name: *const c_char,
) -> *mut c_char {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let tmpl = match unsafe { c_str_to_str(template_name, "template_name") } {
            Some(s) => s,
            None => return std::ptr::null_mut(),
        };
        with_session!(session, std::ptr::null_mut(), |_entry, env| {
            match env.find_template(tmpl) {
                Ok(Some(template)) => match template.slot_names() {
                    Ok(names) => {
                        let json_slots: Vec<String> = names
                            .iter()
                            .map(|name| {
                                let is_multi = template.slot_is_multi(name).unwrap_or(false);
                                let slot_type = if is_multi { "multi" } else { "single" };
                                format!(
                                    r#"{{"name":"{}","slot_type":"{}"}}"#,
                                    escape_json_value(name),
                                    slot_type
                                )
                            })
                            .collect();
                        to_c_string_or_null(&format!("[{}]", json_slots.join(",")))
                    }
                    Err(e) => {
                        error::set_last_error(
                            "clips_error",
                            &format!("Template slot info failed: {e}"),
                            None,
                        );
                        std::ptr::null_mut()
                    }
                },
                Ok(None) => {
                    error::set_last_error(
                        "clips_error",
                        &format!("Template '{tmpl}' not found"),
                        None,
                    );
                    std::ptr::null_mut()
                }
                Err(e) => {
                    error::set_last_error(
                        "clips_error",
                        &format!("Template slot info failed: {e}"),
                        None,
                    );
                    std::ptr::null_mut()
                }
            }
        })
    }));
    result.unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in template_slot_info", None);
        std::ptr::null_mut()
    })
}

/// Get facts for a template as JSON array of indices. Caller frees.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_clips_template_facts(
    session: u64,
    template_name: *const c_char,
) -> *mut c_char {
    error::clear_last_error();
    // Delegate to facts_by_template — same operation
    unsafe { nxuskit_clips_facts_by_template(session, template_name) }
}

/// Get pretty-print form of a template. Caller frees.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_clips_template_pp_form(
    session: u64,
    template_name: *const c_char,
) -> *mut c_char {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let tmpl = match unsafe { c_str_to_str(template_name, "template_name") } {
            Some(s) => s,
            None => return std::ptr::null_mut(),
        };
        with_session!(session, std::ptr::null_mut(), |_entry, env| {
            match env.find_template(tmpl) {
                Ok(Some(template)) => match template.pp_form() {
                    Some(pp) => to_c_string_or_null(&pp),
                    None => to_c_string_or_null(""),
                },
                Ok(None) => {
                    error::set_last_error(
                        "clips_error",
                        &format!("Template '{tmpl}' not found"),
                        None,
                    );
                    std::ptr::null_mut()
                }
                Err(e) => {
                    error::set_last_error(
                        "clips_error",
                        &format!("Template PP form failed: {e}"),
                        None,
                    );
                    std::ptr::null_mut()
                }
            }
        })
    }));
    result.unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in template_pp_form", None);
        std::ptr::null_mut()
    })
}

// ── Execution & Agenda C ABI (9 functions) ─────────────────────────────

/// Run inference. limit = -1 for no limit. Returns rules fired or -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_clips_session_run(session: u64, limit: i64) -> i64 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let start = Instant::now();
        let run_limit = if limit < 0 { None } else { Some(limit) };
        with_session!(session, -1, |entry, env| {
            // Clear halt flag before running
            entry.halt_flag.store(false, Ordering::SeqCst);
            match env.run(run_limit) {
                Ok(run_result) => {
                    let fired = run_result.rules_fired as i64;
                    log::debug!(
                        "Session {session}: run completed, fired={fired}, duration={:?}",
                        start.elapsed()
                    );
                    fired
                }
                Err(e) => {
                    error::set_last_error("clips_error", &format!("Run failed: {e}"), None);
                    -1
                }
            }
        })
    }));
    result.unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in session_run", None);
        -1
    })
}

/// Halt a running inference from another thread.
/// Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_clips_session_halt(session: u64) -> i32 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let key = u64_to_key(session);
        let reg = registry().read();
        match reg.get(key) {
            Some(entry) => {
                // Set the halt flag — does NOT require the session lock (AtomicBool)
                entry.halt_flag.store(true, Ordering::SeqCst);
                // Also halt the CLIPS environment directly
                entry.environment.halt();
                0
            }
            None => {
                error::set_last_error(
                    "invalid_argument",
                    "Invalid or destroyed session handle",
                    None,
                );
                -1
            }
        }
    }));
    result.unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in session_halt", None);
        -1
    })
}

/// Get agenda size. Returns count or -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_clips_agenda_size(session: u64) -> i64 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        with_session!(session, -1, |_entry, env| { env.agenda_size() as i64 })
    }));
    result.unwrap_or(-1)
}

/// Clear the agenda. Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_clips_agenda_clear(session: u64) -> i32 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        with_session!(session, -1, |_entry, env| {
            env.clear_agenda();
            0
        })
    }));
    result.unwrap_or(-1)
}

/// Reorder the agenda. Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_clips_agenda_reorder(_session: u64) -> i32 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        error::set_last_error(
            "clips_error",
            "Agenda reorder is not yet available in this CLIPS binding",
            None,
        );
        -1
    }));
    result.unwrap_or(-1)
}

/// Get current conflict resolution strategy. Caller frees.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_clips_strategy_get(session: u64) -> *mut c_char {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        with_session!(session, std::ptr::null_mut(), |_entry, env| {
            let strategy = env.get_strategy();
            let name = match strategy {
                Strategy::Depth => "depth",
                Strategy::Breadth => "breadth",
                Strategy::Lex => "lex",
                Strategy::Mea => "mea",
                Strategy::Complexity => "complexity",
                Strategy::Simplicity => "simplicity",
                Strategy::Random => "random",
            };
            to_c_string_or_null(name)
        })
    }));
    result.unwrap_or(std::ptr::null_mut())
}

/// Set conflict resolution strategy. Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_clips_strategy_set(session: u64, strategy: *const c_char) -> i32 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let strategy_str = match unsafe { c_str_to_str(strategy, "strategy") } {
            Some(s) => s,
            None => return -1,
        };
        let parsed = match strategy_str.to_lowercase().as_str() {
            "depth" => Strategy::Depth,
            "breadth" => Strategy::Breadth,
            "lex" => Strategy::Lex,
            "mea" => Strategy::Mea,
            "complexity" => Strategy::Complexity,
            "simplicity" => Strategy::Simplicity,
            "random" => Strategy::Random,
            other => {
                error::set_last_error(
                    "invalid_argument",
                    &format!(
                        "Unknown strategy: '{other}'. Valid: depth, breadth, lex, mea, complexity, simplicity, random"
                    ),
                    None,
                );
                return -1;
            }
        };
        with_session!(session, -1, |_entry, env| {
            env.set_strategy(parsed);
            0
        })
    }));
    result.unwrap_or(-1)
}

/// Get salience evaluation mode. Caller frees.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_clips_salience_mode_get(_session: u64) -> *mut c_char {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        error::set_last_error(
            "clips_error",
            "Salience mode get is not yet available in this CLIPS binding",
            None,
        );
        std::ptr::null_mut()
    }));
    result.unwrap_or(std::ptr::null_mut())
}

/// Set salience evaluation mode. Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_clips_salience_mode_set(
    _session: u64,
    mode: *const c_char,
) -> i32 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let mode_str = match unsafe { c_str_to_str(mode, "mode") } {
            Some(s) => s,
            None => return -1,
        };
        let _ = mode_str; // suppress unused warning
        error::set_last_error(
            "clips_error",
            "Salience mode set is not yet available in this CLIPS binding",
            None,
        );
        -1
    }));
    result.unwrap_or(-1)
}

// ── Rule Operations C ABI (9 functions) ────────────────────────────────

/// Check if a rule exists.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_clips_rule_exists(session: u64, name: *const c_char) -> bool {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let rule_name = match unsafe { c_str_to_str(name, "name") } {
            Some(s) => s,
            None => return false,
        };
        with_session!(session, false, |_entry, env| {
            env.find_rule(rule_name)
                .map(|opt| opt.is_some())
                .unwrap_or(false)
        })
    }));
    result.unwrap_or(false)
}

/// Get list of all rule names as JSON array. Caller frees.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_clips_rule_list(session: u64) -> *mut c_char {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        with_session!(session, std::ptr::null_mut(), |_entry, env| {
            let names: Vec<String> = env
                .rules()
                .filter_map(|r| r.ok())
                .filter_map(|r| r.name().ok())
                .collect();
            let json_names: Vec<String> = names
                .iter()
                .map(|n| format!("\"{}\"", escape_json_value(n)))
                .collect();
            to_c_string_or_null(&format!("[{}]", json_names.join(",")))
        })
    }));
    result.unwrap_or(std::ptr::null_mut())
}

/// Get number of times a rule has fired. Returns count or -1 on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_clips_rule_times_fired(
    session: u64,
    rule_name: *const c_char,
) -> i64 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let name = match unsafe { c_str_to_str(rule_name, "rule_name") } {
            Some(s) => s,
            None => return -1,
        };
        with_session!(session, -1, |_entry, env| {
            match env.find_rule(name) {
                Ok(Some(rule)) => rule.times_fired() as i64,
                Ok(None) => {
                    error::set_last_error("clips_error", &format!("Rule '{name}' not found"), None);
                    -1
                }
                Err(e) => {
                    error::set_last_error(
                        "clips_error",
                        &format!("Rule times fired failed: {e}"),
                        None,
                    );
                    -1
                }
            }
        })
    }));
    result.unwrap_or(-1)
}

/// Set a breakpoint on a rule. Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_clips_rule_breakpoint_set(
    session: u64,
    rule_name: *const c_char,
) -> i32 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let name = match unsafe { c_str_to_str(rule_name, "rule_name") } {
            Some(s) => s,
            None => return -1,
        };
        with_session!(session, -1, |_entry, env| {
            match env.find_rule(name) {
                Ok(Some(rule)) => {
                    rule.set_breakpoint();
                    0
                }
                Ok(None) => {
                    error::set_last_error("clips_error", &format!("Rule '{name}' not found"), None);
                    -1
                }
                Err(e) => {
                    error::set_last_error(
                        "clips_error",
                        &format!("Set breakpoint failed: {e}"),
                        None,
                    );
                    -1
                }
            }
        })
    }));
    result.unwrap_or(-1)
}

/// Remove a breakpoint from a rule. Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_clips_rule_breakpoint_remove(
    session: u64,
    rule_name: *const c_char,
) -> i32 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let name = match unsafe { c_str_to_str(rule_name, "rule_name") } {
            Some(s) => s,
            None => return -1,
        };
        with_session!(session, -1, |_entry, env| {
            match env.find_rule(name) {
                Ok(Some(rule)) => {
                    rule.remove_breakpoint();
                    0
                }
                Ok(None) => {
                    error::set_last_error("clips_error", &format!("Rule '{name}' not found"), None);
                    -1
                }
                Err(e) => {
                    error::set_last_error(
                        "clips_error",
                        &format!("Remove breakpoint failed: {e}"),
                        None,
                    );
                    -1
                }
            }
        })
    }));
    result.unwrap_or(-1)
}

/// Check if a rule has a breakpoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_clips_rule_has_breakpoint(
    session: u64,
    rule_name: *const c_char,
) -> bool {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let name = match unsafe { c_str_to_str(rule_name, "rule_name") } {
            Some(s) => s,
            None => return false,
        };
        with_session!(session, false, |_entry, env| {
            env.find_rule(name)
                .map(|opt| opt.map(|r| r.has_breakpoint()).unwrap_or(false))
                .unwrap_or(false)
        })
    }));
    result.unwrap_or(false)
}

/// Refresh a rule's activations. Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_clips_rule_refresh(
    _session: u64,
    rule_name: *const c_char,
) -> i32 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let name = match unsafe { c_str_to_str(rule_name, "rule_name") } {
            Some(s) => s,
            None => return -1,
        };
        let _ = name; // suppress unused warning
        error::set_last_error(
            "clips_error",
            "Rule refresh is not yet available in this CLIPS binding",
            None,
        );
        -1
    }));
    result.unwrap_or(-1)
}

/// Get pretty-print form of a rule. Caller frees.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_clips_rule_pp_form(
    session: u64,
    rule_name: *const c_char,
) -> *mut c_char {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let name = match unsafe { c_str_to_str(rule_name, "rule_name") } {
            Some(s) => s,
            None => return std::ptr::null_mut(),
        };
        with_session!(session, std::ptr::null_mut(), |_entry, env| {
            match env.find_rule(name) {
                Ok(Some(rule)) => match rule.pp_form() {
                    Some(pp) => to_c_string_or_null(&pp),
                    None => to_c_string_or_null(""),
                },
                Ok(None) => {
                    error::set_last_error("clips_error", &format!("Rule '{name}' not found"), None);
                    std::ptr::null_mut()
                }
                Err(e) => {
                    error::set_last_error(
                        "clips_error",
                        &format!("Rule PP form failed: {e}"),
                        None,
                    );
                    std::ptr::null_mut()
                }
            }
        })
    }));
    result.unwrap_or(std::ptr::null_mut())
}

/// Delete a rule. Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_clips_rule_delete(_session: u64, rule_name: *const c_char) -> i32 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let name = match unsafe { c_str_to_str(rule_name, "rule_name") } {
            Some(s) => s,
            None => return -1,
        };
        let _ = name; // suppress unused warning
        error::set_last_error(
            "clips_error",
            "Rule delete is not yet available in this CLIPS binding",
            None,
        );
        -1
    }));
    result.unwrap_or(-1)
}

// ── Settings C ABI (4 functions) ───────────────────────────────────────

/// Get fact duplication setting.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_clips_fact_duplication_get(session: u64) -> bool {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        with_session!(session, false, |_entry, env| { env.get_fact_duplication() })
    }));
    result.unwrap_or(false)
}

/// Set fact duplication setting. Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_clips_fact_duplication_set(session: u64, allow: bool) -> i32 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        with_session!(session, -1, |_entry, env| {
            env.set_fact_duplication(allow);
            0
        })
    }));
    result.unwrap_or(-1)
}

/// Get reset globals setting.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_clips_reset_globals_get(_session: u64) -> bool {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        // reset_globals_get is not yet available in this CLIPS binding
        // Default to true (CLIPS default behavior)
        true
    }));
    result.unwrap_or(false)
}

/// Set reset globals setting. Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_clips_reset_globals_set(_session: u64, reset: bool) -> i32 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let _ = reset; // suppress unused warning
        error::set_last_error(
            "clips_error",
            "Reset globals set is not yet available in this CLIPS binding",
            None,
        );
        -1
    }));
    result.unwrap_or(-1)
}

// ── Expression Evaluation C ABI (2 functions) ──────────────────────────

/// Evaluate a CLIPS expression. Returns type-tagged JSON. Caller frees.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_clips_eval(
    session: u64,
    expression: *const c_char,
) -> *mut c_char {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let expr = match unsafe { c_str_to_str(expression, "expression") } {
            Some(s) => s,
            None => return std::ptr::null_mut(),
        };
        with_session!(session, std::ptr::null_mut(), |_entry, env| {
            match env.eval(expr) {
                Ok(value) => to_c_string_or_null(&clips_value_to_json(&value)),
                Err(e) => {
                    error::set_last_error("clips_error", &format!("Eval failed: {e}"), None);
                    std::ptr::null_mut()
                }
            }
        })
    }));
    result.unwrap_or(std::ptr::null_mut())
}

/// Call a CLIPS function by name with JSON arguments. Returns type-tagged JSON. Caller frees.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_clips_function_call(
    session: u64,
    function_name: *const c_char,
    args_json: *const c_char,
) -> *mut c_char {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let name = match unsafe { c_str_to_str(function_name, "function_name") } {
            Some(s) => s,
            None => return std::ptr::null_mut(),
        };
        let args_str = match unsafe { c_str_to_str(args_json, "args_json") } {
            Some(s) => s,
            None => return std::ptr::null_mut(),
        };

        // Parse args JSON array into CLIPS function call string
        let args: serde_json::Value = match serde_json::from_str(args_str) {
            Ok(v) => v,
            Err(e) => {
                error::set_last_error("invalid_argument", &format!("Invalid args JSON: {e}"), None);
                return std::ptr::null_mut();
            }
        };

        let args_clips = if let Some(arr) = args.as_array() {
            let parts: Vec<String> = arr
                .iter()
                .filter_map(|v| {
                    let s = serde_json::to_string(v).ok()?;
                    json_to_clips_value(&s).ok().map(|cv| cv.to_clips_string())
                })
                .collect();
            parts.join(" ")
        } else {
            String::new()
        };

        let call_str = format!("({name} {args_clips})");

        with_session!(session, std::ptr::null_mut(), |_entry, env| {
            match env.eval(&call_str) {
                Ok(value) => to_c_string_or_null(&clips_value_to_json(&value)),
                Err(e) => {
                    error::set_last_error(
                        "clips_error",
                        &format!("Function call failed: {e}"),
                        None,
                    );
                    std::ptr::null_mut()
                }
            }
        })
    }));
    result.unwrap_or(std::ptr::null_mut())
}

// ── JSON Rule Loading C ABI (T035) ─────────────────────────────────────

/// Load constructs from a JSON definition.
///
/// The JSON format supports:
/// ```json
/// {
///   "modules": [{"name": "mod-name"}],
///   "templates": [{"name": "t", "slots": [{"name": "s", "type": "INTEGER"}]}],
///   "rules": [{"name": "r", "source": "(defrule r ...)"}],
///   "facts": ["(t (s 1))"]
/// }
/// ```
///
/// Processing order: modules → templates → rules → facts.
/// Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_clips_session_load_json(session: u64, json: *const c_char) -> i32 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let json_str = match unsafe { c_str_to_str(json, "json") } {
            Some(s) => s,
            None => return -1,
        };

        let parsed: serde_json::Value = match serde_json::from_str(json_str) {
            Ok(v) => v,
            Err(e) => {
                error::set_last_error("invalid_argument", &format!("Invalid JSON: {e}"), None);
                return -1;
            }
        };

        let obj = match parsed.as_object() {
            Some(o) => o,
            None => {
                error::set_last_error("invalid_argument", "Expected JSON object", None);
                return -1;
            }
        };

        with_session!(session, -1, |_entry, env| {
            // 1. Modules
            if let Some(modules) = obj.get("modules").and_then(|v| v.as_array()) {
                for m in modules {
                    let name = match m.get("name").and_then(|v| v.as_str()) {
                        Some(n) => n,
                        None => {
                            error::set_last_error(
                                "invalid_argument",
                                "Module definition missing 'name'",
                                None,
                            );
                            return -1;
                        }
                    };
                    let defmodule = format!("(defmodule {name})");
                    if let Err(e) = env.build(&defmodule) {
                        error::set_last_error(
                            "clips_error",
                            &format!("Failed to build defmodule '{name}': {e}"),
                            None,
                        );
                        return -1;
                    }
                }
            }

            // 2. Templates
            if let Some(templates) = obj.get("templates").and_then(|v| v.as_array()) {
                for t in templates {
                    let construct = json_template_to_clips(t);
                    match construct {
                        Ok(clips_str) => {
                            if let Err(e) = env.build(&clips_str) {
                                let name = t.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                                error::set_last_error(
                                    "clips_error",
                                    &format!("Failed to build deftemplate '{name}': {e}"),
                                    None,
                                );
                                return -1;
                            }
                        }
                        Err(msg) => {
                            error::set_last_error("invalid_argument", &msg, None);
                            return -1;
                        }
                    }
                }
            }

            // 3. Rules
            if let Some(rules) = obj.get("rules").and_then(|v| v.as_array()) {
                for r in rules {
                    let source = match r.get("source").and_then(|v| v.as_str()) {
                        Some(s) => s,
                        None => {
                            let name = r.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                            error::set_last_error(
                                "invalid_argument",
                                &format!("Rule '{name}' missing 'source' field"),
                                None,
                            );
                            return -1;
                        }
                    };
                    if let Err(e) = env.build(source) {
                        let name = r.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                        error::set_last_error(
                            "clips_error",
                            &format!("Failed to build defrule '{name}': {e}"),
                            None,
                        );
                        return -1;
                    }
                }
            }

            // 4. Facts (assert after reset)
            if let Some(facts) = obj.get("facts").and_then(|v| v.as_array()) {
                for f in facts {
                    let fact_str = match f.as_str() {
                        Some(s) => s,
                        None => {
                            error::set_last_error(
                                "invalid_argument",
                                "Fact entries must be strings",
                                None,
                            );
                            return -1;
                        }
                    };
                    if let Err(e) = env.assert_string(fact_str) {
                        error::set_last_error(
                            "clips_error",
                            &format!("Failed to assert fact: {e}"),
                            None,
                        );
                        return -1;
                    }
                }
            }

            log::debug!("session_load_json: loaded JSON constructs into session {session}");
            0
        })
    }));
    result.unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in session_load_json", None);
        -1
    })
}

/// Convert a JSON template definition to a CLIPS deftemplate string.
fn json_template_to_clips(t: &serde_json::Value) -> Result<String, String> {
    let name = t
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Template definition missing 'name'".to_string())?;

    let module = t.get("module").and_then(|v| v.as_str());
    let qualified_name = if let Some(m) = module {
        format!("{m}::{name}")
    } else {
        name.to_string()
    };

    let mut parts = vec![format!("(deftemplate {qualified_name}")];

    if let Some(slots) = t.get("slots").and_then(|v| v.as_array()) {
        for slot in slots {
            let slot_name = slot
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Slot definition missing 'name'".to_string())?;

            let is_multislot = slot
                .get("multislot")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let keyword = if is_multislot { "multislot" } else { "slot" };

            let mut slot_parts = vec![format!("({keyword} {slot_name}")];

            if let Some(type_str) = slot.get("type").and_then(|v| v.as_str()) {
                slot_parts.push(format!("(type {type_str})"));
            }

            if let Some(default) = slot.get("default") {
                let default_clips = json_value_to_clips_literal(default);
                slot_parts.push(format!("(default {default_clips})"));
            }

            if let Some(allowed) = slot.get("allowed_values").and_then(|v| v.as_array()) {
                let vals: Vec<String> = allowed.iter().map(json_value_to_clips_literal).collect();
                slot_parts.push(format!("(allowed-values {})", vals.join(" ")));
            }

            if let Some(rmin) = slot.get("range_min").and_then(|v| v.as_f64()) {
                let rmax = slot
                    .get("range_max")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(f64::MAX);
                slot_parts.push(format!("(range {rmin} {rmax})"));
            }

            if let Some(cmin) = slot.get("cardinality_min").and_then(|v| v.as_u64()) {
                let cmax = slot
                    .get("cardinality_max")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(u64::MAX);
                slot_parts.push(format!("(cardinality {cmin} {cmax})"));
            }

            slot_parts.push(")".to_string());
            parts.push(format!("  {}", slot_parts.join(" ")));
        }
    }

    parts.push(")".to_string());
    Ok(parts.join("\n"))
}

/// Convert a serde_json::Value to a CLIPS literal for use in slot constraints.
fn json_value_to_clips_literal(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.to_string()
            } else if let Some(f) = n.as_f64() {
                format!("{f}")
            } else {
                n.to_string()
            }
        }
        serde_json::Value::String(s) => format!("\"{}\"", escape_json_value(s)),
        serde_json::Value::Bool(b) => {
            if *b {
                "TRUE".to_string()
            } else {
                "FALSE".to_string()
            }
        }
        serde_json::Value::Null => "nil".to_string(),
        serde_json::Value::Object(obj) => {
            // Handle typed ClipsValue objects: {"type":"symbol","value":"active"}
            if let (Some(t), Some(val)) =
                (obj.get("type").and_then(|v| v.as_str()), obj.get("value"))
            {
                match t {
                    "symbol" => val.as_str().unwrap_or("nil").to_string(),
                    "string" => format!("\"{}\"", escape_json_value(val.as_str().unwrap_or(""))),
                    "integer" => val.as_i64().map(|i| i.to_string()).unwrap_or_default(),
                    "float" => val.as_f64().map(|f| format!("{f}")).unwrap_or_default(),
                    _ => "nil".to_string(),
                }
            } else {
                "nil".to_string()
            }
        }
        serde_json::Value::Array(_) => "nil".to_string(),
    }
}

// ── Session Cache C ABI (T036) ─────────────────────────────────────────

/// Preload a named session with rules configuration.
/// The rules_json is stored and SHA-256 hashed for deduplication.
/// Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_clips_session_preload(
    name: *const c_char,
    rules_json: *const c_char,
) -> i32 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let name_str = match unsafe { c_str_to_str(name, "name") } {
            Some(s) => s,
            None => return -1,
        };
        let json_str = match unsafe { c_str_to_str(rules_json, "rules_json") } {
            Some(s) => s,
            None => return -1,
        };

        // Compute SHA-256 content hash
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(json_str.as_bytes());
        let content_hash = format!("{:x}", hasher.finalize());

        // Check for dedup — if same content hash exists under a different name,
        // just add an alias
        let mut cache = session_cache().lock();

        // Check if the content hash already exists
        if let Some(existing_name) = cache.hash_registry.get(&content_hash) {
            if existing_name == name_str {
                // Same name, same content — no-op
                log::debug!("session_preload: cache hit (identical content) for '{name_str}'");
                return 0;
            }
            // Different name but same content — store the rules_config under the new name too
            let rules_config = cache
                .cache
                .peek(existing_name)
                .map(|e| e.rules_config.clone());
            if let Some(config) = rules_config {
                cache.cache.put(
                    name_str.to_string(),
                    CachedRulebase {
                        rules_config: config,
                        content_hash: content_hash.clone(),
                        created_at: Instant::now(),
                    },
                );
                cache
                    .hash_registry
                    .insert(content_hash, name_str.to_string());
                log::debug!("session_preload: dedup alias '{name_str}' → same content");
                return 0;
            }
        }

        // Validate the JSON by parsing it
        if serde_json::from_str::<serde_json::Value>(json_str).is_err() {
            error::set_last_error("invalid_argument", "Invalid rules JSON", None);
            return -1;
        }

        // Store in cache
        cache.cache.put(
            name_str.to_string(),
            CachedRulebase {
                rules_config: json_str.to_string(),
                content_hash: content_hash.clone(),
                created_at: Instant::now(),
            },
        );
        cache
            .hash_registry
            .insert(content_hash, name_str.to_string());

        log::debug!("session_preload: cached '{name_str}'");
        0
    }));
    result.unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in session_preload", None);
        -1
    })
}

/// Retrieve a clone of a cached session. Creates a new independent session
/// pre-loaded with the cached rules. Returns session handle or 0 on error.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_clips_session_get_cached(name: *const c_char) -> u64 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let name_str = match unsafe { c_str_to_str(name, "name") } {
            Some(s) => s,
            None => return 0,
        };

        // Get the rules config from cache
        let rules_config = {
            let mut cache = session_cache().lock();
            cache
                .cache
                .get(name_str)
                .map(|entry| entry.rules_config.clone())
        };

        let rules_config = match rules_config {
            Some(c) => c,
            None => {
                error::set_last_error(
                    "clips_error",
                    &format!("No cached session named '{name_str}'"),
                    None,
                );
                return 0;
            }
        };

        // Create a new session and load the cached rules
        let env = match ClipsEnvironment::new() {
            Ok(e) => e,
            Err(e) => {
                error::set_last_error(
                    "clips_error",
                    &format!("Failed to create CLIPS environment for cache clone: {e}"),
                    None,
                );
                return 0;
            }
        };

        // Load the rules JSON into the new environment
        let parsed: serde_json::Value = match serde_json::from_str(&rules_config) {
            Ok(v) => v,
            Err(e) => {
                error::set_last_error(
                    "clips_error",
                    &format!("Failed to parse cached rules JSON: {e}"),
                    None,
                );
                return 0;
            }
        };

        if let Err(msg) = load_json_into_env(&env, &parsed) {
            error::set_last_error("clips_error", &msg, None);
            return 0;
        }

        // Register the new session
        let mut reg = registry().write();
        match reg.create(env, Some(format!("cache-clone:{name_str}"))) {
            Ok(key) => {
                let handle = key_to_u64(key);
                log::debug!("session_get_cached: cloned '{name_str}' → handle={handle}");
                handle
            }
            Err(msg) => {
                error::set_last_error("clips_error", &msg, None);
                0
            }
        }
    }));
    result.unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in session_get_cached", None);
        0
    })
}

/// Remove a cached session by name.
/// Returns 0 on success, -1 if not found.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_clips_session_cache_remove(name: *const c_char) -> i32 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let name_str = match unsafe { c_str_to_str(name, "name") } {
            Some(s) => s,
            None => return -1,
        };

        let mut cache = session_cache().lock();
        match cache.cache.pop(name_str) {
            Some(entry) => {
                // Also remove the hash registry entry if it points to this name
                cache.hash_registry.retain(|_, v| v != name_str);
                log::debug!(
                    "session_cache_remove: removed '{name_str}' (hash={})",
                    &entry.content_hash[..8]
                );
                0
            }
            None => {
                error::set_last_error(
                    "clips_error",
                    &format!("No cached session named '{name_str}'"),
                    None,
                );
                -1
            }
        }
    }));
    result.unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in session_cache_remove", None);
        -1
    })
}

/// Internal helper: load JSON constructs into an environment (without session registry).
fn load_json_into_env(env: &ClipsEnvironment, parsed: &serde_json::Value) -> Result<(), String> {
    let obj = parsed
        .as_object()
        .ok_or_else(|| "Expected JSON object".to_string())?;

    // 1. Modules
    if let Some(modules) = obj.get("modules").and_then(|v| v.as_array()) {
        for m in modules {
            let name = m
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Module definition missing 'name'".to_string())?;
            env.build(&format!("(defmodule {name})"))
                .map_err(|e| format!("Failed to build defmodule '{name}': {e}"))?;
        }
    }

    // 2. Templates
    if let Some(templates) = obj.get("templates").and_then(|v| v.as_array()) {
        for t in templates {
            let clips_str = json_template_to_clips(t)?;
            let name = t.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            env.build(&clips_str)
                .map_err(|e| format!("Failed to build deftemplate '{name}': {e}"))?;
        }
    }

    // 3. Rules
    if let Some(rules) = obj.get("rules").and_then(|v| v.as_array()) {
        for r in rules {
            let source = r.get("source").and_then(|v| v.as_str()).ok_or_else(|| {
                let name = r.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                format!("Rule '{name}' missing 'source' field")
            })?;
            let name = r.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            env.build(source)
                .map_err(|e| format!("Failed to build defrule '{name}': {e}"))?;
        }
    }

    // 4. Facts
    if let Some(facts) = obj.get("facts").and_then(|v| v.as_array()) {
        for f in facts {
            let fact_str = f
                .as_str()
                .ok_or_else(|| "Fact entries must be strings".to_string())?;
            env.assert_string(fact_str)
                .map_err(|e| format!("Failed to assert fact: {e}"))?;
        }
    }

    Ok(())
}

// ── Module & Focus Stack C ABI (T037) ──────────────────────────────────

/// Check whether a defmodule exists.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_clips_module_exists(session: u64, name: *const c_char) -> bool {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let name_str = match unsafe { c_str_to_str(name, "name") } {
            Some(s) => s,
            None => return false,
        };
        with_session!(session, false, |_entry, env| {
            matches!(env.find_module(name_str), Ok(Some(_)))
        })
    }));
    result.unwrap_or(false)
}

/// List all defmodule names. Returns JSON array. Caller frees.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_clips_module_list(session: u64) -> *mut c_char {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        with_session!(session, std::ptr::null_mut(), |_entry, env| {
            match env.list_module_names() {
                Ok(names) => {
                    let json = serde_json::to_string(&names).unwrap_or_else(|_| "[]".to_string());
                    to_c_string_or_null(&json)
                }
                Err(e) => {
                    error::set_last_error(
                        "clips_error",
                        &format!("Failed to list modules: {e}"),
                        None,
                    );
                    std::ptr::null_mut()
                }
            }
        })
    }));
    result.unwrap_or(std::ptr::null_mut())
}

/// Get the current module name. Returns allocated string. Caller frees.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_clips_module_current_get(session: u64) -> *mut c_char {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        with_session!(session, std::ptr::null_mut(), |_entry, env| {
            match env.get_current_module() {
                Some(module_handle) => match module_handle.name() {
                    Ok(name) => to_c_string_or_null(&name),
                    Err(e) => {
                        error::set_last_error(
                            "clips_error",
                            &format!("Failed to get current module name: {e}"),
                            None,
                        );
                        std::ptr::null_mut()
                    }
                },
                None => {
                    error::set_last_error("clips_error", "No current module", None);
                    std::ptr::null_mut()
                }
            }
        })
    }));
    result.unwrap_or(std::ptr::null_mut())
}

/// Set the current module. Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_clips_module_current_set(
    session: u64,
    name: *const c_char,
) -> i32 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let name_str = match unsafe { c_str_to_str(name, "name") } {
            Some(s) => s,
            None => return -1,
        };
        with_session!(session, -1, |_entry, env| {
            match env.find_module(name_str) {
                Ok(Some(module_handle)) => {
                    env.set_current_module(&module_handle);
                    0
                }
                Ok(None) => {
                    error::set_last_error(
                        "clips_error",
                        &format!("Module '{name_str}' not found"),
                        None,
                    );
                    -1
                }
                Err(e) => {
                    error::set_last_error(
                        "clips_error",
                        &format!("Failed to find module: {e}"),
                        None,
                    );
                    -1
                }
            }
        })
    }));
    result.unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in module_current_set", None);
        -1
    })
}

/// Push a module onto the focus stack.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_clips_focus_push(session: u64, module_name: *const c_char) -> i32 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let name_str = match unsafe { c_str_to_str(module_name, "module_name") } {
            Some(s) => s,
            None => return -1,
        };
        with_session!(session, -1, |_entry, env| {
            match env.find_module(name_str) {
                Ok(Some(module_handle)) => {
                    env.focus(&module_handle);
                    0
                }
                Ok(None) => {
                    error::set_last_error(
                        "clips_error",
                        &format!("Module '{name_str}' not found"),
                        None,
                    );
                    -1
                }
                Err(e) => {
                    error::set_last_error(
                        "clips_error",
                        &format!("Failed to find module: {e}"),
                        None,
                    );
                    -1
                }
            }
        })
    }));
    result.unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in focus_push", None);
        -1
    })
}

/// Get the module at the top of the focus stack. Returns module name or NULL.
/// Caller frees.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_clips_focus_get(session: u64) -> *mut c_char {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        with_session!(session, std::ptr::null_mut(), |_entry, env| {
            match env.get_focus() {
                Some(module_handle) => match module_handle.name() {
                    Ok(name) => to_c_string_or_null(&name),
                    Err(e) => {
                        error::set_last_error(
                            "clips_error",
                            &format!("Failed to get focus module name: {e}"),
                            None,
                        );
                        std::ptr::null_mut()
                    }
                },
                None => {
                    // Empty focus stack
                    std::ptr::null_mut()
                }
            }
        })
    }));
    result.unwrap_or(std::ptr::null_mut())
}

/// Pop the top module from the focus stack. Returns 0 or -1 if stack empty.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_clips_focus_pop(session: u64) -> i32 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        with_session!(session, -1, |_entry, env| {
            match env.pop_focus() {
                Some(_) => 0,
                None => {
                    error::set_last_error("clips_error", "Focus stack is empty", None);
                    -1
                }
            }
        })
    }));
    result.unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in focus_pop", None);
        -1
    })
}

/// Clear the entire focus stack. Returns 0 on success.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_clips_focus_clear(session: u64) -> i32 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        with_session!(session, -1, |_entry, env| {
            env.clear_focus_stack();
            0
        })
    }));
    result.unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in focus_clear", None);
        -1
    })
}

// ── Global Variables ──────────────────────────────────────────────────

/// Check if a global variable exists.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_clips_global_exists(session: u64, name: *const c_char) -> bool {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let name_str = match unsafe { CStr::from_ptr(name) }.to_str() {
            Ok(s) => s,
            Err(_) => return false,
        };
        with_session!(session, false, |_entry, env| {
            env.find_global(name_str)
                .map(|opt| opt.is_some())
                .unwrap_or(false)
        })
    }));
    result.unwrap_or(false)
}

/// Return all global variable names as a JSON array. Caller frees.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_clips_global_list(session: u64) -> *mut c_char {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        with_session!(session, std::ptr::null_mut(), |_entry, env| {
            let env_ptr = unsafe { env.raw() };
            let mut names = Vec::new();
            let mut global =
                unsafe { clips_sys::ffi::GetNextDefglobal(env_ptr, std::ptr::null_mut()) };
            while !global.is_null() {
                let name_ptr = unsafe { clips_sys::ffi::DefglobalName(global) };
                if !name_ptr.is_null()
                    && let Ok(name) = unsafe { CStr::from_ptr(name_ptr) }.to_str()
                {
                    names.push(name.to_string());
                }
                global = unsafe { clips_sys::ffi::GetNextDefglobal(env_ptr, global) };
            }
            match serde_json::to_string(&names) {
                Ok(json) => CString::new(json)
                    .map(|c| c.into_raw())
                    .unwrap_or(std::ptr::null_mut()),
                Err(_) => std::ptr::null_mut(),
            }
        })
    }));
    result.unwrap_or(std::ptr::null_mut())
}

/// Get the value of a global variable as JSON. Caller frees.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_clips_global_get_value(session: u64, name: *const c_char) -> *mut c_char {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let name_str = match unsafe { CStr::from_ptr(name) }.to_str() {
            Ok(s) => s,
            Err(_) => {
                error::set_last_error("invalid_argument", "Invalid global name", None);
                return std::ptr::null_mut();
            }
        };
        with_session!(session, std::ptr::null_mut(), |_entry, env| {
            match env.find_global(name_str) {
                Ok(Some(handle)) => match handle.get_value() {
                    Ok(value) => {
                        let json = clips_value_to_json(&value);
                        CString::new(json.to_string())
                            .map(|c| c.into_raw())
                            .unwrap_or(std::ptr::null_mut())
                    }
                    Err(e) => {
                        error::set_last_error(
                            "clips_error",
                            &format!("Get global value failed: {e}"),
                            None,
                        );
                        std::ptr::null_mut()
                    }
                },
                Ok(None) => {
                    error::set_last_error(
                        "clips_error",
                        &format!("Global not found: {name_str}"),
                        None,
                    );
                    std::ptr::null_mut()
                }
                Err(e) => {
                    error::set_last_error("clips_error", &format!("Find global failed: {e}"), None);
                    std::ptr::null_mut()
                }
            }
        })
    }));
    result.unwrap_or(std::ptr::null_mut())
}

/// Set the value of a global variable from a JSON value. Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_clips_global_set_value(
    session: u64,
    name: *const c_char,
    value_json: *const c_char,
) -> i32 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let name_str = match unsafe { CStr::from_ptr(name) }.to_str() {
            Ok(s) => s,
            Err(_) => {
                error::set_last_error("invalid_argument", "Invalid global name", None);
                return -1;
            }
        };
        let val_str = match unsafe { CStr::from_ptr(value_json) }.to_str() {
            Ok(s) => s,
            Err(_) => {
                error::set_last_error("invalid_argument", "Invalid value JSON", None);
                return -1;
            }
        };
        let clips_val = match json_to_clips_value(val_str) {
            Ok(v) => v,
            Err(e) => {
                error::set_last_error("invalid_argument", &format!("Invalid value: {e}"), None);
                return -1;
            }
        };
        with_session!(session, -1, |_entry, env| {
            match env.find_global(name_str) {
                Ok(Some(handle)) => match handle.set_value(&clips_val) {
                    Ok(()) => 0,
                    Err(e) => {
                        error::set_last_error(
                            "clips_error",
                            &format!("Set global value failed: {e}"),
                            None,
                        );
                        -1
                    }
                },
                Ok(None) => {
                    error::set_last_error(
                        "clips_error",
                        &format!("Global not found: {name_str}"),
                        None,
                    );
                    -1
                }
                Err(e) => {
                    error::set_last_error("clips_error", &format!("Find global failed: {e}"), None);
                    -1
                }
            }
        })
    }));
    result.unwrap_or(-1)
}

// ── Watch & Diagnostics ───────────────────────────────────────────────

/// Parse a watch item string to WatchItem enum.
fn parse_watch_item(s: &str) -> Option<clips_sys::WatchItem> {
    match s.to_lowercase().as_str() {
        "facts" => Some(clips_sys::WatchItem::Facts),
        "rules" => Some(clips_sys::WatchItem::Rules),
        "activations" => Some(clips_sys::WatchItem::Activations),
        "compilations" => Some(clips_sys::WatchItem::Compilations),
        "statistics" => Some(clips_sys::WatchItem::Statistics),
        "globals" => Some(clips_sys::WatchItem::Globals),
        "deffunctions" => Some(clips_sys::WatchItem::Deffunctions),
        "instances" => Some(clips_sys::WatchItem::Instances),
        "slots" => Some(clips_sys::WatchItem::Slots),
        "messages" => Some(clips_sys::WatchItem::Messages),
        "message-handlers" => Some(clips_sys::WatchItem::MessageHandlers),
        "generic-functions" => Some(clips_sys::WatchItem::GenericFunctions),
        "methods" => Some(clips_sys::WatchItem::Methods),
        "focus" => Some(clips_sys::WatchItem::Focus),
        "all" => Some(clips_sys::WatchItem::All),
        _ => None,
    }
}

/// Enable watching for the specified item. Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_clips_watch(session: u64, item: *const c_char) -> i32 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let item_str = match unsafe { CStr::from_ptr(item) }.to_str() {
            Ok(s) => s,
            Err(_) => {
                error::set_last_error("invalid_argument", "Invalid watch item", None);
                return -1;
            }
        };
        let watch_item = match parse_watch_item(item_str) {
            Some(w) => w,
            None => {
                error::set_last_error(
                    "invalid_argument",
                    &format!("Unknown watch item: {item_str}"),
                    None,
                );
                return -1;
            }
        };
        with_session!(session, -1, |_entry, env| {
            match env.watch(watch_item) {
                Ok(()) => 0,
                Err(e) => {
                    error::set_last_error("clips_error", &format!("Watch failed: {e}"), None);
                    -1
                }
            }
        })
    }));
    result.unwrap_or(-1)
}

/// Disable watching for the specified item. Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_clips_unwatch(session: u64, item: *const c_char) -> i32 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let item_str = match unsafe { CStr::from_ptr(item) }.to_str() {
            Ok(s) => s,
            Err(_) => {
                error::set_last_error("invalid_argument", "Invalid watch item", None);
                return -1;
            }
        };
        let watch_item = match parse_watch_item(item_str) {
            Some(w) => w,
            None => {
                error::set_last_error(
                    "invalid_argument",
                    &format!("Unknown watch item: {item_str}"),
                    None,
                );
                return -1;
            }
        };
        with_session!(session, -1, |_entry, env| {
            match env.unwatch(watch_item) {
                Ok(()) => 0,
                Err(e) => {
                    error::set_last_error("clips_error", &format!("Unwatch failed: {e}"), None);
                    -1
                }
            }
        })
    }));
    result.unwrap_or(-1)
}

/// Start recording all CLIPS output to a file. Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_clips_dribble_on(session: u64, path: *const c_char) -> i32 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let path_str = match unsafe { CStr::from_ptr(path) }.to_str() {
            Ok(s) => s,
            Err(_) => {
                error::set_last_error("invalid_argument", "Invalid path", None);
                return -1;
            }
        };
        with_session!(session, -1, |_entry, env| {
            match env.dribble_on(path_str) {
                Ok(()) => 0,
                Err(e) => {
                    error::set_last_error("clips_error", &format!("Dribble on failed: {e}"), None);
                    -1
                }
            }
        })
    }));
    result.unwrap_or(-1)
}

/// Stop recording CLIPS output. Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_clips_dribble_off(session: u64) -> i32 {
    error::clear_last_error();
    let result = catch_unwind(AssertUnwindSafe(|| {
        with_session!(session, -1, |_entry, env| {
            match env.dribble_off() {
                Ok(()) => 0,
                Err(e) => {
                    error::set_last_error("clips_error", &format!("Dribble off failed: {e}"), None);
                    -1
                }
            }
        })
    }));
    result.unwrap_or(-1)
}
