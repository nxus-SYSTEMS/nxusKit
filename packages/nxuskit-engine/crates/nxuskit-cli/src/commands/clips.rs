//! `nxuskit-cli clips eval|session` — CLIPS rule evaluation and session lifecycle (FR-004, FR-014).

use std::collections::HashMap;
use std::sync::Mutex;

use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};

use crate::cli_error::CliError;
use crate::envelope::TraceFields;
use crate::output::{OutputFormat, OutputWriter};

#[derive(Debug, Subcommand)]
pub enum ClipsAction {
    /// Evaluate CLIPS rules against facts
    Eval(ClipsEvalArgs),
    /// Manage CLIPS inference sessions
    Session {
        #[command(subcommand)]
        action: ClipsSessionAction,
    },
}

#[derive(Debug, Subcommand)]
pub enum ClipsSessionAction {
    /// Create a new CLIPS inference session
    Create(ClipsSessionCreateArgs),
    /// List all active CLIPS sessions
    List(ClipsSessionListArgs),
    /// Destroy a CLIPS session by ID
    Destroy(ClipsSessionDestroyArgs),
}

#[derive(Debug, Args)]
pub struct ClipsEvalArgs {
    /// Input file or `-` for stdin.
    ///
    /// JSON: {"rules": "(defrule ...)", "facts": ["(fact1)", ...]}
    /// Note: use \\n for newlines in rule strings.
    #[arg(short, long)]
    pub input: String,

    /// Output format
    #[arg(short, long, default_value = "json")]
    pub format: String,

    /// Suppress non-essential output
    #[arg(short, long, default_value_t = false)]
    pub quiet: bool,

    /// Output file path
    #[arg(short, long)]
    pub output: Option<String>,
}

// ── Session args ──────────────────────────────────────────────────────

#[derive(Debug, Args)]
pub struct ClipsSessionCreateArgs {
    /// Output format
    #[arg(short, long, default_value = "json")]
    pub format: String,

