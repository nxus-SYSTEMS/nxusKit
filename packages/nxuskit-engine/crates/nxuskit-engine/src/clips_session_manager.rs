//! CLIPS Session Manager — shared Rust-native session lifecycle.
//!
//! Provides a typed session manager over `clips_sys::ClipsEnvironment` using
//! slotmap generational keys (u64 handles). This module is used by:
//!
//! - The CLIPS provider (internal engine dogfooding)
//! - `nxuskit-core`'s C ABI layer (external SDK surface)
//!
//! All CLIPS environment access goes through this single manager, ensuring
//! one session registry, one lifecycle policy, and one set of concurrency
//! guarantees.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use parking_lot::{Mutex, RwLock};
use slotmap::{SlotMap, new_key_type};

use clips_sys::ClipsEnvironment;

// ── Session Key ────────────────────────────────────────────────────────

new_key_type! {
    /// Opaque session handle. The generational index prevents use-after-free:
    /// a destroyed session's key will not match any future session.
    pub(crate) struct SessionKey;
}

/// Serialize a `SessionKey` to `u64` for C ABI / cross-module use.
pub(crate) fn key_to_u64(key: SessionKey) -> u64 {
    key.0.as_ffi()
}

/// Deserialize a `u64` back to a `SessionKey`.
pub(crate) fn u64_to_key(handle: u64) -> SessionKey {
    SessionKey::from(slotmap::KeyData::from_ffi(handle))
}

// ── Session Entry ──────────────────────────────────────────────────────

/// An active CLIPS inference session managed by the session registry.
#[allow(missing_docs, missing_debug_implementations)]
pub struct SessionEntry {
    /// The underlying CLIPS environment.
    environment: ClipsEnvironment,
    /// Optional human-readable name (set for cached sessions).
    #[allow(dead_code)]
    name: Option<String>,
    /// Timestamp of session creation.
    #[allow(dead_code)]
    created_at: Instant,
    /// Thread-safe halt signal.
    halt_flag: AtomicBool,
    /// Single-owner enforcement. Operations acquire with `try_lock()`.
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

    /// Get a reference to the underlying CLIPS environment.
    pub fn environment(&self) -> &ClipsEnvironment {
        &self.environment
    }

    /// Get the halt flag for thread-safe signalling.
    pub fn halt_flag(&self) -> &AtomicBool {
        &self.halt_flag
    }

    /// Try to acquire the single-owner lock. Returns the guard on success.
    pub fn try_lock(&self) -> Option<parking_lot::MutexGuard<'_, ()>> {
        self.lock.try_lock()
    }

    /// Signal halt (from another thread).
    pub fn signal_halt(&self) {
        self.halt_flag.store(true, Ordering::SeqCst);
    }

    /// Clear the halt flag.
    pub fn clear_halt(&self) {
        self.halt_flag.store(false, Ordering::SeqCst);
    }
}

// ── Session Registry ───────────────────────────────────────────────────

/// Default maximum concurrent sessions.
const DEFAULT_MAX_SESSIONS: usize = 64;

/// Global registry of all active sessions.
#[allow(missing_docs)]
pub(crate) struct SessionRegistry {
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
}

/// Global session registry instance, protected by RwLock.
pub(crate) fn registry() -> &'static RwLock<SessionRegistry> {
    static REGISTRY: OnceLock<RwLock<SessionRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(|| RwLock::new(SessionRegistry::new(DEFAULT_MAX_SESSIONS)))
}

// ── Public API ─────────────────────────────────────────────────────────

/// Create a new CLIPS session. Returns a u64 handle or an error string.
pub fn session_create() -> Result<u64, String> {
    let env = ClipsEnvironment::new().map_err(|e| format!("Failed to create environment: {e}"))?;
    let mut reg = registry().write();
    if reg.sessions.len() >= reg.max_sessions {
        return Err(format!(
            "Maximum concurrent sessions ({}) reached",
            reg.max_sessions
        ));
    }
    let entry = SessionEntry::new(env, None);
    let key = reg.sessions.insert(entry);
    Ok(key_to_u64(key))
}

/// Create a new session from an existing `ClipsEnvironment`.
/// Used by the provider when it needs to manage its own environment creation.
pub fn session_create_from_env(env: ClipsEnvironment, name: Option<String>) -> Result<u64, String> {
    let mut reg = registry().write();
    if reg.sessions.len() >= reg.max_sessions {
        return Err(format!(
            "Maximum concurrent sessions ({}) reached",
            reg.max_sessions
        ));
    }
    let entry = SessionEntry::new(env, name);
    let key = reg.sessions.insert(entry);
    Ok(key_to_u64(key))
}

/// Destroy a session by handle. Returns true if it existed.
pub fn session_destroy(handle: u64) -> bool {
    let key = u64_to_key(handle);
    let mut reg = registry().write();
    reg.sessions.remove(key).is_some()
}

/// Execute a closure with a read-lock reference to a session entry.
/// Returns `None` if the session does not exist.
pub fn with_session<F, R>(handle: u64, f: F) -> Option<R>
where
    F: FnOnce(&SessionEntry) -> R,
{
    let key = u64_to_key(handle);
    let reg = registry().read();
    reg.sessions.get(key).map(f)
}

/// Execute a closure with a read-lock reference to the session's environment.
/// Returns `Err` if the session does not exist.
pub fn with_env<F, R>(handle: u64, f: F) -> Result<R, String>
where
    F: FnOnce(&ClipsEnvironment) -> R,
{
    let key = u64_to_key(handle);
    let reg = registry().read();
    match reg.sessions.get(key) {
        Some(entry) => Ok(f(&entry.environment)),
        None => Err("Invalid session handle".to_string()),
    }
}

/// Execute a fallible closure with the session's environment.
/// Returns `Err` if the session does not exist or the closure returns an error.
pub fn with_env_result<F, R, E>(handle: u64, f: F) -> Result<R, String>
where
    F: FnOnce(&ClipsEnvironment) -> Result<R, E>,
    E: std::fmt::Display,
{
    let key = u64_to_key(handle);
    let reg = registry().read();
    match reg.sessions.get(key) {
        Some(entry) => f(&entry.environment).map_err(|e| e.to_string()),
        None => Err("Invalid session handle".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_destroy() {
        let handle = session_create().expect("create should succeed");
        assert_ne!(handle, 0);
        assert!(session_destroy(handle));
        // Double destroy returns false
        assert!(!session_destroy(handle));
    }

    #[test]
    fn test_with_env() {
        let handle = session_create().expect("create should succeed");
        let result = with_env(handle, |env| {
            let modules = env.list_module_names().unwrap_or_default();
            modules.contains(&"MAIN".to_string())
        });
        assert!(result.is_ok());
        assert!(result.unwrap());
        session_destroy(handle);
    }

    #[test]
    fn test_invalid_handle() {
        let result = with_env(999999, |_| ());
        assert!(result.is_err());
    }
}
