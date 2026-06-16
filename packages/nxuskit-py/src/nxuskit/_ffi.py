"""Low-level cffi bindings to the nxuskit shared library.

This module loads libnxuskit and exposes the raw C functions. Higher-level
Python wrappers are in _ffi_provider.py.

The shared library is located by searching (aligned with Rust SDK):
  1. ``NXUSKIT_LIB_DIR`` environment variable (directory containing the library)
  2. ``NXUSKIT_SDK_DIR/lib`` (SDK root with /lib subdirectory)
  3. ``NXUSKIT_LIB_PATH`` environment variable (exact file path, legacy)
  4. ``~/.nxuskit/sdk/current/lib/`` (standard install path)
  5. The ``libs/`` subdirectory next to this file (wheel distribution)
  6. System library search path (LD_LIBRARY_PATH, DYLD_LIBRARY_PATH, PATH)
"""

from __future__ import annotations

import os
import platform
import sys
import warnings
from pathlib import Path

from cffi import FFI

# Expected version — must match the linked nxuskit library.
EXPECTED_VERSION = "1.0.4"

# ── cffi setup ────────────────────────────────────────────────

ffi = FFI()

# C declarations matching nxuskit.h (cbindgen output).
# We use a stripped-down version without includes or typedefs that
# cffi can't parse — cffi handles opaque pointers via ``...``.
ffi.cdef("""
    typedef struct NxuskitProvider NxuskitProvider;
    typedef struct NxuskitResponse NxuskitResponse;
    typedef struct NxuskitStream NxuskitStream;

    typedef int32_t (*NxuskitStreamCallback)(const char *chunk_json, void *user_data);
    typedef void (*NxuskitStreamDoneCallback)(const char *final_json, void *user_data);

    const char *nxuskit_version(void);
    NxuskitProvider *nxuskit_create_provider(const char *config_json);
    void nxuskit_free_provider(NxuskitProvider *provider);
    NxuskitResponse *nxuskit_chat(NxuskitProvider *provider, const char *request_json);
    const char *nxuskit_response_json(const NxuskitResponse *response);
    void nxuskit_free_response(NxuskitResponse *response);
    NxuskitStream *nxuskit_chat_stream(
        NxuskitProvider *provider,
        const char *request_json,
        NxuskitStreamCallback on_chunk,
        NxuskitStreamDoneCallback on_done,
        void *user_data
    );
    void nxuskit_cancel_stream(NxuskitStream *stream);
    void nxuskit_free_stream(NxuskitStream *stream);
    char *nxuskit_list_models(NxuskitProvider *provider);
    const char *nxuskit_last_error(void);
    void nxuskit_free_string(char *ptr);

    /* License functions (v0.9.1) */
    char *nxuskit_license_resolve(const char *explicit_key);
    char *nxuskit_license_validate(const char *token_jwt);
    char *nxuskit_license_machine_id(void);
    char *nxuskit_license_activate(const char *purchase_id);
    char *nxuskit_license_deactivate(void);
    char *nxuskit_license_trial_issue(void);
    char *nxuskit_license_trial_activate(const char *activation_code);

    /* Plugin trust mode (v0.9.1) */
    int32_t nxuskit_plugin_set_trust_mode(int32_t mode);
    int32_t nxuskit_plugin_get_trust_mode(void);
    int32_t nxuskit_plugin_load_dir_trusted(const char *dir_path);
    char *nxuskit_plugin_list(void);
    char *nxuskit_plugin_info(const char *name);
    int32_t nxuskit_plugin_count(void);
    int32_t nxuskit_plugin_loaded(const char *name);
    void nxuskit_plugin_unload_all(void);

    /* OAuth functions (v0.9.1) */
    char *nxuskit_oauth_start(const char *provider_id, uint32_t timeout_secs);
    char *nxuskit_oauth_status(const char *provider_id);
    int32_t nxuskit_oauth_revoke(const char *provider_id);
""")

# ── Domain declarations (BN, CLIPS, Solver, ZEN) ─────────────
#
# These were previously in separate _*_ffi.py modules, each with its
# own FFI() instance and dlopen call. Consolidated here so all domains
# share a single library handle. The domain wrapper modules now import
# ffi and lib from this module.

