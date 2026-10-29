//! End-to-end tests of the `rok-tools` binary.

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::{Value, json};
use std::fs;
use tempfile::TempDir;

fn rok_tools(dir: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("rok-tools").unwrap();
    cmd.current_dir(dir.path());
    cmd
}

fn json_out(cmd: &mut Command) -> Value {
    let out = cmd.output().unwrap();
    serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&out.stdout)))
}

#[test]
fn version_and_help() {
    let dir = TempDir::new().unwrap();
    rok_tools(&dir)
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("rok-tools"));
    rok_tools(&dir)
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("loc"));
}

#[test]
fn usage_errors_exit_2() {
    let dir = TempDir::new().unwrap();
    rok_tools(&dir).assert().code(2);
    rok_tools(&dir).arg("frobnicate").assert().code(2);
    rok_tools(&dir)
        .args(["grep"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("PATTERN"));
}

#[test]
fn list_shows_tools() {
    let dir = TempDir::new().unwrap();
    rok_tools(&dir)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("dead-code"));
    let rows = json_out(rok_tools(&dir).args(["list", "--json"]));
    assert_eq!(rows.as_array().unwrap().len(), 26);
    assert!(
        rows.as_array()
            .unwrap()
            .iter()
            .all(|r| r["enabled"] == true)
    );
}

#[test]
fn grep_text_and_json() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.txt"), "line one\nTODO fix\n").unwrap();
    rok_tools(&dir)
        .args(["grep", "TODO"])
        .assert()
        .success()
        .stdout("a.txt:2: TODO fix\n");
    let out = json_out(rok_tools(&dir).args(["grep", "--json", "todo", "-i"]));
    assert_eq!(out["matches"][0]["line"], 2);
}

#[test]
fn tool_failures_exit_1() {
    let dir = TempDir::new().unwrap();
    rok_tools(&dir)
        .args(["read", "missing.txt"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn cwd_flag() {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join("sub")).unwrap();
    fs::write(dir.path().join("sub/x.rs"), "fn x() {}\n").unwrap();
    let out = json_out(rok_tools(&dir).args(["-C", "sub", "loc", "--json"]));
    assert_eq!(out["files"][0]["path"], "x.rs");
}

#[test]
fn loc_check_uses_config_then_flags() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("big.rs"), "let a = 1;\n".repeat(5)).unwrap();
    rok_tools(&dir).args(["loc", "--check"]).assert().success();

    fs::write(
        dir.path().join("rok.toml"),
        "[database]\nurl = 'ignored'\n\n[tools.loc]\nmax_lines = 3\n",
    )
    .unwrap();
    rok_tools(&dir)
        .args(["loc", "--check"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains(
            "big.rs: 5 lines of code (limit 3)",
        ));
    rok_tools(&dir)
        .args(["loc", "--check", "--max-lines", "10"])
        .assert()
        .success();

    fs::write(
        dir.path().join("rok.toml"),
        "[tools.loc]\nmax_lines = 3\n[[tools.loc.override]]\npath = 'big.rs'\nmax_lines = 0\n",
    )
    .unwrap();
    rok_tools(&dir).args(["loc", "--check"]).assert().success();

    fs::write(
        dir.path().join("strict.toml"),
        "[tools.loc]\nmax_lines = 1\ncheck = true\n",
    )
    .unwrap();
    rok_tools(&dir)
        .args(["loc", "--config", "strict.toml"])
        .assert()
        .code(1);
}

#[test]
fn bad_config_exits_2() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("rok.toml"), "[tools.loc\n").unwrap();
    rok_tools(&dir).args(["loc"]).assert().code(2);
}

#[test]
fn run_inline_file_and_stdin() {
    let dir = TempDir::new().unwrap();
    let payload = json!({ "steps": [
        { "type": "write", "path": "test.txt", "content": "hello world" },
        { "type": "read", "path": "test.txt" }
    ] });
    let out = json_out(rok_tools(&dir).args(["run", "--json", "-p", &payload.to_string()]));
    assert_eq!(out["status"], "ok");
    assert_eq!(
        out["results"][1]["output"]["files"][0]["content"],
        "hello world"
    );

    fs::write(
        dir.path().join("task.json"),
        json!({ "steps": [{ "type": "mkdir", "path": "made" }] }).to_string(),
    )
    .unwrap();
    rok_tools(&dir)
        .args(["run", "-f", "task.json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[OK]"));
    assert!(dir.path().join("made").is_dir());

    rok_tools(&dir)
        .args(["run", "--json"])
        .write_stdin(json!({ "steps": [{ "type": "mkdir", "path": "from-stdin" }] }).to_string())
        .assert()
        .success();
    assert!(dir.path().join("from-stdin").is_dir());
}

#[test]
fn run_bash_with_permission_and_dependencies() {
    let dir = TempDir::new().unwrap();
    let payload = json!({ "options": { "allowShell": true }, "steps": [
        { "type": "bash", "id": "step1", "cmd": "echo first" },
        { "type": "bash", "id": "step2", "depends_on": ["step1"], "cmd": "echo second" }
    ] });
    let out = json_out(rok_tools(&dir).args(["run", "--json", "-p", &payload.to_string()]));
    assert_eq!(out["stepsOk"], 2);
    assert!(
        out["results"][1]["output"]["stdout"]
            .as_str()
            .unwrap()
            .contains("second")
    );

    let denied = json!({ "steps": [{ "type": "bash", "cmd": "echo hi" }] });
    rok_tools(&dir)
        .args(["run", "-p", &denied.to_string()])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("allow_shell"));
}

#[test]
fn run_control_flow() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("exists.txt"), "").unwrap();
    let payload = json!({ "steps": [
        { "type": "if", "condition": { "type": "exists", "path": "exists.txt" },
          "then": [{ "type": "mkdir", "path": "yes" }], "else": [{ "type": "mkdir", "path": "no" }] },
        { "type": "if", "condition": { "type": "exists", "path": "nonexistent.txt" },
          "then": [{ "type": "mkdir", "path": "no2" }], "else": [{ "type": "mkdir", "path": "yes2" }] },
        { "type": "each", "over": ["a", "b", "c"], "step": { "type": "mkdir", "path": "each/{{item}}" } }
    ] });
    rok_tools(&dir)
        .args(["run", "-p", &payload.to_string()])
        .assert()
        .success();
    for d in ["yes", "yes2", "each/a", "each/b", "each/c"] {
        assert!(dir.path().join(d).is_dir(), "{d}");
    }
    assert!(!dir.path().join("no").exists() && !dir.path().join("no2").exists());
}

#[test]
fn run_dry_run_executes_nothing() {
    let dir = TempDir::new().unwrap();
    let payload = json!({ "steps": [{ "type": "mkdir", "path": "nope" }] });
    rok_tools(&dir)
        .args(["run", "--dry-run", "-p", &payload.to_string()])
        .assert()
        .success()
        .stdout(predicate::str::contains("0: mkdir"));
    assert!(!dir.path().join("nope").exists());
}

#[test]
fn run_invalid_or_missing_payload_exits_2() {
    let dir = TempDir::new().unwrap();
    rok_tools(&dir)
        .args(["run", "-p", "{\"steps\": invalid json}"])
        .assert()
        .code(2);
    rok_tools(&dir)
        .args(["run"])
        .write_stdin("")
        .assert()
        .code(2);
}

#[test]
fn embedded_program_name() {
    // `rok tools …` in rok-data forwards its tokens with its own program name.
    let code = rok_tools_cli::run_from(["rok tools", "list"]);
    assert_eq!(code, 0);
}
