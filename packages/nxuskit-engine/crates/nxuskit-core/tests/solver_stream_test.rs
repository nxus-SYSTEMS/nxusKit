//! Contract tests for solver streaming (`nxuskit_solver_solve_stream`).
//!
//! **TDD**: Written before implementation. These tests define the expected
//! streaming behavior for the solver C ABI. They will fail until the streaming
//! implementation in `solver_sdk.rs` delivers real `SolverProgressEvent` chunks
//! to the `on_chunk` callback for optimization problems.
//!
//! # Test Matrix
//!
//! | Test | Scenario | Expected Behavior |
//! |------|----------|-------------------|
//! | `test_optimization_yields_progress_events` | Optimization (maximize) | on_chunk called >= 1 time |
//! | `test_satisfaction_yields_no_progress_events` | Pure SAT | on_chunk called 0 times |
//! | `test_progress_event_json_schema` | Optimization | Progress JSON has required fields |
//! | `test_done_result_has_terminal_status` | Optimization | done JSON has terminal SolveStatus |
//!
//! # Architecture
//!
//! The `nxuskit_solver_*` functions are `#[unsafe(no_mangle)] pub extern "C"`
//! in the private `solver_sdk` module. We declare them via `unsafe extern "C"`
//! blocks since Cargo links integration tests against the crate's rlib,
//! making these symbols available.

use std::ffi::{CStr, CString, c_void};
use std::os::raw::c_char;
use std::sync::{Arc, Mutex};

// Re-export crate so it gets linked (the rlib provides the no_mangle symbols).
use nxuskit_core as _;

// ── C ABI Declarations ──────────────────────────────────────────────
//
// These symbols are defined in nxuskit-core's solver_sdk.rs with
// #[unsafe(no_mangle)]. They are available as linker symbols even though
// the module is private.

// Opaque handle — we never inspect it, just pass it around.
#[repr(C)]
struct NxuskitSolverSession {
    _opaque: [u8; 0],
}

unsafe extern "C" {
    fn nxuskit_solver_session_create(config_json: *const c_char) -> *mut NxuskitSolverSession;

    fn nxuskit_solver_session_destroy(session: *mut NxuskitSolverSession);

    fn nxuskit_solver_add_variables(
        session: *mut NxuskitSolverSession,
        variables_json: *const c_char,
    ) -> bool;

    fn nxuskit_solver_add_constraints(
        session: *mut NxuskitSolverSession,
        constraints_json: *const c_char,
    ) -> bool;

    fn nxuskit_solver_set_objective(
        session: *mut NxuskitSolverSession,
        objective_json: *const c_char,
    ) -> bool;

    fn nxuskit_solver_solve_stream(
        session: *mut NxuskitSolverSession,
        config_json: *const c_char,
        on_chunk: Option<unsafe extern "C" fn(*const c_char, *mut c_void) -> i32>,
        on_done: Option<unsafe extern "C" fn(*const c_char, *mut c_void)>,
        user_data: *mut c_void,
    ) -> bool;

    #[allow(dead_code)]
    fn nxuskit_free_string(ptr: *mut c_char);
}

// ── Stream Collector ────────────────────────────────────────────────

/// Collects all stream events from a streaming solve.
#[derive(Debug, Default)]
struct StreamCollector {
    /// Progress chunks received via on_chunk (parsed JSON values).
    chunks: Vec<serde_json::Value>,
    /// Final result received via on_done (parsed JSON value).
    done_result: Option<serde_json::Value>,
}

/// C callback for `on_chunk` — collects each JSON progress event.
///
/// Returns 0 to continue streaming.
///
/// # Safety
///
/// `json_ptr` must be a valid, NUL-terminated C string.
/// `user_data` must be a valid pointer to `Arc<Mutex<StreamCollector>>`.
unsafe extern "C" fn collect_chunk(json_ptr: *const c_char, user_data: *mut c_void) -> i32 {
    let collector = unsafe { &*(user_data as *const Arc<Mutex<StreamCollector>>) };
    let json_str = unsafe { CStr::from_ptr(json_ptr) }
        .to_str()
        .expect("on_chunk: JSON is not valid UTF-8");
    let value: serde_json::Value =
        serde_json::from_str(json_str).expect("on_chunk: JSON failed to parse");
    collector.lock().unwrap().chunks.push(value);
    0 // continue
}