ffi.cdef("""
    /* ── Bayesian Network ──────────────────────────────────────── */

    typedef struct NxuskitBnNet NxuskitBnNet;
    typedef struct NxuskitBnEvidence NxuskitBnEvidence;
    typedef struct NxuskitBnResult NxuskitBnResult;

    typedef _Bool (*NxuskitBnStreamCallback)(
        const char *chunk_json,
        uint32_t iteration,
        uint32_t total,
        _Bool is_final,
        void *user_data
    );

    NxuskitBnNet *nxuskit_bn_net_create(void);
    void nxuskit_bn_net_destroy(NxuskitBnNet *net);
    NxuskitBnNet *nxuskit_bn_net_load_file(const char *path);
    int32_t nxuskit_bn_net_num_variables(const NxuskitBnNet *net);
    char *nxuskit_bn_net_variables(const NxuskitBnNet *net);
    char *nxuskit_bn_net_variable_states(const NxuskitBnNet *net, const char *variable);
    _Bool nxuskit_bn_net_save_file(const NxuskitBnNet *net, const char *path);
    _Bool nxuskit_bn_net_add_gaussian_variable(
        NxuskitBnNet *net, const char *name, double mean_base, double variance
    );
    _Bool nxuskit_bn_net_set_gaussian_weight(
        NxuskitBnNet *net, const char *variable, const char *parent, double weight
    );

    NxuskitBnEvidence *nxuskit_bn_ev_create(void);
    void nxuskit_bn_ev_destroy(NxuskitBnEvidence *ev);
    _Bool nxuskit_bn_ev_set_discrete(
        NxuskitBnEvidence *ev, const NxuskitBnNet *net,
        const char *variable, const char *state
    );
    _Bool nxuskit_bn_ev_set_continuous(
        NxuskitBnEvidence *ev, const NxuskitBnNet *net,
        const char *variable, double value
    );
    _Bool nxuskit_bn_ev_retract(NxuskitBnEvidence *ev, const char *variable);
    _Bool nxuskit_bn_ev_clear(NxuskitBnEvidence *ev);

    NxuskitBnResult *nxuskit_bn_infer(
        const NxuskitBnNet *net, const NxuskitBnEvidence *ev,
        const char *algorithm, uint32_t num_samples, uint32_t burn_in, uint64_t seed
    );
    NxuskitBnResult *nxuskit_bn_infer_with_config(
        const NxuskitBnNet *net, const NxuskitBnEvidence *ev,
        const char *algorithm, const char *config_json
    );
    _Bool nxuskit_bn_infer_stream(
        const NxuskitBnNet *net, const NxuskitBnEvidence *ev,
        uint32_t num_samples, uint32_t burn_in, uint64_t seed, uint32_t chunk_size,
        NxuskitBnStreamCallback on_chunk, void *user_data
    );

    void nxuskit_bn_result_destroy(NxuskitBnResult *result);
    char *nxuskit_bn_result_json(const NxuskitBnResult *result);
    char *nxuskit_bn_result_query(const NxuskitBnResult *result, const char *variable);
    int32_t nxuskit_bn_result_num_variables(const NxuskitBnResult *result);
    char *nxuskit_bn_result_next(NxuskitBnResult *result);
    void nxuskit_bn_result_reset(NxuskitBnResult *result);
    double nxuskit_bn_result_mean(const NxuskitBnResult *result, const char *variable);
    double nxuskit_bn_result_variance(const NxuskitBnResult *result, const char *variable);
    char *nxuskit_bn_result_continuous_marginal(
        const NxuskitBnResult *result, const char *variable
    );

    _Bool nxuskit_bn_learn_mle(NxuskitBnNet *net, const char *csv_path, double pseudocount);
    double nxuskit_bn_log_likelihood(const NxuskitBnNet *net, const char *csv_path);
    char *nxuskit_bn_search_structure(
        const NxuskitBnNet *net,
        const char *csv_path,
        const char *algorithm,
        const char *scoring,
        uint32_t max_parents,
        uint32_t max_steps,
        double ess,
        const char *ordering_json
    );

    /* ── CLIPS Session ─────────────────────────────────────────── */

    uint64_t nxuskit_clips_session_create(void);
    void nxuskit_clips_session_destroy(uint64_t session);
    int32_t nxuskit_clips_session_reset(uint64_t session);
    int32_t nxuskit_clips_session_clear(uint64_t session);
    char *nxuskit_clips_session_info(uint64_t session);

    int32_t nxuskit_clips_session_load_file(uint64_t session, const char *path);
    int32_t nxuskit_clips_session_load_string(uint64_t session, const char *constructs);
    int32_t nxuskit_clips_session_load_binary(uint64_t session, const char *path);
    int32_t nxuskit_clips_session_save_binary(uint64_t session, const char *path);
    int32_t nxuskit_clips_session_build(uint64_t session, const char *construct);
    int32_t nxuskit_clips_session_load_json(uint64_t session, const char *json);
    int32_t nxuskit_clips_session_batch(uint64_t session, const char *path);

    int64_t nxuskit_clips_fact_assert_string(uint64_t session, const char *fact_string);
    int64_t nxuskit_clips_fact_assert_structured(uint64_t session,
                                                 const char *template_name,
                                                 const char *slots_json);
    int32_t nxuskit_clips_fact_retract(uint64_t session, int64_t fact_index);
    int32_t nxuskit_clips_fact_retract_by_template(uint64_t session, const char *template_name);
    _Bool nxuskit_clips_fact_exists(uint64_t session, int64_t fact_index);
    char *nxuskit_clips_fact_get_slot(uint64_t session, int64_t fact_index, const char *slot_name);
    char *nxuskit_clips_fact_slot_values(uint64_t session, int64_t fact_index);
    char *nxuskit_clips_fact_pp_form(uint64_t session, int64_t fact_index);
    int64_t nxuskit_clips_fact_index(uint64_t session, int64_t fact_index);
    char *nxuskit_clips_facts_list(uint64_t session);
    char *nxuskit_clips_facts_by_template(uint64_t session, const char *template_name);

    _Bool nxuskit_clips_template_exists(uint64_t session, const char *name);
    char *nxuskit_clips_template_list(uint64_t session);
    char *nxuskit_clips_template_slot_names(uint64_t session, const char *template_name);
    char *nxuskit_clips_template_slot_info(uint64_t session, const char *template_name);
    char *nxuskit_clips_template_facts(uint64_t session, const char *template_name);
    char *nxuskit_clips_template_pp_form(uint64_t session, const char *template_name);

    _Bool nxuskit_clips_rule_exists(uint64_t session, const char *name);
    char *nxuskit_clips_rule_list(uint64_t session);
    int64_t nxuskit_clips_rule_times_fired(uint64_t session, const char *rule_name);
    int32_t nxuskit_clips_rule_breakpoint_set(uint64_t session, const char *rule_name);
    int32_t nxuskit_clips_rule_breakpoint_remove(uint64_t session, const char *rule_name);
    _Bool nxuskit_clips_rule_has_breakpoint(uint64_t session, const char *rule_name);
    int32_t nxuskit_clips_rule_refresh(uint64_t session, const char *rule_name);
    char *nxuskit_clips_rule_pp_form(uint64_t session, const char *rule_name);
    int32_t nxuskit_clips_rule_delete(uint64_t session, const char *rule_name);

    int64_t nxuskit_clips_session_run(uint64_t session, int64_t limit);
    int32_t nxuskit_clips_session_halt(uint64_t session);
    int64_t nxuskit_clips_agenda_size(uint64_t session);
    int32_t nxuskit_clips_agenda_clear(uint64_t session);
    int32_t nxuskit_clips_agenda_reorder(uint64_t session);
    char *nxuskit_clips_strategy_get(uint64_t session);
    int32_t nxuskit_clips_strategy_set(uint64_t session, const char *strategy);
    char *nxuskit_clips_salience_mode_get(uint64_t session);
    int32_t nxuskit_clips_salience_mode_set(uint64_t session, const char *mode);

    _Bool nxuskit_clips_module_exists(uint64_t session, const char *name);
    char *nxuskit_clips_module_list(uint64_t session);
    char *nxuskit_clips_module_current_get(uint64_t session);
    int32_t nxuskit_clips_module_current_set(uint64_t session, const char *name);
    int32_t nxuskit_clips_focus_push(uint64_t session, const char *module_name);
    char *nxuskit_clips_focus_get(uint64_t session);
    int32_t nxuskit_clips_focus_pop(uint64_t session);
    int32_t nxuskit_clips_focus_clear(uint64_t session);

    _Bool nxuskit_clips_global_exists(uint64_t session, const char *name);
    char *nxuskit_clips_global_list(uint64_t session);
    char *nxuskit_clips_global_get_value(uint64_t session, const char *name);
    int32_t nxuskit_clips_global_set_value(uint64_t session, const char *name,
                                           const char *value_json);

    char *nxuskit_clips_eval(uint64_t session, const char *expression);
    char *nxuskit_clips_function_call(uint64_t session, const char *function_name,
                                      const char *args_json);

    int32_t nxuskit_clips_watch(uint64_t session, const char *item);
    int32_t nxuskit_clips_unwatch(uint64_t session, const char *item);
    int32_t nxuskit_clips_dribble_on(uint64_t session, const char *path);
    int32_t nxuskit_clips_dribble_off(uint64_t session);

    _Bool nxuskit_clips_fact_duplication_get(uint64_t session);
    int32_t nxuskit_clips_fact_duplication_set(uint64_t session, _Bool allow);
    _Bool nxuskit_clips_reset_globals_get(uint64_t session);
    int32_t nxuskit_clips_reset_globals_set(uint64_t session, _Bool reset);

    int32_t nxuskit_clips_session_preload(const char *name, const char *rules_json);
    uint64_t nxuskit_clips_session_get_cached(const char *name);
    int32_t nxuskit_clips_session_cache_remove(const char *name);

    /* ── Constraint Solver ─────────────────────────────────────── */

    typedef struct NxuskitSolverSession NxuskitSolverSession;

    typedef int32_t (*NxuskitSolverStreamOnChunk)(
        const char *chunk_json, void *user_data
    );
    typedef void (*NxuskitSolverStreamOnDone)(
        const char *result_json, void *user_data
    );

    NxuskitSolverSession *nxuskit_solver_session_create(const char *config_json);
    void nxuskit_solver_session_destroy(NxuskitSolverSession *session);

    _Bool nxuskit_solver_add_variables(NxuskitSolverSession *s, const char *json);
    _Bool nxuskit_solver_add_constraints(NxuskitSolverSession *s, const char *json);
    _Bool nxuskit_solver_set_objective(NxuskitSolverSession *s, const char *json);
    _Bool nxuskit_solver_add_objective(NxuskitSolverSession *s, const char *json);
    _Bool nxuskit_solver_retract(NxuskitSolverSession *s, const char *json);
    _Bool nxuskit_solver_retract_objective(NxuskitSolverSession *s, const char *name);
    _Bool nxuskit_solver_add_assumptions(NxuskitSolverSession *s, const char *json);

    _Bool nxuskit_solver_push(NxuskitSolverSession *s);
    _Bool nxuskit_solver_pop(NxuskitSolverSession *s);
    _Bool nxuskit_solver_reset(NxuskitSolverSession *s);

    char *nxuskit_solver_solve(NxuskitSolverSession *s, const char *config_json);
    _Bool nxuskit_solver_solve_stream(
        NxuskitSolverSession *session,
        const char *config_json,
        NxuskitSolverStreamOnChunk on_chunk,
        NxuskitSolverStreamOnDone on_done,
        void *user_data
    );
    char *nxuskit_solver_explanation(NxuskitSolverSession *s);

    char *nxuskit_solver_variables(const NxuskitSolverSession *s);
    char *nxuskit_solver_constraints(const NxuskitSolverSession *s);
    char *nxuskit_solver_objectives(const NxuskitSolverSession *s);
    char *nxuskit_solver_status(const NxuskitSolverSession *s);
    char *nxuskit_solver_capabilities(const NxuskitSolverSession *s);
    int64_t nxuskit_solver_num_variables(const NxuskitSolverSession *s);
    int64_t nxuskit_solver_num_constraints(const NxuskitSolverSession *s);

    /* ── ZEN Decision Tables ───────────────────────────────────── */

    char *nxuskit_zen_evaluate(const char *model_json, const char *input_json);
""")

