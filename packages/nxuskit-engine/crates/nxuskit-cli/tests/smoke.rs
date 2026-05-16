use assert_cmd::Command;
use predicates::prelude::*;

fn cli() -> Command {
    Command::cargo_bin("nxuskit-cli").unwrap()
}

// T027: --help verification for all public subcommands

#[test]
fn help_root() {
    cli()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("JSON-first control plane"));
}

#[test]
fn help_chat() {
    cli()
        .args(["chat", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Send a chat message"));
}

#[test]
fn help_models() {
    cli()
        .args(["models", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("List available models"));
}

#[test]
fn help_capabilities() {
    cli()
        .args(["capabilities", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Get model capabilities"));
}

#[test]
fn help_schema() {
    cli()
        .args(["schema", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("schema"));
}

#[test]
fn help_pipeline() {
    cli()
        .args(["pipeline", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("pipeline"));
}

// T028: invalid flag / error path tests

#[test]
fn invalid_subcommand() {
    cli()
        .arg("nonexistent-command")
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

#[test]
fn capabilities_unknown_provider() {
    cli()
        .args(["capabilities", "-p", "badprovider", "test-model"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Unknown provider"));
}

#[test]
fn capabilities_loopback_default_fallback() {
    cli()
        .args(["capabilities", "-p", "loopback", "-f", "json", "test-model"])
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""source": "default""#));
}