/// C callback for `on_done` — stores the final result.
///
/// # Safety
///
/// `json_ptr` must be a valid, NUL-terminated C string.
/// `user_data` must be a valid pointer to `Arc<Mutex<StreamCollector>>`.
unsafe extern "C" fn collect_done(json_ptr: *const c_char, user_data: *mut c_void) {
    let collector = unsafe { &*(user_data as *const Arc<Mutex<StreamCollector>>) };
    let json_str = unsafe { CStr::from_ptr(json_ptr) }
        .to_str()
        .expect("on_done: JSON is not valid UTF-8");
    let value: serde_json::Value =
        serde_json::from_str(json_str).expect("on_done: JSON failed to parse");
    collector.lock().unwrap().done_result = Some(value);
}

// ── Session Setup Helpers ───────────────────────────────────────────

/// Create an optimization session with enough complexity that Z3
/// goes through multiple solver iterations.
///
/// Model: 10 integer variables x0..x9 in [0, 1000], with ordering
/// constraints x0 <= x1 <= ... <= x9 <= 500, maximizing x9.
///
/// # Safety
///
/// Returns a non-null session handle that must be destroyed with
/// `nxuskit_solver_session_destroy`.
unsafe fn setup_optimization_session() -> *mut NxuskitSolverSession {
    let session = unsafe { nxuskit_solver_session_create(std::ptr::null()) };
    assert!(!session.is_null(), "Failed to create solver session");

    // Add 10 integer variables with domain [0, 1000]
    let vars: Vec<serde_json::Value> = (0..10)
        .map(|i| {
            serde_json::json!({
                "name": format!("x{i}"),
                "var_type": "integer",
                "domain": { "min": 0.0, "max": 1000.0 }
            })
        })
        .collect();
    let vars_json = CString::new(serde_json::to_string(&vars).unwrap()).unwrap();
    assert!(
        unsafe { nxuskit_solver_add_variables(session, vars_json.as_ptr()) },
        "Failed to add variables"
    );

    // Add ordering constraints: x_i <= x_{i+1}
    let mut constraints: Vec<serde_json::Value> = (0..9)
        .map(|i| {
            serde_json::json!({
                "constraint_type": "le",
                "variables": [format!("x{i}"), format!("x{}", i + 1)],
                "parameters": {}
            })
        })
        .collect();

    // Add upper bound: x9 <= 500
    constraints.push(serde_json::json!({
        "constraint_type": "le",
        "variables": ["x9"],
        "parameters": { "right": 500 }
    }));

    let constraints_json = CString::new(serde_json::to_string(&constraints).unwrap()).unwrap();
    assert!(
        unsafe { nxuskit_solver_add_constraints(session, constraints_json.as_ptr()) },
        "Failed to add constraints"
    );

    // Set objective: maximize x9
    let objective = serde_json::json!({
        "name": "maximize_x9",
        "direction": "maximize",
        "expression": "x9"
    });
    let obj_json = CString::new(serde_json::to_string(&objective).unwrap()).unwrap();
    assert!(
        unsafe { nxuskit_solver_set_objective(session, obj_json.as_ptr()) },
        "Failed to set objective"
    );

    session
}

