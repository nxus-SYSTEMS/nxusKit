/// Build script for nxuskit.
///
/// For `static-link` feature: discovers the SDK library directory and emits
/// linker directives so Cargo can statically link `libnxuskit`.
///
/// For `dynamic-link` feature (default): no build-time linking required —
/// the library is loaded at runtime via `libloading`.
fn main() {
    // Re-run if these env vars change.
    println!("cargo::rerun-if-env-changed=NXUSKIT_SDK_DIR");
    println!("cargo::rerun-if-env-changed=NXUSKIT_LIB_DIR");

    #[cfg(feature = "static-link")]
    {
        let lib_dir = find_lib_dir();
        println!("cargo::rustc-link-search=native={lib_dir}");
        println!("cargo::rustc-link-lib=static=nxuskit");

        // Platform-specific system libraries required by libnxuskit.
        if cfg!(target_os = "linux") {
            println!("cargo::rustc-link-lib=dylib=pthread");
            println!("cargo::rustc-link-lib=dylib=dl");
            println!("cargo::rustc-link-lib=dylib=m");
        } else if cfg!(target_os = "macos") {
            println!("cargo::rustc-link-lib=framework=Security");
            println!("cargo::rustc-link-lib=framework=SystemConfiguration");
        } else if cfg!(target_os = "windows") {
            println!("cargo::rustc-link-lib=dylib=ws2_32");
            println!("cargo::rustc-link-lib=dylib=bcrypt");
            println!("cargo::rustc-link-lib=dylib=userenv");
            println!("cargo::rustc-link-lib=dylib=ntdll");
        }
    }
}

/// Locate the SDK library directory from environment variables or platform defaults.
#[cfg(feature = "static-link")]
fn find_lib_dir() -> String {
    // Priority 1: Explicit lib directory.
    if let Ok(dir) = std::env::var("NXUSKIT_LIB_DIR") {
        return dir;
    }

    // Priority 2: SDK root with /lib subdirectory.
    if let Ok(sdk) = std::env::var("NXUSKIT_SDK_DIR") {
        return format!("{sdk}/lib");
    }

    // Priority 3: Platform defaults.
    if cfg!(target_os = "macos") {
        "/usr/local/lib".to_string()
    } else if cfg!(target_os = "windows") {
        "C:\\nxuskit\\lib".to_string()
    } else {
        "/usr/lib".to_string()
    }
}
