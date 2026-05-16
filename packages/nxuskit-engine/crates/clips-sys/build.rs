//! Build script for clips-sys
//!
//! This script compiles the CLIPS C source code and links it statically.
//!
//! CLIPS source code is searched for in the following locations (in order):
//! 1. `CLIPS_SOURCE_DIR` environment variable (if set — must be valid, no silent fallback)
//! 2. `../../../../internal/CLIPS/clips_core_source_642/core/` (in-tree fallback)
//! 3. `clips-source/` directory (local to this crate, used by CI download)
//!
//! If not found, the script will attempt to download CLIPS 6.4 from SourceForge.
//!
//! Download manually from: https://sourceforge.net/projects/clipsrules/files/CLIPS/6.4/
//!
//! CLIPS 6.4.2 is licensed under MIT No Attribution (MIT-0).
//! See: https://www.clipsrules.net/

use std::env;
use std::path::{Path, PathBuf};

/// SourceForge download URL for CLIPS 6.4.2 source (manual download required)
const CLIPS_DOWNLOAD_URL: &str =
    "https://sourceforge.net/projects/clipsrules/files/CLIPS/6.4.2/clips_core_source_642.tar.gz";

fn main() {
    // Always rebuild when build.rs itself changes (ensures stub updates are picked up)
    println!("cargo:rerun-if-changed=build.rs");

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    // Try to find CLIPS source in various locations
    let clips_source = find_clips_source(&manifest_dir);

    let clips_source = match clips_source {
        Some(path) => {
            println!("cargo:warning=Using CLIPS source from: {}", path.display());
            path
        }
        None => {
            println!("cargo:warning=CLIPS source not found!");
            println!("cargo:warning=");
            println!("cargo:warning=Download CLIPS 6.4 source from:");
            println!("cargo:warning=  {}", CLIPS_DOWNLOAD_URL);
            println!("cargo:warning=");
            println!("cargo:warning=Extract to one of these locations:");
            println!(
                "cargo:warning=  1. Set CLIPS_SOURCE_DIR environment variable to the core/ directory"
            );
            println!(
                "cargo:warning=  2. ../../../internal/CLIPS/clips_core_source_642/core/ (internal development)"
            );
            println!(
                "cargo:warning=  3. ../../../ex-repo-refs/CLIPS/clips_core_source_642/core/ (legacy external)"
            );
            println!("cargo:warning=  4. clips-source/ (local to this crate)");
            println!("cargo:warning=");
            println!("cargo:warning=Using stub library - CLIPS functionality will not work!");

            // Create a stub library for development without CLIPS source
            create_stub_library(&out_dir);
            return;
        }
    };

    // Collect all C source files
    let c_files: Vec<PathBuf> = std::fs::read_dir(&clips_source)
        .expect("Failed to read CLIPS source directory")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().map(|e| e == "c").unwrap_or(false))
        .collect();

    if c_files.is_empty() {
        println!(
            "cargo:warning=No C source files found in {}",
            clips_source.display()
        );
        create_stub_library(&out_dir);
        return;
    }

    println!(
        "cargo:warning=Compiling {} CLIPS source files",
        c_files.len()
    );
    println!("cargo:rerun-if-changed={}", clips_source.display());
    println!("cargo:rerun-if-env-changed=CLIPS_SOURCE_DIR");

    // Also compile our wrapper file
    let wrapper_file = manifest_dir.join("src/clips_wrapper.c");
    println!("cargo:rerun-if-changed={}", wrapper_file.display());

    // Build CLIPS library
    let mut build = cc::Build::new();

    build
        .files(&c_files)
        .file(&wrapper_file)
        .include(&clips_source)
        // CLIPS compilation flags
        .define("GENERIC", None)
        // Enable all constructs
        .define("DEFRULE_CONSTRUCT", Some("1"))
        .define("DEFTEMPLATE_CONSTRUCT", Some("1"))
        .define("DEFFACTS_CONSTRUCT", Some("1"))
        .define("DEFGLOBAL_CONSTRUCT", Some("1"))
        .define("DEFFUNCTION_CONSTRUCT", Some("1"))
        .define("DEFGENERIC_CONSTRUCT", Some("1"))
        .define("DEFMETHOD_CONSTRUCT", Some("1"))
        // Enable COOL (object system)
        .define("OBJECT_SYSTEM", Some("1"))
        // Optimization
        .opt_level(2)
        // Suppress warnings from CLIPS code
        .warnings(false);

    // Platform-specific settings
    #[cfg(target_os = "linux")]
    {
        build.define("LINUX", None);
    }

    #[cfg(target_os = "macos")]
    {
        build.define("DARWIN", None);
    }

    #[cfg(target_os = "windows")]
    {
        build.define("WIN_MVC", None);
    }

    build.compile("clips");

    println!("cargo:rustc-link-lib=static=clips");
    println!("cargo:rustc-link-search=native={}", out_dir.display());
}

