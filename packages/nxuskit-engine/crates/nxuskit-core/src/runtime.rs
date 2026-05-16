use std::sync::OnceLock;

static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

/// Get or lazily initialize the shared tokio runtime.
///
/// The runtime is multi-threaded and lives for the process lifetime.
/// Panics only if the runtime cannot be built (extremely unlikely).
pub(crate) fn get_runtime() -> &'static tokio::runtime::Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("nxuskit-worker")
            .build()
            .expect("failed to create tokio runtime")
    })
}