# ── Library discovery ─────────────────────────────────────────


def _lib_name() -> str:
    """Return the platform-specific library filename."""
    system = platform.system()
    if system == "Darwin":
        return "libnxuskit.dylib"
    elif system == "Windows":
        return "nxuskit.dll"
    else:
        return "libnxuskit.so"


def _find_library() -> str:
    """Locate the nxuskit shared library.

    Search order (aligned with Rust SDK):
      1. NXUSKIT_LIB_DIR environment variable (directory containing the library)
      2. NXUSKIT_SDK_DIR/lib (SDK root with /lib subdirectory)
      3. NXUSKIT_LIB_PATH environment variable (exact file path, legacy)
      4. Standard install path (~/.nxuskit/sdk/current/lib/)
      5. libs/ subdirectory next to this file (wheel distribution)
      6. Standard library search path (LD_LIBRARY_PATH, DYLD_LIBRARY_PATH, PATH)
    """
    lib = _lib_name()

    # 1. Explicit lib directory (Rust-aligned).
    lib_dir = os.environ.get("NXUSKIT_LIB_DIR")
    if lib_dir:
        p = Path(lib_dir) / lib
        if p.is_file():
            return str(p)
        # If user explicitly set the env var, don't silently fall through.
        if not Path(lib_dir).is_dir():
            raise _config_error(f"NXUSKIT_LIB_DIR points to non-existent directory: {lib_dir}")

    # 2. SDK root with /lib subdirectory (Rust-aligned).
    sdk_dir = os.environ.get("NXUSKIT_SDK_DIR")
    if sdk_dir:
        p = Path(sdk_dir) / "lib" / lib
        if p.is_file():
            return str(p)
        if not Path(sdk_dir).is_dir():
            raise _config_error(f"NXUSKIT_SDK_DIR points to non-existent directory: {sdk_dir}")

    # 3. Legacy exact-path override.
    env_path = os.environ.get("NXUSKIT_LIB_PATH")
    if env_path:
        p = Path(env_path)
        if p.is_file():
            warnings.warn(
                "NXUSKIT_LIB_PATH is deprecated. "
                "Use NXUSKIT_LIB_DIR (directory) or NXUSKIT_SDK_DIR (SDK root) instead.",
                DeprecationWarning,
                stacklevel=2,
            )
            return str(p)
        raise _config_error(f"NXUSKIT_LIB_PATH points to non-existent file: {env_path}")

    # 4. Standard install path.
    sdk_lib = Path.home() / ".nxuskit" / "sdk" / "current" / "lib" / lib
    if sdk_lib.is_file():
        return str(sdk_lib)

    # 5. Bundled in libs/ (wheel distribution).
    libs_dir = Path(__file__).parent / "libs"
    lib_file = libs_dir / lib
    if lib_file.is_file():
        return str(lib_file)

    # 6. System library path — let dlopen search.
    return lib


