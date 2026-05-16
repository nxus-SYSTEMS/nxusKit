//! Contract tests for pipeline command (T026) — FR-008

use assert_cmd::Command;

fn cli() -> Command {
    Command::cargo_bin("nxuskit-cli").unwrap()
}

// ── (a) 3-stage pipeline with loopback → all stages completed ───────

#[test]
fn pipeline_run_three_stages_all_complete() {
    let pipeline = r#"{
        "name": "test-pipeline",
        "stages": [
            {"name": "stage1", "type": "llm", "config": {"prompt": "hello", "provider": "loopback", "model": "echo"}},
            {"name": "stage2", "type": "llm", "config": {"prompt": "world", "provider": "loopback", "model": "echo"}},
            {"name": "stage3", "type": "llm", "config": {"prompt": "done", "provider": "loopback", "model": "echo"}}
        ]
    }"#;

    let mut cmd = cli();
    cmd.args(["pipeline", "run", "--input", "-", "--format", "json"])
        .write_stdin(pipeline);

    let output = cmd.output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let resp: serde_json::Value = serde_json::from_str(stdout.trim()).expect("valid JSON");

    let stages = resp["result"]["stages"].as_array().expect("stages array");
    assert_eq!(stages.len(), 3);
    for stage in stages {
        assert_eq!(stage["status"], "completed");
    }
    assert_eq!(resp["result"]["summary"]["completed"], 3);
    assert_eq!(resp["result"]["summary"]["failed"], 0);
}

// ── (b) Pipeline with Pro stage without Pro → halt ──────────────────

#[test]
fn pipeline_pro_stage_halts_with_entitlement_error() {
    let pipeline = r#"{
        "name": "pro-pipeline",
        "stages": [
            {"name": "stage1", "type": "llm", "config": {"prompt": "ok", "provider": "loopback", "model": "echo"}},
            {"name": "stage2", "type": "zen_eval", "config": {}},
            {"name": "stage3", "type": "llm", "config": {"prompt": "never", "provider": "loopback", "model": "echo"}}
        ]
    }"#;

    let mut cmd = cli();
    cmd.args(["pipeline", "run", "--input", "-"])
        .write_stdin(pipeline)
        .env_remove("NXUSKIT_LICENSE_KEY")
        .env_remove("NXUSKIT_LICENSE_TOKEN")
        .env("HOME", "/tmp/nxuskit-no-license");

    let output = cmd.output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(1),
        "Expected pipeline_stage_failed exit 1"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    let err: serde_json::Value = serde_json::from_str(stderr.trim()).expect("stderr JSON");
    assert_eq!(err["code"], "internal");
}

// ── (c) JSONL streaming mode emits stage_complete events ────────────

#[test]
fn pipeline_jsonl_emits_stage_complete_events() {
    let pipeline = r#"{
        "name": "stream-pipeline",
        "stages": [
            {"name": "s1", "type": "llm", "config": {"prompt": "a", "provider": "loopback", "model": "echo"}},
            {"name": "s2", "type": "llm", "config": {"prompt": "b", "provider": "loopback", "model": "echo"}}
        ]
    }"#;

    let mut cmd = cli();
    cmd.args(["pipeline", "run", "--input", "-", "--format", "jsonl"])
        .write_stdin(pipeline);

    let output = cmd.output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.trim().lines().collect();

    // Should have stage_complete events
    let stage_events: Vec<serde_json::Value> = lines
        .iter()
        .filter_map(|line| serde_json::from_str(line).ok())
        .filter(|v: &serde_json::Value| v["type"] == "stage_complete")
        .collect();

    assert_eq!(stage_events.len(), 2, "Expected 2 stage_complete events");
}

// ═══════════════════════════════════════════════════════════════════
// Phase 4 — Pipeline semantic tests (T018–T020, T023)
// ═══════════════════════════════════════════════════════════════════

// ── T018: Multi-engine pipeline (LLM + CLIPS) — all stages complete ──

