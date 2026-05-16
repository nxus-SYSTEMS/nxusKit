//! Stub parity test: ensures every FFI function declared in ffi.rs
//! has a matching stub in build.rs `create_stub_library()`.
//!
//! Windows builds use stub libraries when CLIPS source is unavailable.
//! If a new FFI function is added to ffi.rs without a corresponding stub,
//! the Windows linker will fail with "unresolved external symbol".
//!
//! This test prevents that by parsing both files and comparing.
#![allow(clippy::panic, clippy::print_stdout, clippy::print_stderr)]

use std::collections::BTreeSet;

/// Extract function names from `extern "C" { ... }` blocks in ffi.rs.
///
/// Only extracts functions inside `unsafe extern "C" { ... }` blocks,
/// ignoring `impl` blocks and standalone functions.
///
/// Handles two patterns:
/// 1. Normal: `pub fn FunctionName(...)` -> extracts "FunctionName"
/// 2. With link_name: `#[link_name = "real_name"]` followed by `pub fn RustName(...)` -> extracts "real_name"
fn extract_ffi_function_names(ffi_source: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut pending_link_name: Option<String> = None;
    let mut in_extern_block = false;
    let mut brace_depth: i32 = 0;

    for line in ffi_source.lines() {
        let trimmed = line.trim();

        // Detect entry into an extern "C" block
        if !in_extern_block {
            if trimmed.contains("extern \"C\"") && trimmed.contains('{') {
                in_extern_block = true;
                brace_depth = 1;
                continue;
            } else if trimmed.contains("extern \"C\"") {
                // The opening brace may be on the next line
                in_extern_block = true;
                brace_depth = 0;
                continue;
            }
            // Not in an extern block, skip this line
            continue;
        }

        // Track brace depth to find end of extern block
        for ch in trimmed.chars() {
            if ch == '{' {
                brace_depth += 1;
            } else if ch == '}' {
                brace_depth -= 1;
                if brace_depth <= 0 {
                    in_extern_block = false;
                    break;
                }
            }
        }

        if !in_extern_block {
            pending_link_name = None;
            continue;
        }

        // Check for #[link_name = "..."]
        if trimmed.starts_with("#[link_name") {
            if let Some(start) = trimmed.find('"')
                && let Some(end) = trimmed[start + 1..].find('"')
            {
                pending_link_name = Some(trimmed[start + 1..start + 1 + end].to_string());
            }
            continue;
        }

        // Check for `pub fn FunctionName(`
        if let Some(rest) = trimmed.strip_prefix("pub fn ") {
            // skip "pub fn "
            if let Some(paren) = rest.find('(') {
                let fn_name = rest[..paren].trim();
                if !fn_name.is_empty() {
                    if let Some(link_name) = pending_link_name.take() {
                        names.insert(link_name);
                    } else {
                        names.insert(fn_name.to_string());
                    }
                }
            }
            // Reset pending link_name even if we didn't use it
            pending_link_name = None;
            continue;
        }

        // If we see a non-attribute, non-comment, non-empty line, clear pending_link_name
        if !trimmed.is_empty()
            && !trimmed.starts_with("//")
            && !trimmed.starts_with('#')
            && !trimmed.starts_with("///")
        {
            pending_link_name = None;
        }
    }

    names
}

/// Extract stub function names from `create_stub_library()` in build.rs.
///
/// Looks for C function definition patterns:
///   - `type FunctionName(` at the start of a C string literal
///   - Handles pointer returns like `Type* FunctionName(`
///   - Handles `void FunctionName(`
///   - Handles `const char* FunctionName(`
fn extract_stub_function_names(build_source: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();

    // Find the create_stub_library function body
    let Some(start_idx) = build_source.find("fn create_stub_library") else {
        return names;
    };
    let stub_source = &build_source[start_idx..];

    // We look for C function definitions inside string literals.
    // Patterns like:
    //   Environment* CreateEnvironment(void)
    //   void Reset(Environment* env)
    //   int Load(Environment* env, const char* filename)
    //   long long clips_cv_to_integer(CLIPSValue* cv)
    //   const char* clips_cv_to_string(CLIPSValue* cv)
    //
    // The function name is the last identifier before the '(' that is NOT
    // a C type keyword and NOT preceded by '*'.
    //
    // Strategy: find all `word(` patterns where word is the function name.
    // In C, the function name is the identifier immediately before `(`.

    for line in stub_source.lines() {
        // Only look at lines that contain writeln! with function definitions
        // Skip typedef lines, comment lines, and the writeln macro boilerplate
        let trimmed = line.trim();

        // Must contain a '(' to be a function definition
        if !trimmed.contains('(') {
            continue;
        }

        // Skip Rust code (writeln!, let, fn, if, etc.) by looking inside the string
        // Extract the content inside the string literal (between quotes or raw strings)
        // The pattern in build.rs is: writeln!(file, "C_CODE_HERE").unwrap();
        let content = if let Some(first_quote) = trimmed.find('"') {
            let after_first = &trimmed[first_quote + 1..];
            // Find the matching closing quote (before .unwrap() or end)
            // Handle escaped quotes by looking for the pattern
            if let Some(last_quote) = after_first.rfind('"') {
                &after_first[..last_quote]
            } else {
                continue;
            }
        } else {
            continue;
        };

        // Skip typedefs and pure comments
        if content.starts_with("typedef ") || content.starts_with("//") {
            continue;
        }

        // Now parse the C function definition from content
        // Find the function name: it's the identifier right before the first '('
        if let Some(paren_pos) = content.find('(') {
            let before_paren = content[..paren_pos].trim_end();
            // The function name is the last word (after space or *)
            let fn_name = before_paren.rsplit([' ', '*']).next().unwrap_or("").trim();

            // Validate: must be a valid C identifier
            if !fn_name.is_empty()
                && fn_name.chars().all(|c| c.is_alphanumeric() || c == '_')
                && fn_name
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_alphabetic() || c == '_')
            {
                // Skip C keywords that aren't function names
                let keywords = [
                    "if", "else", "for", "while", "return", "void", "int", "long", "char", "bool",
                    "const", "unsigned", "short", "double", "float", "size_t", "sizeof", "struct",
                    "enum", "typedef", "static", "extern",
                ];
                if !keywords.contains(&fn_name) {
                    names.insert(fn_name.to_string());
                }
            }
        }
    }

    names
}