    /// Shorthand for --format json
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ClipsSessionListArgs {
    /// Output format
    #[arg(short, long, default_value = "json")]
    pub format: String,

    /// Shorthand for --format json
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ClipsSessionDestroyArgs {
    /// Session ID to destroy (hex string from `session create`)
    pub session_id: String,

    /// Output format
    #[arg(short, long, default_value = "json")]
    pub format: String,

    /// Shorthand for --format json
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

// ── Session response types ───────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct SessionCreateResult {
    pub session_id: String,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct SessionListResult {
    pub sessions: Vec<SessionSummary>,
    pub count: usize,
    pub limit: Option<u64>,
    pub tier: String,
}

#[derive(Debug, Serialize)]
pub struct SessionSummary {
    pub session_id: String,
    pub rule_count: u64,
    pub fact_count: u64,
}

#[derive(Debug, Serialize)]
pub struct SessionDestroyResult {
    pub destroyed: bool,
    pub message: String,
}

// ── CLI session registry ─────────────────────────────────────────────

struct CliSession {
    env: clips_sys::ClipsEnvironment,
    #[allow(dead_code)]
    created_at: String,
}

static CLI_SESSIONS: std::sync::LazyLock<Mutex<HashMap<String, CliSession>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

fn next_session_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{:016x}", id)
}

fn now_iso8601() -> String {
    nxuskit_core::entitlement::chrono_now_iso_public()
}

// ── Eval types ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ClipsEvalInput {
    pub rules: String,
    #[serde(default)]
    pub facts: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct MatchedRule {
    pub name: String,
    pub times_fired: u64,
}

#[derive(Debug, Serialize)]
pub struct DerivedFact {
    pub template: String,
    pub slots: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct ClipsEvalResult {
    pub matched_rules: Vec<MatchedRule>,
    pub derived_facts: Vec<DerivedFact>,
    pub agenda_count: u32,
    pub fired_rules: u32,
}

pub async fn run_clips_command(action: ClipsAction) -> Result<(), CliError> {
    match action {
        ClipsAction::Eval(args) => run_clips_eval(args).await,
        ClipsAction::Session { action } => run_clips_session_command(action).await,
    }
}

async fn run_clips_session_command(action: ClipsSessionAction) -> Result<(), CliError> {
    match action {
        ClipsSessionAction::Create(args) => run_clips_session_create(args).await,
        ClipsSessionAction::List(args) => run_clips_session_list(args).await,
        ClipsSessionAction::Destroy(args) => run_clips_session_destroy(args).await,
    }
}

async fn run_clips_session_create(args: ClipsSessionCreateArgs) -> Result<(), CliError> {
    let fmt = if args.json { "json" } else { &args.format };
    let format = OutputFormat::parse(fmt)?;
    let writer = OutputWriter::new(format, false, None);

    // Check session limit
    let limits = nxuskit_core::entitlement::effective_limits(None);
    let sessions = CLI_SESSIONS.lock().unwrap();
    let current_count = sessions.len() as u64;
    if let Some(max) = limits.max_sessions
        && current_count >= max
    {
        let tier = nxuskit_core::entitlement::entitlement_info(None)
            .get("effective_edition")
            .and_then(|v| v.as_str())
            .unwrap_or("community")
            .to_string();
        drop(sessions);
        return Err(CliError::EntitlementRequired {
            required_edition: "pro".to_string(),
            current_edition: tier,
        });
    }
    drop(sessions);

    // Create CLIPS environment
    let env = clips_sys::ClipsEnvironment::new().map_err(|e| CliError::ProviderError {
        message: format!("Failed to create CLIPS environment: {e}"),
    })?;

    let session_id = next_session_id();
    let created_at = now_iso8601();

    let result = SessionCreateResult {
        session_id: session_id.clone(),
        created_at: created_at.clone(),
    };

    CLI_SESSIONS
        .lock()
        .unwrap()
        .insert(session_id, CliSession { env, created_at });

    let trace = TraceFields::new("clips_session_create", "", None, None);
    writer.write_response(result, trace, None, None, None)
}

async fn run_clips_session_list(args: ClipsSessionListArgs) -> Result<(), CliError> {
    let fmt = if args.json { "json" } else { &args.format };
    let format = OutputFormat::parse(fmt)?;
    let writer = OutputWriter::new(format, false, None);

    let sessions = CLI_SESSIONS.lock().unwrap();

    let entries: Vec<SessionSummary> = sessions
        .iter()
        .map(|(id, sess)| {
            let fact_count = sess.env.facts().filter_map(|f| f.ok()).count() as u64;
            let rule_count = sess.env.rules().filter_map(|r| r.ok()).count() as u64;
            SessionSummary {
                session_id: id.clone(),
                rule_count,
                fact_count,
            }
        })
        .collect();

    let count = entries.len();
    let limits = nxuskit_core::entitlement::effective_limits(None);
    let tier = nxuskit_core::entitlement::entitlement_info(None)
        .get("effective_edition")
        .and_then(|v| v.as_str())
        .unwrap_or("community")
        .to_string();

    drop(sessions);

    let result = SessionListResult {
        sessions: entries,
        count,
        limit: limits.max_sessions,
        tier,
    };

    let trace = TraceFields::new("clips_session_list", "", None, None);
    writer.write_response(result, trace, None, None, None)
}

async fn run_clips_session_destroy(args: ClipsSessionDestroyArgs) -> Result<(), CliError> {
    let fmt = if args.json { "json" } else { &args.format };
    let format = OutputFormat::parse(fmt)?;
    let writer = OutputWriter::new(format, false, None);

    let mut sessions = CLI_SESSIONS.lock().unwrap();
    let removed = sessions.remove(&args.session_id);
    drop(sessions);

    if removed.is_none() {
        return Err(CliError::ValidationFailed {
            message: format!(
                "Session '{}' not found. Run `clips session list` to see active sessions.",
                args.session_id
            ),
        });
    }

    let result = SessionDestroyResult {
        destroyed: true,
        message: format!("Session {} destroyed", args.session_id),
    };

    let trace = TraceFields::new("clips_session_destroy", "", None, None);
    writer.write_response(result, trace, None, None, None)
}

async fn run_clips_eval(args: ClipsEvalArgs) -> Result<(), CliError> {
    let format = OutputFormat::parse(&args.format)?;
    let raw_input = OutputWriter::read_input(&args.input)?;
    let eval_input: ClipsEvalInput =
        serde_json::from_str(&raw_input).map_err(|e| CliError::ParseError {
            message: format!("Invalid CLIPS eval input: {e}"),
        })?;

    let trace = TraceFields::new("clips_eval", &raw_input, None, None);
    let writer = OutputWriter::new(format, args.quiet, args.output);

    let start = std::time::Instant::now();

    // Create CLIPS environment and evaluate
    let env = clips_sys::ClipsEnvironment::new().map_err(|e| CliError::ProviderError {
        message: format!("Failed to create CLIPS environment: {e}"),
    })?;

    // Load rules from the input string
    env.load_from_string(&eval_input.rules)
        .map_err(|e| CliError::ProviderError {
            message: format!("Failed to load CLIPS rules: {e}"),
        })?;

    // Assert facts
    for fact in &eval_input.facts {
        env.assert_string(fact)
            .map_err(|e| CliError::ProviderError {
                message: format!("Failed to assert fact '{fact}': {e}"),
            })?;
    }

    // Run the agenda
    let run_result = env.run(None).map_err(|e| CliError::ProviderError {
        message: format!("Failed to run CLIPS agenda: {e}"),
    })?;

    // Collect all defined rules.
    // Note: CLIPS 6.4.x does not track per-rule firing counts, so we report
    // all rules that exist post-run. The `fired_rules` field (from RunResult)
    // indicates total activations that fired.
    let matched_rules: Vec<MatchedRule> = env
        .rules()
        .filter_map(|r| r.ok())
        .filter_map(|r| {
            let name = r.name().ok()?;
            Some(MatchedRule {
                name,
                times_fired: r.times_fired(),
            })
        })
        .collect();

    // Collect derived facts with template and slot data
    let derived_facts: Vec<DerivedFact> = env
        .facts()
        .filter_map(|f| f.ok())
        .filter_map(|f| {
            let template = f.template_name().ok()?;
            let slots = f
                .slot_values()
                .ok()
                .map(|sv| {
                    let map: serde_json::Map<String, serde_json::Value> = sv
                        .into_iter()
                        .map(|(k, v)| (k, clips_value_to_json(&v)))
                        .collect();
                    serde_json::Value::Object(map)
                })
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
            Some(DerivedFact { template, slots })
        })
        .collect();

    // Get real agenda count (should be 0 after run completes, but report actual value)
    let agenda_count = env.agenda_size() as u32;

    let elapsed = start.elapsed().as_secs_f64() * 1000.0;

    let result = ClipsEvalResult {
        matched_rules,
        derived_facts,
        agenda_count,
        fired_rules: run_result.rules_fired as u32,
    };

    writer.write_response(result, trace, None, None, Some(elapsed))
}

/// Convert a CLIPS value to a serde_json::Value.
pub(crate) fn clips_value_to_json(v: &clips_sys::ClipsValue) -> serde_json::Value {
    match v {
        clips_sys::ClipsValue::Void => serde_json::Value::Null,
        clips_sys::ClipsValue::Integer(i) => serde_json::json!(i),
        clips_sys::ClipsValue::Float(f) => serde_json::json!(f),
        clips_sys::ClipsValue::Symbol(s) => serde_json::json!(s),
        clips_sys::ClipsValue::String(s) => serde_json::json!(s),
        clips_sys::ClipsValue::Boolean(b) => serde_json::json!(b),
        clips_sys::ClipsValue::Multifield(items) => {
            serde_json::Value::Array(items.iter().map(clips_value_to_json).collect())
        }
        clips_sys::ClipsValue::FactAddress(idx) => serde_json::json!(format!("<Fact-{}>", idx)),
        clips_sys::ClipsValue::InstanceAddress(name) => serde_json::json!(format!("[{}]", name)),
        clips_sys::ClipsValue::ExternalAddress(addr) => {
            serde_json::json!(format!("<ExternalAddress-{:x}>", addr))
        }
    }
}
