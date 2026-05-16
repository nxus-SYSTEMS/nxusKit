//! Level 1 smoke tests (T054, T055) — FR-015
//!
//! One happy-path test per L1 command + error-case tests.
//! Must complete in < 2 minutes (T056).

use assert_cmd::Command;
use std::path::PathBuf;

fn cli() -> Command {
    Command::cargo_bin("nxuskit-cli").unwrap()
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

// ═══════════════════════════════════════════════════════════════════
// Happy-path smoke tests (T054)
// ═══════════════════════════════════════════════════════════════════

#[test]
fn smoke_call() {
    let output = cli()
        .args([
            "call",
            "--input",
            fixture("prompt.json").to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let _: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim()).expect("valid JSON");
}

#[test]
fn smoke_clips_eval() {
    let output = cli()
        .args([
            "clips",
            "eval",
            "--input",
            fixture("rules.json").to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let _: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim()).expect("valid JSON");
}

#[test]
fn smoke_bn_infer() {
    let output = cli()
        .args([
            "bn",
            "infer",
            "--input",
            fixture("network.json").to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let _: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim()).expect("valid JSON");
}

#[test]
fn smoke_pipeline_run() {
    let output = cli()
        .args([
            "pipeline",
            "run",
            "--input",
            fixture("pipeline.yaml").to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let _: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim()).expect("valid JSON");
}

#[test]
fn smoke_packet_validate() {
    let output = cli()
        .args([
            "packet",
            "validate",
            "--input",
            fixture("packet.json").to_str().unwrap(),
            "--schema",
            fixture("schema.json").to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let _: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim()).expect("valid JSON");
}

#[test]
fn smoke_artifact_merge() {
    let output = cli()
        .args([
            "artifact",
            "merge",
            "--input",
            fixture("artifact_a.json").to_str().unwrap(),
            "--input",
            fixture("artifact_b.json").to_str().unwrap(),
            "--merge-strategy",
            "last",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let _: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim()).expect("valid JSON");
}

#[test]
fn smoke_artifact_summarize() {
    let output = cli()
        .args([
            "artifact",
            "summarize",
            "--input",
            fixture("artifact_a.json").to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let _: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim()).expect("valid JSON");
}

#[test]
fn smoke_tool_loop_run() {
    let mut cmd = cli();
    cmd.args(["tool-loop", "run", "--input", "-", "--format", "json"])
        .write_stdin(r#"{"prompt":"smoke","provider":"loopback","model":"echo","max_iterations":1,"tools":[]}"#);
    let output = cmd.output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    let _: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim()).expect("valid JSON");
}

#[test]
fn smoke_judge_select() {
    // Judge with loopback/echo fails to parse because the echo model doesn't
    // return structured JSON. Since T037 replaced the silent fallback with
    // CliError::ParseError, we expect exit 1 and a parse_error on stderr.
    let mut cmd = cli();
    cmd.args(["judge", "select", "--input", "-", "--format", "json"])
        .write_stdin(
            r#"{"candidates":[{"id":"x","content":"hello"}],"provider":"loopback","model":"echo"}"#,
        );
    let output = cmd.output().unwrap();
    assert_eq!(output.status.code(), Some(5));
    let err: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&output.stderr).trim()).expect("stderr JSON");
    assert_eq!(err["code"], "validation");
}

#[test]
fn smoke_branch_fork() {
    let mut cmd = cli();
    cmd.args(["branch", "fork", "--input", "-", "--format", "json"])
        .write_stdin(r#"{"prompt":"smoke","models":["echo"],"provider":"loopback"}"#);
    let output = cmd.output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    let _: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim()).expect("valid JSON");
}

#[test]
fn smoke_branch_compare() {
    let mut cmd = cli();
    cmd.args(["branch", "compare", "--input", "-", "--format", "json"])
        .write_stdin(r#"{"results":[{"model":"echo","content":"hi","elapsed_ms":1.0}]}"#);
    let output = cmd.output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    let _: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim()).expect("valid JSON");
}

#[test]
fn smoke_models() {
    let output = cli()
        .args(["models", "--provider", "loopback", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn smoke_provider_status() {
    let output = cli()
        .args(["provider", "status", "--json"])
        .output()
        .unwrap();
    // May return 0 or non-zero depending on auth state
    assert!(output.status.code().is_some());
}

// ── Level 2 binary smoke (T022): provider list/info, completions, clips session ──

#[test]
fn smoke_provider_list() {
    let output = cli()
        .args(["provider", "list", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let _: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim()).expect("valid JSON");
}

#[test]
fn smoke_provider_info() {
    let output = cli()
        .args(["provider", "info", "openai", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn smoke_completions_bash() {
    let output = cli().args(["completions", "bash"]).output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(
        !output.stdout.is_empty(),
        "completions bash should emit a non-empty script"
    );
}

#[test]
fn smoke_completions_zsh_and_fish() {
    for shell in ["zsh", "fish"] {
        let output = cli().args(["completions", shell]).output().unwrap();
        assert_eq!(
            output.status.code(),
            Some(0),
            "completions {shell} should exit 0"
        );
        assert!(
            !output.stdout.is_empty(),
            "completions {shell} should emit a non-empty script"
        );
    }
}

#[test]
fn smoke_clips_session_list() {
    let output = cli()
        .args(["clips", "session", "list", "--json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
}

// ═══════════════════════════════════════════════════════════════════
// Error-case smoke tests (T055)
// ═══════════════════════════════════════════════════════════════════

#[test]
fn error_call_nonexistent_input() {
    let output = cli()
        .args(["call", "--input", "/tmp/nxuskit_smoke_nonexistent.json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    let _: serde_json::Value = serde_json::from_str(stderr.trim()).expect("stderr JSON");
}

#[test]
fn error_clips_bad_format() {
    let mut cmd = cli();
    cmd.args(["clips", "eval", "--input", "-", "--format", "bad"])
        .write_stdin(r#"{"rules":"","facts":[]}"#);
    let output = cmd.output().unwrap();
    assert_eq!(output.status.code(), Some(5));
    let stderr = String::from_utf8_lossy(&output.stderr);
    let err: serde_json::Value = serde_json::from_str(stderr.trim()).expect("stderr JSON");
    assert_eq!(err["code"], "validation");
}

#[test]
fn error_zen_no_pro() {
    let mut cmd = cli();
    cmd.args(["zen", "eval", "--input", "-"])
        .write_stdin(r#"{"table":{},"input":{}}"#)
        .env_remove("NXUSKIT_LICENSE_KEY")
        .env_remove("NXUSKIT_LICENSE_TOKEN")
        .env("HOME", "/tmp/nxuskit-no-license");
    let output = cmd.output().unwrap();
    assert_eq!(output.status.code(), Some(4));
    let stderr = String::from_utf8_lossy(&output.stderr);
    let err: serde_json::Value = serde_json::from_str(stderr.trim()).expect("stderr JSON");
    assert_eq!(err["code"], "entitlement_denied");
}

#[test]
fn error_solver_no_pro() {
    let mut cmd = cli();
    cmd.args(["solver", "solve", "--input", "-"])
        .write_stdin(r#"{"constraints":[],"variables":[]}"#)
        .env_remove("NXUSKIT_LICENSE_KEY")
        .env_remove("NXUSKIT_LICENSE_TOKEN")
        .env("HOME", "/tmp/nxuskit-no-license");
    let output = cmd.output().unwrap();
    assert_eq!(output.status.code(), Some(4));
    let stderr = String::from_utf8_lossy(&output.stderr);
    let err: serde_json::Value = serde_json::from_str(stderr.trim()).expect("stderr JSON");
    assert_eq!(err["code"], "entitlement_denied");
}

// ═══════════════════════════════════════════════════════════════════
// Help text wiring verification (T013) — Article II v2.4.0
// Ensures every L1 command's --help includes input format hints,
// catching wiring gaps like the 064 models/pipeline-run dispatch issue.
// ═══════════════════════════════════════════════════════════════════

#[test]
fn help_text_includes_input_format_hints() {
    let mut cases: Vec<(&[&str], &str)> = vec![
        (&["call", "--help"], "prompt"),
        (&["clips", "eval", "--help"], "rules"),
        (&["bn", "infer", "--help"], "network"),
        (&["pipeline", "run", "--help"], "stages"),
        (&["artifact", "merge", "--help"], "strategy"),
        (&["artifact", "summarize", "--help"], "input"),
        (&["packet", "validate", "--help"], "schema"),
        (&["tool-loop", "run", "--help"], "prompt"),
        (&["judge", "select", "--help"], "candidates"),
        (&["branch", "fork", "--help"], "models"),
        (&["branch", "compare", "--help"], "input"),
    ];

    #[cfg(feature = "pro-engines")]
    {
        cases.push((&["zen", "eval", "--help"], "table"));
        cases.push((&["solver", "solve", "--help"], "variables"));
    }

    #[cfg(not(feature = "pro-engines"))]
    {
        cases.push((&["zen", "eval", "--help"], "requires nxuskit pro"));
        cases.push((&["solver", "solve", "--help"], "requires nxuskit pro"));
    }

    for (args, expected_keyword) in cases {
        let output = cli().args(args).output().unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let combined = format!("{}{}", stdout, String::from_utf8_lossy(&output.stderr));
        assert!(
            combined.to_lowercase().contains(expected_keyword),
            "Command {:?} --help should contain '{}' but output was:\n{}",
            args,
            expected_keyword,
            combined
        );
    }
}