#[test]
fn every_ffi_function_has_a_stub() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");

    let ffi_path = format!("{}/src/ffi.rs", manifest_dir);
    let build_path = format!("{}/build.rs", manifest_dir);

    let ffi_source = std::fs::read_to_string(&ffi_path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", ffi_path, e));
    let build_source = std::fs::read_to_string(&build_path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", build_path, e));

    let ffi_functions = extract_ffi_function_names(&ffi_source);
    let stub_functions = extract_stub_function_names(&build_source);

    // Find functions declared in ffi.rs but missing from stubs
    let missing: BTreeSet<_> = ffi_functions.difference(&stub_functions).collect();

    if !missing.is_empty() {
        let missing_list: Vec<_> = missing.iter().map(|s| s.as_str()).collect();
        panic!(
            "The following {} FFI function(s) declared in ffi.rs have no matching stub in \
             build.rs create_stub_library():\n\n  {}\n\n\
             Add stubs for these functions to prevent Windows linker failures.\n\
             Total FFI functions: {}, Stubs present: {}, Missing: {}",
            missing_list.len(),
            missing_list.join("\n  "),
            ffi_functions.len(),
            stub_functions.len(),
            missing_list.len(),
        );
    }

    // Also report any stubs that don't correspond to FFI declarations (stale stubs)
    let stale: BTreeSet<_> = stub_functions.difference(&ffi_functions).collect();
    if !stale.is_empty() {
        let stale_list: Vec<_> = stale.iter().map(|s| s.as_str()).collect();
        eprintln!(
            "Note: {} stub(s) in build.rs have no matching FFI declaration in ffi.rs \
             (may be stale):\n  {}",
            stale_list.len(),
            stale_list.join("\n  "),
        );
    }

    eprintln!(
        "Stub parity check passed: {} FFI functions, {} stubs",
        ffi_functions.len(),
        stub_functions.len()
    );
}

#[test]
fn extract_ffi_names_handles_link_name() {
    let source = r#"
impl Foo {
    pub fn new() -> Self { Foo {} }
}

unsafe extern "C" {
    pub fn CreateEnvironment() -> *mut Environment;

    #[link_name = "clips_cv_type"]
    pub fn CVType(value: *mut CLIPSValue) -> c_int;

    pub fn Run(env: *mut Environment, limit: c_long) -> c_long;
}

pub fn standalone_function() {}
    "#;

    let names = extract_ffi_function_names(source);
    assert!(names.contains("CreateEnvironment"));
    assert!(names.contains("clips_cv_type"));
    assert!(names.contains("Run"));
    // Should NOT contain the Rust alias "CVType"
    assert!(!names.contains("CVType"));
    // Should NOT contain impl methods or standalone functions
    assert!(
        !names.contains("new"),
        "impl method 'new' should be excluded"
    );
    assert!(
        !names.contains("standalone_function"),
        "standalone fn should be excluded"
    );
    assert_eq!(names.len(), 3);
}

#[test]
fn extract_stub_names_parses_c_functions() {
    let source = r#"
fn create_stub_library(out_dir: &Path) {
    writeln!(file, "Environment* CreateEnvironment(void) {{ return (void*)0; }}").unwrap();
    writeln!(file, "int Load(Environment* env, const char* filename) {{ return 0; }}").unwrap();
    writeln!(file, "void Reset(Environment* env) {{}}").unwrap();
    writeln!(file, "typedef void Environment;").unwrap();
    writeln!(file, "const char* DefruleName(Defrule* rule) {{ return \"\"; }}").unwrap();
}
    "#;

    let names = extract_stub_function_names(source);
    assert!(
        names.contains("CreateEnvironment"),
        "missing CreateEnvironment"
    );
    assert!(names.contains("Load"), "missing Load");
    assert!(names.contains("Reset"), "missing Reset");
    assert!(names.contains("DefruleName"), "missing DefruleName");
    // typedefs should NOT appear
    assert!(
        !names.contains("Environment"),
        "typedef leaked as function name"
    );
    assert_eq!(names.len(), 4);
}
