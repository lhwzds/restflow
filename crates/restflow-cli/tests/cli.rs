use assert_cmd::Command;
use predicates::str::{contains, starts_with};

#[test]
fn test_cli_help() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("restflow"));
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(contains("RestFlow"));
}

#[test]
fn test_cli_version() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("restflow"));
    cmd.arg("--version").assert().success();
}

#[test]
fn test_cli_completions() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("restflow"));
    cmd.args(["completions", "bash"])
        .assert()
        .success()
        .stdout(starts_with("_restflow"));
}