#[test]
fn pipeline_multi_engine_all_stages_complete() {
    let pipeline = r#"
name: multi-engine-test
stages:
  - name: generate
    type: llm
    config:
      prompt: "generate a temperature reading"
      provider: loopback
      model: echo
  - name: evaluate_rules
    type: clips_eval
    config:
      rules: "(deftemplate temperature (slot value)) (deftemplate action (slot type)) (defrule hot (temperature (value ?t&:(> ?t 30))) => (assert (action (type \"cool\"))))"
      facts:
        - "(temperature (value 35))"
  - name: summarize
    type: llm
    config:
      prompt: "summarize the results"
      provider: loopback
      model: echo
"#;

    let mut cmd = cli();
    cmd.args(["pipeline", "run", "--input", "-", "--format", "json"])
        .write_stdin(pipeline);

    let output = cmd.output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let resp: serde_json::Value = serde_json::from_str(stdout.trim()).expect("valid JSON");

    let stages = resp["result"]["stages"].as_array().expect("stages array");
    assert_eq!(stages.len(), 3, "Expected 3 stages");
    for stage in stages {
        assert_eq!(
            stage["status"], "completed",
            "Stage '{}' should be completed, got: {}",
            stage["name"], stage["status"]
        );
    }

    // CLIPS stage should have non-stub results
    let clips_stage = &stages[1];
    assert!(
        clips_stage["result"]["fired_rules"]
            .as_u64()
            .is_some_and(|n| n > 0),
        "CLIPS stage should have fired_rules > 0, got: {:?}",
        clips_stage["result"]
    );

    assert_eq!(resp["result"]["summary"]["completed"], 3);
    assert_eq!(resp["result"]["summary"]["failed"], 0);
}

// ── T019: Pipeline with Pro-gated stage → partial results on failure ──

#[test]
fn pipeline_partial_results_on_failure() {
    let pipeline = r#"{
        "name": "partial-fail-pipeline",
        "stages": [
            {"name": "stage1", "type": "llm", "config": {"prompt": "ok", "provider": "loopback", "model": "echo"}},
            {"name": "stage2", "type": "zen_eval", "config": {}},
            {"name": "stage3", "type": "llm", "config": {"prompt": "never", "provider": "loopback", "model": "echo"}}
        ]
    }"#;

    let mut cmd = cli();
    cmd.args(["pipeline", "run", "--input", "-"])
        .write_stdin(pipeline)
        .env_remove("NXUSKIT_LICENSE_KEY")
        .env_remove("NXUSKIT_LICENSE_TOKEN")
        .env("HOME", "/tmp/nxuskit-no-license");

    let output = cmd.output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(1),
        "Expected exit 1 for pipeline failure"
    );

    // stdout should have partial results envelope
    let stdout = String::from_utf8_lossy(&output.stdout);
    let resp: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("stdout JSON with partial results");

    let stages = resp["result"]["stages"].as_array().expect("stages array");
    assert_eq!(stages.len(), 3, "All 3 stages should be reported");

    // stage1 completed
    assert_eq!(stages[0]["status"], "completed");
    // stage2 failed
    assert_eq!(stages[1]["status"], "failed");
    // stage3 skipped
    assert_eq!(stages[2]["status"], "skipped");

    // Summary should reflect the breakdown
    assert_eq!(resp["result"]["summary"]["completed"], 1);
    assert_eq!(resp["result"]["summary"]["failed"], 1);
    assert_eq!(resp["result"]["summary"]["skipped"], 1);

    // stderr should also have the error
    let stderr = String::from_utf8_lossy(&output.stderr);
    let err: serde_json::Value = serde_json::from_str(stderr.trim()).expect("stderr JSON");
    assert_eq!(err["code"], "internal");
}

// ── T020: Pipeline output_key handoff between stages ──────────────────

#[test]
fn pipeline_output_key_handoff() {
    let pipeline = r#"{
        "name": "output-key-pipeline",
        "stages": [
            {"name": "generate", "type": "llm", "config": {"prompt": "hello", "provider": "loopback", "model": "echo"}, "output_key": "llm_result"},
            {"name": "summarize", "type": "llm", "config": {"prompt": "summarize {{llm_result}}", "provider": "loopback", "model": "echo"}}
        ]
    }"#;

    let mut cmd = cli();
    cmd.args(["pipeline", "run", "--input", "-", "--format", "json"])
        .write_stdin(pipeline);

    let output = cmd.output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let resp: serde_json::Value = serde_json::from_str(stdout.trim()).expect("valid JSON");

    let stages = resp["result"]["stages"].as_array().expect("stages array");
    assert_eq!(stages.len(), 2);
    for stage in stages {
        assert_eq!(stage["status"], "completed");
    }
}

// ── YAML pipeline input works ───────────────────────────────────────

#[test]
fn pipeline_yaml_input_works() {
    let pipeline_yaml = r#"
name: yaml-pipeline
stages:
  - name: greet
    type: llm
    config:
      prompt: hello
      provider: loopback
      model: echo
"#;

    let mut cmd = cli();
    cmd.args(["pipeline", "run", "--input", "-", "--format", "json"])
        .write_stdin(pipeline_yaml);

    let output = cmd.output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let resp: serde_json::Value = serde_json::from_str(stdout.trim()).expect("valid JSON");
    assert_eq!(resp["result"]["summary"]["completed"], 1);
}