def _config_error(message: str) -> Exception:
    """Create a ConfigError (imported lazily to avoid circular imports)."""
    from nxuskit._ffi_errors import ConfigError

    return ConfigError(message)


# ── Load library ──────────────────────────────────────────────

_lib_path = _find_library()
try:
    lib = ffi.dlopen(_lib_path)
except OSError as e:
    raise _config_error(
        f"Failed to load nxuskit library ({_lib_path}): {e}\n"
        f"Platform: {platform.system()} {platform.machine()}\n"
        f"Python: {sys.version}\n"
        "Set NXUSKIT_LIB_DIR, NXUSKIT_SDK_DIR, or install the SDK at "
        "~/.nxuskit/sdk/current/."
    ) from e

# ── Version check ─────────────────────────────────────────────

_version_ptr = lib.nxuskit_version()
if _version_ptr == ffi.NULL:
    raise _config_error("nxuskit_version() returned NULL — library may be corrupted")

_actual_version = ffi.string(_version_ptr).decode("utf-8")
if _actual_version != EXPECTED_VERSION:
    raise _config_error(
        f"nxuskit version mismatch: expected {EXPECTED_VERSION}, got {_actual_version}"
    )


# ── Helper functions ──────────────────────────────────────────


def last_error() -> str | None:
    """Return the last error message from the nxuskit library (thread-local)."""
    ptr = lib.nxuskit_last_error()
    if ptr == ffi.NULL:
        return None
    return ffi.string(ptr).decode("utf-8")