/// Search for CLIPS source in various locations
fn find_clips_source(manifest_dir: &Path) -> Option<PathBuf> {
    // 1. Check CLIPS_SOURCE_DIR environment variable
    //    If set, it MUST be valid — fail immediately if not (no silent fallback)
    if let Ok(dir) = env::var("CLIPS_SOURCE_DIR") {
        let path = PathBuf::from(&dir);
        if path.exists() && has_clips_sources(&path) {
            return Some(path);
        }
        // Maybe it's the parent dir containing core/
        let core_path = path.join("core");
        if core_path.exists() && has_clips_sources(&core_path) {
            return Some(core_path);
        }
        // CLIPS_SOURCE_DIR was set but invalid — fail fast
        panic!(
            "CLIPS_SOURCE_DIR is set to '{}' but does not contain valid CLIPS source files (*.c, *.h). \
             Ensure the directory exists and contains CLIPS 6.4.2 core source.",
            dir
        );
    }

    // 2. Check internal development location (in-tree fallback)
    let internal_path = manifest_dir.join("../../../../internal/CLIPS/clips_core_source_642/core");
    if internal_path.exists() && has_clips_sources(&internal_path) {
        // On Windows, avoid canonicalize() as it creates UNC paths that MSVC compiler doesn't handle well
        #[cfg(windows)]
        {
            return Some(internal_path);
        }
        #[cfg(not(windows))]
        {
            return Some(internal_path.canonicalize().unwrap_or(internal_path));
        }
    }

    // 3. Check local clips-source/ directory (CI download location)
    let local_path = manifest_dir.join("clips-source");
    if local_path.exists() && has_clips_sources(&local_path) {
        return Some(local_path);
    }

    None
}

/// Check if a directory contains CLIPS source files
fn has_clips_sources(dir: &Path) -> bool {
    // Check for key CLIPS source files
    let key_files = ["clips.h", "envrnmnt.c", "factmngr.c"];

    for file in &key_files {
        if dir.join(file).exists() {
            return true;
        }
    }

    // Also check if there are any .c files at all
    if let Ok(entries) = std::fs::read_dir(dir) {
        let c_count = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map(|ext| ext == "c").unwrap_or(false))
            .count();
        return c_count > 10; // CLIPS has many source files
    }

    false
}