/// Create a simple satisfaction session (no objective).
///
/// Model: 1 integer variable x in [0, 10] with constraint x <= 5.
///
/// # Safety
///
/// Returns a non-null session handle that must be destroyed with
/// `nxuskit_solver_session_destroy`.
unsafe fn setup_satisfaction_session() -> *mut NxuskitSolverSession {
    let session = unsafe { nxuskit_solver_session_create(std::ptr::null()) };
    assert!(!session.is_null(), "Failed to create solver session");

    let vars_json =
        CString::new(r#"[{"name":"x","var_type":"integer","domain":{"min":0.0,"max":10.0}}]"#)
            .unwrap();
    assert!(
        unsafe { nxuskit_solver_add_variables(session, vars_json.as_ptr()) },
        "Failed to add variable"
    );

    let constraints_json =
        CString::new(r#"[{"constraint_type":"le","variables":["x"],"parameters":{"right":5}}]"#)
            .unwrap();
    assert!(
        unsafe { nxuskit_solver_add_constraints(session, constraints_json.as_ptr()) },
        "Failed to add constraint"
    );

    // No objective set — pure satisfaction problem.

    session
}

/// Run `nxuskit_solver_solve_stream` with the given session, collecting
/// all chunks and the done callback result into a `StreamCollector`.
///
/// # Safety
///
/// `session` must be a valid solver session handle.
unsafe fn run_streaming_solve(session: *mut NxuskitSolverSession) -> (bool, StreamCollector) {
    let collector = Arc::new(Mutex::new(StreamCollector::default()));
    let collector_ptr = &collector as *const Arc<Mutex<StreamCollector>> as *mut c_void;

    let success = unsafe {
        nxuskit_solver_solve_stream(
            session,
            std::ptr::null(),
            Some(collect_chunk),
            Some(collect_done),
            collector_ptr,
        )
    };

    let collected = Arc::try_unwrap(collector)
        .expect("Arc should have single owner after synchronous solve_stream")
        .into_inner()
        .unwrap();
    (success, collected)
}

// ── Contract Tests ──────────────────────────────────────────────────

/// T014-CT1: Optimization problem yields >= 1 progress event.
///
/// An optimization problem (maximize x9) should emit at least one
/// `SolverProgressEvent` via the `on_chunk` callback with
/// `is_final: false` before delivering the final result via `on_done`.
#[test]
#[ignore = "requires Z3 runtime + streaming implementation (T014)"]
fn test_optimization_yields_progress_events() {
    unsafe {
        let session = setup_optimization_session();
        let (success, collected) = run_streaming_solve(session);
        nxuskit_solver_session_destroy(session);

        assert!(success, "solve_stream should return true on success");

        assert!(
            !collected.chunks.is_empty(),
            "Optimization should yield at least 1 progress event, got 0"
        );

        // All non-final chunks should have is_final: false
        for (i, chunk) in collected.chunks.iter().enumerate() {
            if i < collected.chunks.len() - 1 {
                assert_eq!(
                    chunk.get("is_final").and_then(|v| v.as_bool()),
                    Some(false),
                    "Intermediate progress event {i} should have is_final: false, got: {chunk}"
                );
            }
        }

        assert!(
            collected.done_result.is_some(),
            "Should receive done callback with final result"
        );
    }
}

/// T014-CT2: Satisfaction problem yields 0 progress events.
///
/// A pure SAT problem (no objective) should not produce any progress
/// chunks — only the final result via `on_done`.
#[test]
#[ignore = "requires Z3 runtime + streaming implementation (T014)"]
fn test_satisfaction_yields_no_progress_events() {
    unsafe {
        let session = setup_satisfaction_session();
        let (success, collected) = run_streaming_solve(session);
        nxuskit_solver_session_destroy(session);

        assert!(success, "solve_stream should return true on success");

        assert!(
            collected.chunks.is_empty(),
            "Satisfaction solve should not produce progress chunks, got {}",
            collected.chunks.len()
        );

        assert!(
            collected.done_result.is_some(),
            "Should receive done callback with final result"
        );
    }
}

/// T014-CT3: Progress event JSON has the expected schema.
///
/// Each `SolverProgressEvent` delivered via `on_chunk` must contain the
/// fields defined in `data-model.md`:
///   - `status` (string)
///   - `iteration` (u32)
///   - `elapsed_ms` (u64)
///   - `is_final` (bool)
///
/// Optional fields (`objective_value`, `total_iterations`, `bound_gap`)
/// may be present or absent.
#[test]
#[ignore = "requires Z3 runtime + streaming implementation (T014)"]
fn test_progress_event_json_schema() {
    unsafe {
        let session = setup_optimization_session();
        let (_success, collected) = run_streaming_solve(session);
        nxuskit_solver_session_destroy(session);

        assert!(
            !collected.chunks.is_empty(),
            "Need at least 1 progress event to verify schema"
        );

        for (i, chunk) in collected.chunks.iter().enumerate() {
            // Required fields
            assert!(
                chunk.get("status").is_some(),
                "Progress event {i} missing 'status' field: {chunk}"
            );
            assert!(
                chunk.get("iteration").is_some(),
                "Progress event {i} missing 'iteration' field: {chunk}"
            );
            assert!(
                chunk.get("elapsed_ms").is_some(),
                "Progress event {i} missing 'elapsed_ms' field: {chunk}"
            );
            assert!(
                chunk.get("is_final").is_some(),
                "Progress event {i} missing 'is_final' field: {chunk}"
            );

            // Type checks
            assert!(
                chunk["status"].is_string(),
                "Progress event {i}: 'status' should be a string, got: {}",
                chunk["status"]
            );
            assert!(
                chunk["iteration"].is_u64(),
                "Progress event {i}: 'iteration' should be a u32/u64, got: {}",
                chunk["iteration"]
            );
            assert!(
                chunk["elapsed_ms"].is_u64(),
                "Progress event {i}: 'elapsed_ms' should be a u64, got: {}",
                chunk["elapsed_ms"]
            );
            assert!(
                chunk["is_final"].is_boolean(),
                "Progress event {i}: 'is_final' should be a bool, got: {}",
                chunk["is_final"]
            );

            // Non-final events should have is_final: false
            if !chunk["is_final"].as_bool().unwrap_or(true) {
                assert_eq!(
                    chunk["status"].as_str(),
                    Some("in_progress"),
                    "Non-final event {i} should have status 'in_progress', got: {}",
                    chunk["status"]
                );
            }

            // If objective_value is present, it should be a number
            if let Some(obj_val) = chunk.get("objective_value") {
                assert!(
                    obj_val.is_number(),
                    "Progress event {i}: 'objective_value' should be a number, got: {obj_val}"
                );
            }

            // If bound_gap is present, it should be a non-negative number
            if let Some(gap) = chunk.get("bound_gap") {
                assert!(
                    gap.is_number(),
                    "Progress event {i}: 'bound_gap' should be a number, got: {gap}"
                );
                if let Some(gap_f) = gap.as_f64() {
                    assert!(
                        gap_f >= 0.0,
                        "Progress event {i}: 'bound_gap' should be >= 0, got: {gap_f}"
                    );
                }
            }
        }

        // Verify monotonic iteration counts
        let iterations: Vec<u64> = collected
            .chunks
            .iter()
            .filter_map(|c| c.get("iteration").and_then(|v| v.as_u64()))
            .collect();
        for window in iterations.windows(2) {
            assert!(
                window[1] >= window[0],
                "Iteration counts should be monotonically increasing: {:?}",
                iterations
            );
        }
    }
}

/// T014-CT4: Done result has a terminal status.
///
/// The final result delivered via `on_done` must be a valid `SolveResult`
/// JSON with a terminal `status` field (one of: "sat", "unsat", "optimal",
/// "timeout", "unknown").
#[test]
#[ignore = "requires Z3 runtime + streaming implementation (T014)"]
fn test_done_result_has_terminal_status() {
    unsafe {
        let session = setup_optimization_session();
        let (success, collected) = run_streaming_solve(session);
        nxuskit_solver_session_destroy(session);

        assert!(success, "solve_stream should return true");

        let done = collected
            .done_result
            .as_ref()
            .expect("Should have received done callback");

        // The done result should have a 'status' field
        let status = done
            .get("status")
            .and_then(|s| s.as_str())
            .expect("Done result should have a string 'status' field");

        let terminal_statuses = ["sat", "unsat", "optimal", "timeout", "unknown"];
        assert!(
            terminal_statuses.contains(&status),
            "Done result status should be one of {terminal_statuses:?}, got: {status:?}"
        );

        // For this optimization problem, we expect "optimal" (x9 = 500)
        // but we accept any terminal status for the contract test
        assert!(
            done.get("stats").is_some(),
            "Done result should contain 'stats' field"
        );

        // If status is "sat" or "optimal", assignments should be present
        if status == "sat" || status == "optimal" {
            assert!(
                done.get("assignments").is_some(),
                "Done result with status '{status}' should contain 'assignments'"
            );
        }
    }
}