/// Create a stub library when CLIPS source is not available.
/// This allows the crate to compile for development/documentation purposes.
///
/// IMPORTANT: Every function declared in `src/ffi.rs` must have a corresponding
/// stub here. The test in `tests/stub_parity_test.rs` enforces this.
/// Functions with `#[link_name = "xxx"]` in ffi.rs need stubs named "xxx".
fn create_stub_library(out_dir: &Path) {
    use std::fs::File;
    use std::io::Write;

    let stub_c = out_dir.join("clips_stub.c");
    let mut file = File::create(&stub_c).expect("Failed to create stub file");

    writeln!(file, "// Stub implementation - CLIPS source not available").unwrap();
    writeln!(
        file,
        "// This allows compilation but all functions return NULL/0/false"
    )
    .unwrap();
    writeln!(file, "//").unwrap();
    writeln!(file, "// KEEP IN SYNC with src/ffi.rs extern declarations.").unwrap();
    writeln!(
        file,
        "// Run `cargo test -p clips-sys --test stub_parity_test` to verify."
    )
    .unwrap();
    writeln!(file).unwrap();
    writeln!(file, "#include <stddef.h>").unwrap();
    writeln!(file, "#include <stdbool.h>").unwrap();
    writeln!(file).unwrap();

    // ========================================================================
    // Opaque type declarations
    // ========================================================================
    writeln!(file, "// Opaque types").unwrap();
    writeln!(file, "typedef void Environment;").unwrap();
    writeln!(file, "typedef void Fact;").unwrap();
    writeln!(file, "typedef void Instance;").unwrap();
    writeln!(file, "typedef void Deftemplate;").unwrap();
    writeln!(file, "typedef void Defrule;").unwrap();
    writeln!(file, "typedef void Defmodule;").unwrap();
    writeln!(file, "typedef void Deffunction;").unwrap();
    writeln!(file, "typedef void Defglobal;").unwrap();
    writeln!(file, "typedef void Defclass;").unwrap();
    writeln!(file, "typedef void Defgeneric;").unwrap();
    writeln!(file, "typedef void Activation;").unwrap();
    writeln!(file, "typedef void FactBuilder;").unwrap();
    writeln!(file, "typedef void MultifieldBuilder;").unwrap();
    writeln!(file, "typedef void StringBuilder;").unwrap();
    writeln!(file, "typedef void UDFContext;").unwrap();
    writeln!(file, "typedef void UDFValue;").unwrap();
    writeln!(file, "typedef void Multifield;").unwrap();
    writeln!(file, "typedef struct {{ void* header; }} CLIPSValue;").unwrap();
    writeln!(file).unwrap();

    // ========================================================================
    // Environment Management
    // ========================================================================
    writeln!(file, "// Environment Management").unwrap();
    writeln!(
        file,
        "Environment* CreateEnvironment(void) {{ return (void*)0; }}"
    )
    .unwrap();
    writeln!(
        file,
        "bool DestroyEnvironment(Environment* env) {{ return false; }}"
    )
    .unwrap();
    writeln!(file, "bool Clear(Environment* env) {{ return false; }}").unwrap();
    writeln!(file, "void Reset(Environment* env) {{}}").unwrap();
    writeln!(
        file,
        "void* GetEnvironmentData(Environment* env, unsigned int pos) {{ return (void*)0; }}"
    )
    .unwrap();
    writeln!(file, "bool SetEnvironmentData(Environment* env, unsigned int pos, void* data) {{ return false; }}").unwrap();
    writeln!(file, "bool AllocateEnvironmentData(Environment* env, unsigned int pos, size_t size, void (*cleanup)(Environment*)) {{ return false; }}").unwrap();
    writeln!(file).unwrap();

    // ========================================================================
    // Loading and Parsing
    // ========================================================================
    writeln!(file, "// Loading and Parsing").unwrap();
    writeln!(
        file,
        "int Load(Environment* env, const char* filename) {{ return 0; }}"
    )
    .unwrap();
    writeln!(file, "bool LoadFromString(Environment* env, const char* input, size_t max_pos) {{ return false; }}").unwrap();
    writeln!(
        file,
        "bool Bload(Environment* env, const char* filename) {{ return false; }}"
    )
    .unwrap();
    writeln!(
        file,
        "bool Bsave(Environment* env, const char* filename) {{ return false; }}"
    )
    .unwrap();
    writeln!(
        file,
        "bool BatchStar(Environment* env, const char* filename) {{ return false; }}"
    )
    .unwrap();
    writeln!(
        file,
        "int Build(Environment* env, const char* str) {{ return 0; }}"
    )
    .unwrap();
    writeln!(file).unwrap();

    // ========================================================================
    // Evaluation
    // ========================================================================
    writeln!(file, "// Evaluation").unwrap();
    writeln!(
        file,
        "bool Eval(Environment* env, const char* expr, CLIPSValue* result) {{ return false; }}"
    )
    .unwrap();
    writeln!(file, "bool FunctionCall(Environment* env, const char* name, const char* args, CLIPSValue* result) {{ return false; }}").unwrap();
    writeln!(file).unwrap();

    // ========================================================================
    // Fact Operations
    // ========================================================================
    writeln!(file, "// Fact Operations").unwrap();
    writeln!(
        file,
        "Fact* AssertString(Environment* env, const char* str) {{ return (void*)0; }}"
    )
    .unwrap();
    writeln!(file, "int Retract(Fact* fact) {{ return 0; }}").unwrap();
    writeln!(
        file,
        "Fact* GetNextFact(Environment* env, Fact* fact) {{ return (void*)0; }}"
    )
    .unwrap();
    writeln!(
        file,
        "Fact* GetNextFactInTemplate(Deftemplate* tmpl, Fact* fact) {{ return (void*)0; }}"
    )
    .unwrap();
    writeln!(file, "long FactIndex(Fact* fact) {{ return 0; }}").unwrap();
    writeln!(file, "bool FactExistp(Fact* fact) {{ return false; }}").unwrap();
    writeln!(
        file,
        "int GetFactSlot(Fact* fact, const char* slot, CLIPSValue* value) {{ return 0; }}"
    )
    .unwrap();
    writeln!(
        file,
        "void FactPPForm(Fact* fact, char* buffer, size_t size) {{}}"
    )
    .unwrap();
    writeln!(
        file,
        "Deftemplate* FactDeftemplate(Fact* fact) {{ return (void*)0; }}"
    )
    .unwrap();
    writeln!(
        file,
        "Environment* FactEnv(Fact* fact) {{ return (void*)0; }}"
    )
    .unwrap();
    writeln!(file, "void RetainFact(Fact* fact) {{}}").unwrap();
    writeln!(file, "void ReleaseFact(Fact* fact) {{}}").unwrap();
    writeln!(file).unwrap();

    // ========================================================================
    // Fact Builder
    // ========================================================================
    writeln!(file, "// Fact Builder").unwrap();
    writeln!(
        file,
        "FactBuilder* CreateFactBuilder(Environment* env, const char* name) {{ return (void*)0; }}"
    )
    .unwrap();
    writeln!(
        file,
        "int FBPutSlotInteger(FactBuilder* fb, const char* slot, long value) {{ return 0; }}"
    )
    .unwrap();
    writeln!(
        file,
        "int FBPutSlotFloat(FactBuilder* fb, const char* slot, double value) {{ return 0; }}"
    )
    .unwrap();
    writeln!(
        file,
        "int FBPutSlotString(FactBuilder* fb, const char* slot, const char* value) {{ return 0; }}"
    )
    .unwrap();
    writeln!(
        file,
        "int FBPutSlotSymbol(FactBuilder* fb, const char* slot, const char* value) {{ return 0; }}"
    )
    .unwrap();
    writeln!(file, "int FBPutSlotMultifield(FactBuilder* fb, const char* slot, Multifield* value) {{ return 0; }}").unwrap();
    writeln!(
        file,
        "int FBPutSlotFact(FactBuilder* fb, const char* slot, Fact* fact) {{ return 0; }}"
    )
    .unwrap();
    writeln!(
        file,
        "int FBPutSlotInstance(FactBuilder* fb, const char* slot, Instance* inst) {{ return 0; }}"
    )
    .unwrap();
    writeln!(file, "int FBPutSlotCLIPSValue(FactBuilder* fb, const char* slot, CLIPSValue* value) {{ return 0; }}").unwrap();
    writeln!(
        file,
        "Fact* FBAssert(FactBuilder* fb) {{ return (void*)0; }}"
    )
    .unwrap();
    writeln!(file, "void FBDispose(FactBuilder* fb) {{}}").unwrap();
    writeln!(file, "void FBAbort(FactBuilder* fb) {{}}").unwrap();
    writeln!(
        file,
        "int FBSetDeftemplate(FactBuilder* fb, const char* name) {{ return 0; }}"
    )
    .unwrap();
    writeln!(file, "int FBError(Environment* env) {{ return 0; }}").unwrap();
    writeln!(file).unwrap();

    // ========================================================================
    // Template Operations
    // ========================================================================
    writeln!(file, "// Template Operations").unwrap();
    writeln!(
        file,
        "Deftemplate* FindDeftemplate(Environment* env, const char* name) {{ return (void*)0; }}"
    )
    .unwrap();
    writeln!(file, "Deftemplate* GetNextDeftemplate(Environment* env, Deftemplate* tmpl) {{ return (void*)0; }}").unwrap();
    writeln!(
        file,
        "const char* DeftemplateName(Deftemplate* tmpl) {{ return \"\"; }}"
    )
    .unwrap();
    writeln!(
        file,
        "const char* DeftemplateModule(Deftemplate* tmpl) {{ return \"\"; }}"
    )
    .unwrap();
    writeln!(
        file,
        "const char* DeftemplatePPForm(Deftemplate* tmpl) {{ return \"\"; }}"
    )
    .unwrap();
    writeln!(
        file,
        "void DeftemplateSlotNames(Deftemplate* tmpl, CLIPSValue* value) {{}}"
    )
    .unwrap();
    writeln!(file, "bool DeftemplateSlotDefaultValue(Deftemplate* tmpl, const char* slot, CLIPSValue* value) {{ return false; }}").unwrap();
    writeln!(file, "bool DeftemplateSlotAllowedValues(Deftemplate* tmpl, const char* slot, CLIPSValue* value) {{ return false; }}").unwrap();
    writeln!(file, "bool DeftemplateSlotCardinality(Deftemplate* tmpl, const char* slot, CLIPSValue* value) {{ return false; }}").unwrap();
    writeln!(file, "bool DeftemplateSlotRange(Deftemplate* tmpl, const char* slot, CLIPSValue* value) {{ return false; }}").unwrap();
    writeln!(file, "bool DeftemplateSlotTypes(Deftemplate* tmpl, const char* slot, CLIPSValue* value) {{ return false; }}").unwrap();
    writeln!(
        file,
        "bool DeftemplateSlotMultiP(Deftemplate* tmpl, const char* slot) {{ return false; }}"
    )
    .unwrap();
    writeln!(
        file,
        "bool DeftemplateSlotSingleP(Deftemplate* tmpl, const char* slot) {{ return false; }}"
    )
    .unwrap();
    writeln!(
        file,
        "bool DeftemplateSlotExistP(Deftemplate* tmpl, const char* slot) {{ return false; }}"
    )
    .unwrap();
    writeln!(
        file,
        "bool Undeftemplate(Deftemplate* tmpl, Environment* env) {{ return false; }}"
    )
    .unwrap();
    writeln!(
        file,
        "bool DeftemplateIsDeletable(Deftemplate* tmpl) {{ return false; }}"
    )
    .unwrap();
    writeln!(file).unwrap();

    // ========================================================================
    // Rule Operations
    // ========================================================================
    writeln!(file, "// Rule Operations").unwrap();
    writeln!(
        file,
        "Defrule* FindDefrule(Environment* env, const char* name) {{ return (void*)0; }}"
    )
    .unwrap();
    writeln!(
        file,
        "Defrule* GetNextDefrule(Environment* env, Defrule* rule) {{ return (void*)0; }}"
    )
    .unwrap();
    writeln!(
        file,
        "const char* DefruleName(Defrule* rule) {{ return \"\"; }}"
    )
    .unwrap();
    writeln!(
        file,
        "const char* DefruleModule(Defrule* rule) {{ return \"\"; }}"
    )
    .unwrap();
    writeln!(
        file,
        "const char* DefrulePPForm(Defrule* rule) {{ return \"\"; }}"
    )
    .unwrap();
    writeln!(file, "void SetBreak(Defrule* rule) {{}}").unwrap();
    writeln!(file, "bool RemoveBreak(Defrule* rule) {{ return false; }}").unwrap();
    writeln!(
        file,
        "bool DefruleHasBreakpoint(Defrule* rule) {{ return false; }}"
    )
    .unwrap();
    writeln!(
        file,
        "bool Undefrule(Defrule* rule, Environment* env) {{ return false; }}"
    )
    .unwrap();
    writeln!(
        file,
        "bool DefruleIsDeletable(Defrule* rule) {{ return false; }}"
    )
    .unwrap();
    writeln!(file, "void Refresh(Environment* env, Defrule* rule) {{}}").unwrap();
    writeln!(
        file,
        "bool GetDefruleWatchActivations(Defrule* rule) {{ return false; }}"
    )
    .unwrap();
    writeln!(
        file,
        "bool GetDefruleWatchFirings(Defrule* rule) {{ return false; }}"
    )
    .unwrap();
    writeln!(
        file,
        "void SetDefruleWatchActivations(Defrule* rule, bool value) {{}}"
    )
    .unwrap();
    writeln!(
        file,
        "void SetDefruleWatchFirings(Defrule* rule, bool value) {{}}"
    )
    .unwrap();
    writeln!(file).unwrap();

    // ========================================================================
    // Inference Engine
    // ========================================================================
    writeln!(file, "// Inference Engine").unwrap();
    writeln!(
        file,
        "long Run(Environment* env, long limit) {{ return 0; }}"
    )
    .unwrap();
    writeln!(file, "void Halt(Environment* env) {{}}").unwrap();
    writeln!(
        file,
        "bool GetHaltRules(Environment* env) {{ return false; }}"
    )
    .unwrap();
    writeln!(file, "void SetHaltRules(Environment* env, bool value) {{}}").unwrap();
    writeln!(
        file,
        "bool GetHaltExecution(Environment* env) {{ return false; }}"
    )
    .unwrap();
    writeln!(
        file,
        "void SetHaltExecution(Environment* env, bool value) {{}}"
    )
    .unwrap();
    writeln!(
        file,
        "bool GetEvaluationError(Environment* env) {{ return false; }}"
    )
    .unwrap();
    writeln!(
        file,
        "void SetEvaluationError(Environment* env, bool value) {{}}"
    )
    .unwrap();
    writeln!(file, "bool AddPeriodicFunction(Environment* env, const char* name, void (*func)(Environment*, void*), int priority, void* context) {{ return false; }}").unwrap();
    writeln!(
        file,
        "bool RemovePeriodicFunction(Environment* env, const char* name) {{ return false; }}"
    )
    .unwrap();
    writeln!(file).unwrap();

    // ========================================================================
    // Agenda Operations
    // ========================================================================
    writeln!(file, "// Agenda Operations").unwrap();
    writeln!(
        file,
        "Activation* GetNextActivation(Environment* env, Activation* act) {{ return (void*)0; }}"
    )
    .unwrap();
    writeln!(
        file,
        "const char* ActivationRuleName(Activation* act) {{ return \"\"; }}"
    )
    .unwrap();
    writeln!(
        file,
        "int ActivationGetSalience(Activation* act) {{ return 0; }}"
    )
    .unwrap();
    writeln!(
        file,
        "int ActivationSetSalience(Activation* act, int salience) {{ return 0; }}"
    )
    .unwrap();
    writeln!(
        file,
        "void ActivationPPForm(Activation* act, char* buffer, size_t size) {{}}"
    )
    .unwrap();
    writeln!(
        file,
        "bool DeleteActivation(Activation* act) {{ return false; }}"
    )
    .unwrap();
    writeln!(
        file,
        "void ClearAgenda(Environment* env, Defmodule* module) {{}}"
    )
    .unwrap();
    writeln!(
        file,
        "void ReorderAgenda(Environment* env, Defmodule* module) {{}}"
    )
    .unwrap();
    writeln!(
        file,
        "void RefreshAgenda(Environment* env, Defmodule* module) {{}}"
    )
    .unwrap();
    writeln!(
        file,
        "long GetAgendaSize(Environment* env, Defmodule* module) {{ return 0; }}"
    )
    .unwrap();
    writeln!(
        file,
        "int GetSalienceEvaluation(Environment* env) {{ return 0; }}"
    )
    .unwrap();
    writeln!(
        file,
        "int SetSalienceEvaluation(Environment* env, int mode) {{ return 0; }}"
    )
    .unwrap();
    writeln!(file, "int GetStrategy(Environment* env) {{ return 0; }}").unwrap();
    writeln!(
        file,
        "int SetStrategy(Environment* env, int strategy) {{ return 0; }}"
    )
    .unwrap();
    writeln!(
        file,
        "bool GetFactDuplication(Environment* env) {{ return false; }}"
    )
    .unwrap();
    writeln!(
        file,
        "bool SetFactDuplication(Environment* env, bool allow) {{ return false; }}"
    )
    .unwrap();
    writeln!(file).unwrap();

    // ========================================================================
    // Module Operations
    // ========================================================================
    writeln!(file, "// Module Operations").unwrap();
    writeln!(
        file,
        "Defmodule* FindDefmodule(Environment* env, const char* name) {{ return (void*)0; }}"
    )
    .unwrap();
    writeln!(
        file,
        "Defmodule* GetNextDefmodule(Environment* env, Defmodule* module) {{ return (void*)0; }}"
    )
    .unwrap();
    writeln!(
        file,
        "const char* DefmoduleName(Defmodule* module) {{ return \"\"; }}"
    )
    .unwrap();
    writeln!(
        file,
        "const char* DefmodulePPForm(Defmodule* module) {{ return \"\"; }}"
    )
    .unwrap();
    writeln!(
        file,
        "Defmodule* GetCurrentModule(Environment* env) {{ return (void*)0; }}"
    )
    .unwrap();
    writeln!(
        file,
        "Defmodule* SetCurrentModule(Environment* env, Defmodule* module) {{ return (void*)0; }}"
    )
    .unwrap();
    writeln!(
        file,
        "Defmodule* GetFocus(Environment* env) {{ return (void*)0; }}"
    )
    .unwrap();
    writeln!(file, "void Focus(Defmodule* module) {{}}").unwrap();
    writeln!(
        file,
        "Defmodule* PopFocus(Environment* env) {{ return (void*)0; }}"
    )
    .unwrap();
    writeln!(file, "void ClearFocusStack(Environment* env) {{}}").unwrap();
    writeln!(file).unwrap();

    // ========================================================================
    // Global Operations
    // ========================================================================
    writeln!(file, "// Global Operations").unwrap();
    writeln!(
        file,
        "Defglobal* FindDefglobal(Environment* env, const char* name) {{ return (void*)0; }}"
    )
    .unwrap();
    writeln!(
        file,
        "Defglobal* GetNextDefglobal(Environment* env, Defglobal* g) {{ return (void*)0; }}"
    )
    .unwrap();
    writeln!(
        file,
        "const char* DefglobalName(Defglobal* g) {{ return \"\"; }}"
    )
    .unwrap();
    writeln!(
        file,
        "const char* DefglobalModule(Defglobal* g) {{ return \"\"; }}"
    )
    .unwrap();
    writeln!(
        file,
        "const char* DefglobalPPForm(Defglobal* g) {{ return \"\"; }}"
    )
    .unwrap();
    writeln!(
        file,
        "bool DefglobalGetValue(Defglobal* g, CLIPSValue* value) {{ return false; }}"
    )
    .unwrap();
    writeln!(
        file,
        "bool DefglobalSetValue(Defglobal* g, CLIPSValue* value) {{ return false; }}"
    )
    .unwrap();
    writeln!(
        file,
        "bool Undefglobal(Defglobal* g, Environment* env) {{ return false; }}"
    )
    .unwrap();
    writeln!(
        file,
        "bool GetResetGlobals(Environment* env) {{ return false; }}"
    )
    .unwrap();
    writeln!(
        file,
        "bool SetResetGlobals(Environment* env, bool value) {{ return false; }}"
    )
    .unwrap();
    writeln!(file).unwrap();

    // ========================================================================
    // Function Operations
    // ========================================================================
    writeln!(file, "// Function Operations").unwrap();
    writeln!(
        file,
        "Deffunction* FindDeffunction(Environment* env, const char* name) {{ return (void*)0; }}"
    )
    .unwrap();
    writeln!(
        file,
        "Deffunction* GetNextDeffunction(Environment* env, Deffunction* f) {{ return (void*)0; }}"
    )
    .unwrap();
    writeln!(
        file,
        "const char* DeffunctionName(Deffunction* f) {{ return \"\"; }}"
    )
    .unwrap();
    writeln!(
        file,
        "const char* DeffunctionModule(Deffunction* f) {{ return \"\"; }}"
    )
    .unwrap();
    writeln!(
        file,
        "const char* DeffunctionPPForm(Deffunction* f) {{ return \"\"; }}"
    )
    .unwrap();
    writeln!(
        file,
        "bool Undeffunction(Deffunction* f, Environment* env) {{ return false; }}"
    )
    .unwrap();
    writeln!(file).unwrap();

    // ========================================================================
    // Generic Functions
    // ========================================================================
    writeln!(file, "// Generic Functions").unwrap();
    writeln!(
        file,
        "Defgeneric* FindDefgeneric(Environment* env, const char* name) {{ return (void*)0; }}"
    )
    .unwrap();
    writeln!(
        file,
        "Defgeneric* GetNextDefgeneric(Environment* env, Defgeneric* g) {{ return (void*)0; }}"
    )
    .unwrap();
    writeln!(
        file,
        "const char* DefgenericName(Defgeneric* g) {{ return \"\"; }}"
    )
    .unwrap();
    writeln!(
        file,
        "const char* DefgenericModule(Defgeneric* g) {{ return \"\"; }}"
    )
    .unwrap();
    writeln!(
        file,
        "const char* DefgenericPPForm(Defgeneric* g) {{ return \"\"; }}"
    )
    .unwrap();
    writeln!(file).unwrap();

    // ========================================================================
    // COOL (Class/Instance) Operations
    // ========================================================================
    writeln!(file, "// COOL (Class/Instance) Operations").unwrap();
    writeln!(
        file,
        "Defclass* FindDefclass(Environment* env, const char* name) {{ return (void*)0; }}"
    )
    .unwrap();
    writeln!(
        file,
        "Defclass* GetNextDefclass(Environment* env, Defclass* cls) {{ return (void*)0; }}"
    )
    .unwrap();
    writeln!(
        file,
        "const char* DefclassName(Defclass* cls) {{ return \"\"; }}"
    )
    .unwrap();
    writeln!(
        file,
        "const char* DefclassModule(Defclass* cls) {{ return \"\"; }}"
    )
    .unwrap();
    writeln!(
        file,
        "const char* DefclassPPForm(Defclass* cls) {{ return \"\"; }}"
    )
    .unwrap();
    writeln!(
        file,
        "Instance* CreateRawInstance(Defclass* cls, const char* name) {{ return (void*)0; }}"
    )
    .unwrap();
    writeln!(
        file,
        "Instance* MakeInstance(Environment* env, const char* str) {{ return (void*)0; }}"
    )
    .unwrap();
    writeln!(
        file,
        "bool DeleteInstance(Instance* inst) {{ return false; }}"
    )
    .unwrap();
    writeln!(
        file,
        "bool UnmakeInstance(Instance* inst) {{ return false; }}"
    )
    .unwrap();
    writeln!(
        file,
        "Instance* GetNextInstance(Environment* env, Instance* inst) {{ return (void*)0; }}"
    )
    .unwrap();
    writeln!(
        file,
        "Instance* GetNextInstanceInClass(Defclass* cls, Instance* inst) {{ return (void*)0; }}"
    )
    .unwrap();
    writeln!(file, "Instance* FindInstance(Environment* env, Defmodule* module, const char* name, bool search) {{ return (void*)0; }}").unwrap();
    writeln!(
        file,
        "const char* InstanceName(Instance* inst) {{ return \"\"; }}"
    )
    .unwrap();
    writeln!(
        file,
        "void InstancePPForm(Instance* inst, char* buffer, size_t size) {{}}"
    )
    .unwrap();
    writeln!(file, "bool DirectGetSlot(Instance* inst, const char* slot, CLIPSValue* value) {{ return false; }}").unwrap();
    writeln!(file, "bool DirectPutSlot(Instance* inst, const char* slot, CLIPSValue* value) {{ return false; }}").unwrap();
    writeln!(file, "bool Send(Environment* env, CLIPSValue* value, const char* msg, const char* args, CLIPSValue* result) {{ return false; }}").unwrap();
    writeln!(file, "void RetainInstance(Instance* inst) {{}}").unwrap();
    writeln!(file, "void ReleaseInstance(Instance* inst) {{}}").unwrap();
    writeln!(file).unwrap();

    // ========================================================================
    // Watch/Debug Operations
    // ========================================================================
    writeln!(file, "// Watch/Debug Operations").unwrap();
    writeln!(
        file,
        "bool Watch(Environment* env, int item) {{ return false; }}"
    )
    .unwrap();
    writeln!(
        file,
        "bool Unwatch(Environment* env, int item) {{ return false; }}"
    )
    .unwrap();
    writeln!(
        file,
        "bool GetWatchState(Environment* env, int item) {{ return false; }}"
    )
    .unwrap();
    writeln!(
        file,
        "void SetWatchState(Environment* env, int item, bool state) {{}}"
    )
    .unwrap();
    writeln!(
        file,
        "bool DribbleOn(Environment* env, const char* filename) {{ return false; }}"
    )
    .unwrap();
    writeln!(
        file,
        "bool DribbleOff(Environment* env) {{ return false; }}"
    )
    .unwrap();
    writeln!(
        file,
        "bool DribbleActive(Environment* env) {{ return false; }}"
    )
    .unwrap();
    writeln!(file).unwrap();

    // ========================================================================
    // I/O Router Operations
    // ========================================================================
    writeln!(file, "// I/O Router Operations").unwrap();
    writeln!(file, "bool AddRouter(Environment* env, const char* name, int priority, void* query, void* write_fn, void* read_fn, void* unread, void* exit_fn, void* context) {{ return false; }}").unwrap();
    writeln!(
        file,
        "bool DeleteRouter(Environment* env, const char* name) {{ return false; }}"
    )
    .unwrap();
    writeln!(
        file,
        "bool ActivateRouter(Environment* env, const char* name) {{ return false; }}"
    )
    .unwrap();
    writeln!(
        file,
        "bool DeactivateRouter(Environment* env, const char* name) {{ return false; }}"
    )
    .unwrap();
    writeln!(
        file,
        "void WriteString(Environment* env, const char* logical, const char* str) {{}}"
    )
    .unwrap();
    writeln!(
        file,
        "void Writeln(Environment* env, const char* logical, const char* str) {{}}"
    )
    .unwrap();
    writeln!(file).unwrap();

    // ========================================================================
    // Multifield Operations
    // ========================================================================
    writeln!(file, "// Multifield Operations").unwrap();
    writeln!(file, "MultifieldBuilder* CreateMultifieldBuilder(Environment* env, size_t capacity) {{ return (void*)0; }}").unwrap();
    writeln!(
        file,
        "void MBAppendInteger(MultifieldBuilder* mb, long value) {{}}"
    )
    .unwrap();
    writeln!(
        file,
        "void MBAppendFloat(MultifieldBuilder* mb, double value) {{}}"
    )
    .unwrap();
    writeln!(
        file,
        "void MBAppendString(MultifieldBuilder* mb, const char* value) {{}}"
    )
    .unwrap();
    writeln!(
        file,
        "void MBAppendSymbol(MultifieldBuilder* mb, const char* value) {{}}"
    )
    .unwrap();
    writeln!(
        file,
        "void MBAppendFact(MultifieldBuilder* mb, Fact* fact) {{}}"
    )
    .unwrap();
    writeln!(
        file,
        "void MBAppendInstance(MultifieldBuilder* mb, Instance* inst) {{}}"
    )
    .unwrap();
    writeln!(
        file,
        "void MBAppendCLIPSValue(MultifieldBuilder* mb, CLIPSValue* value) {{}}"
    )
    .unwrap();
    writeln!(
        file,
        "Multifield* MBCreate(MultifieldBuilder* mb) {{ return (void*)0; }}"
    )
    .unwrap();
    writeln!(file, "void MBReset(MultifieldBuilder* mb) {{}}").unwrap();
    writeln!(file, "void MBDispose(MultifieldBuilder* mb) {{}}").unwrap();
    writeln!(file).unwrap();

    // ========================================================================
    // String Builder
    // ========================================================================
    writeln!(file, "// String Builder").unwrap();
    writeln!(file, "StringBuilder* CreateStringBuilder(Environment* env, size_t capacity) {{ return (void*)0; }}").unwrap();
    writeln!(
        file,
        "void SBAppend(StringBuilder* sb, const char* str) {{}}"
    )
    .unwrap();
    writeln!(
        file,
        "void SBAppendInteger(StringBuilder* sb, long value) {{}}"
    )
    .unwrap();
    writeln!(
        file,
        "void SBAppendFloat(StringBuilder* sb, double value) {{}}"
    )
    .unwrap();
    writeln!(
        file,
        "const char* SBCopy(StringBuilder* sb) {{ return \"\"; }}"
    )
    .unwrap();
    writeln!(file, "void SBReset(StringBuilder* sb) {{}}").unwrap();
    writeln!(file, "void SBDispose(StringBuilder* sb) {{}}").unwrap();
    writeln!(file).unwrap();

    // ========================================================================
    // User Defined Functions
    // ========================================================================
    writeln!(file, "// User Defined Functions").unwrap();
    writeln!(file, "bool AddUDF(Environment* env, const char* name, const char* ret_types, int min_args, int max_args, const char* arg_types, void* func, const char* func_name, void* context) {{ return false; }}").unwrap();
    writeln!(file, "bool UDFFirstArgument(UDFContext* ctx, unsigned int types, UDFValue* value) {{ return false; }}").unwrap();
    writeln!(file, "bool UDFNextArgument(UDFContext* ctx, unsigned int types, UDFValue* value) {{ return false; }}").unwrap();
    writeln!(file, "bool UDFNthArgument(UDFContext* ctx, unsigned int n, unsigned int types, UDFValue* value) {{ return false; }}").unwrap();
    writeln!(
        file,
        "unsigned int UDFArgumentCount(UDFContext* ctx) {{ return 0; }}"
    )
    .unwrap();
    writeln!(
        file,
        "bool UDFHasNextArgument(UDFContext* ctx) {{ return false; }}"
    )
    .unwrap();
    writeln!(file, "void UDFThrowError(UDFContext* ctx) {{}}").unwrap();
    writeln!(file).unwrap();

    // ========================================================================
    // Value Access Functions (wrappers from clips_wrapper.c)
    // ========================================================================
    writeln!(file, "// Value Access Functions (clips_wrapper.c stubs)").unwrap();
    writeln!(file, "int clips_cv_type(CLIPSValue* cv) {{ return 0; }}").unwrap();
    writeln!(
        file,
        "bool clips_cv_is_type(CLIPSValue* cv, unsigned int type_bits) {{ return false; }}"
    )
    .unwrap();
    writeln!(
        file,
        "long long clips_cv_to_integer(CLIPSValue* cv) {{ return 0; }}"
    )
    .unwrap();
    writeln!(
        file,
        "double clips_cv_to_float(CLIPSValue* cv) {{ return 0.0; }}"
    )
    .unwrap();
    writeln!(
        file,
        "const char* clips_cv_to_string(CLIPSValue* cv) {{ return \"\"; }}"
    )
    .unwrap();
    writeln!(
        file,
        "Fact* clips_cv_to_fact(const CLIPSValue* cv) {{ return (void*)0; }}"
    )
    .unwrap();
    writeln!(
        file,
        "Instance* clips_cv_to_instance(const CLIPSValue* cv) {{ return (void*)0; }}"
    )
    .unwrap();
    writeln!(
        file,
        "Multifield* clips_cv_to_multifield(const CLIPSValue* cv) {{ return (void*)0; }}"
    )
    .unwrap();
    writeln!(
        file,
        "void* clips_cv_to_external_address(CLIPSValue* cv) {{ return (void*)0; }}"
    )
    .unwrap();
    writeln!(file, "void clips_cv_set_void(CLIPSValue* cv) {{}}").unwrap();
    writeln!(
        file,
        "void clips_cv_set_integer(Environment* env, CLIPSValue* cv, long long value) {{}}"
    )
    .unwrap();
    writeln!(
        file,
        "void clips_cv_set_float(Environment* env, CLIPSValue* cv, double value) {{}}"
    )
    .unwrap();
    writeln!(
        file,
        "void clips_cv_set_symbol(Environment* env, CLIPSValue* cv, const char* value) {{}}"
    )
    .unwrap();
    writeln!(
        file,
        "void clips_cv_set_string(Environment* env, CLIPSValue* cv, const char* value) {{}}"
    )
    .unwrap();
    writeln!(
        file,
        "void clips_cv_set_fact(CLIPSValue* cv, Fact* fact) {{}}"
    )
    .unwrap();
    writeln!(
        file,
        "void clips_cv_set_instance(CLIPSValue* cv, Instance* inst) {{}}"
    )
    .unwrap();
    writeln!(
        file,
        "void clips_cv_set_multifield(CLIPSValue* cv, Multifield* mf) {{}}"
    )
    .unwrap();
    writeln!(file, "void clips_cv_set_external_address(Environment* env, CLIPSValue* cv, void* addr, unsigned short type_idx) {{}}").unwrap();
    writeln!(file).unwrap();

    // ========================================================================
    // Multifield Access Wrappers (from clips_wrapper.c)
    // ========================================================================
    writeln!(file, "// Multifield Access Wrappers").unwrap();
    writeln!(
        file,
        "size_t clips_multifield_length(const Multifield* mf) {{ return 0; }}"
    )
    .unwrap();
    writeln!(
        file,
        "void clips_multifield_slot(Multifield* mf, size_t index, CLIPSValue* result) {{}}"
    )
    .unwrap();
    writeln!(file).unwrap();

    // ========================================================================
    // Defrule Helper Wrappers (from clips_wrapper.c)
    // ========================================================================
    writeln!(file, "// Defrule Helper Wrappers").unwrap();
    writeln!(
        file,
        "unsigned long long clips_get_defrule_firings(Defrule* rule) {{ return 0; }}"
    )
    .unwrap();
    writeln!(file).unwrap();

    // ========================================================================
    // Memory Management
    // ========================================================================
    writeln!(file, "// Memory Management").unwrap();
    writeln!(
        file,
        "void* genalloc(Environment* env, size_t size) {{ return (void*)0; }}"
    )
    .unwrap();
    writeln!(
        file,
        "void genfree(Environment* env, void* ptr, size_t size) {{}}"
    )
    .unwrap();
    writeln!(file, "long MemUsed(Environment* env) {{ return 0; }}").unwrap();
    writeln!(file, "long MemRequests(Environment* env) {{ return 0; }}").unwrap();
    writeln!(file).unwrap();

    // ========================================================================
    // Utility Functions
    // ========================================================================
    writeln!(file, "// Utility Functions").unwrap();
    writeln!(file, "const char* Version(void) {{ return \"6.4-stub\"; }}").unwrap();

    cc::Build::new()
        .file(&stub_c)
        .warnings(false)
        .compile("clips");

    println!("cargo:rustc-link-lib=static=clips");
    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:warning=Using stub CLIPS library - functionality will not work");
}
