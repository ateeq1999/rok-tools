//! Payload parsing and Runner behaviour (ported from the old `schema.rs` and `runner.rs` suites).
#![cfg(all(feature = "runner", feature = "full"))]

use rok_tools::runner::*;
use serde_json::json;
use std::fs;
use tempfile::TempDir;

fn run_in(dir: &TempDir, payload: serde_json::Value) -> RunOutput {
    let mut payload: Payload = serde_json::from_value(payload).unwrap();
    payload.options.history = false;
    Runner::in_dir(payload, dir.path()).unwrap().run()
}

fn echo(text: &str) -> serde_json::Value {
    json!({ "type": "bash", "cmd": format!("echo {text}") })
}

#[test]
fn parses_steps_with_snake_and_camel_case_options() {
    let payload = Payload::from_json(
        r#"{
            "name": "test-task",
            "options": { "cwd": "./src", "stopOnError": false, "timeout_ms": 60000, "env": { "NODE_ENV": "production" } },
            "steps": [
                { "type": "bash", "id": "step2", "depends_on": ["step1"], "cmd": "echo hello",
                  "retry": { "count": 3, "delayMs": 2000, "backoff": true } },
                { "type": "grep", "pattern": "TODO", "path": "./src", "ext": ["rs"], "regex": true }
            ]
        }"#,
    )
    .unwrap();
    assert_eq!(payload.name.as_deref(), Some("test-task"));
    assert_eq!(payload.options.cwd, "./src");
    assert!(!payload.options.stop_on_error);
    assert_eq!(payload.options.timeout_ms, Some(60000));
    assert_eq!(payload.options.env["NODE_ENV"], "production");
    let bash = &payload.steps[0];
    assert_eq!((bash.kind.as_str(), bash.id.as_str()), ("bash", "step2"));
    assert_eq!(bash.depends_on, vec!["step1"]);
    let retry = bash.retry.as_ref().unwrap();
    assert_eq!(
        (retry.count, retry.delay_ms, retry.backoff),
        (3, 2000, true)
    );
    assert_eq!(bash.args["cmd"], "echo hello");
    assert_eq!(payload.steps[1].args["ext"], json!(["rs"]));
}

#[test]
fn default_options() {
    let o = RunOptions::default();
    assert_eq!(o.cwd, ".");
    assert!(o.stop_on_error && o.history);
    assert!(!o.cache && !o.allow_shell && !o.allow_net);
    assert!(o.timeout_ms.is_none());
}

#[test]
fn runs_steps_in_payload_order() {
    let dir = TempDir::new().unwrap();
    let out = run_in(
        &dir,
        json!({
            "options": { "allowShell": true },
            "steps": [
                { "type": "write", "path": "log.txt", "content": "1" },
                { "type": "patch", "path": "log.txt", "edits": [{ "find": "1", "replace": "12" }] },
                { "type": "read", "path": "log.txt" }
            ]
        }),
    );
    assert_eq!(out.status, "ok");
    assert_eq!(out.steps_ok, 3);
    assert_eq!(
        out.results[2].output.as_ref().unwrap()["files"][0]["content"],
        "12"
    );
}

#[test]
fn dependencies_reorder_and_failures_skip_dependents() {
    let dir = TempDir::new().unwrap();
    let out = run_in(
        &dir,
        json!({
            "options": { "allowShell": true, "stopOnError": false },
            "steps": [
                { "type": "read", "id": "second", "depends_on": ["first"], "path": "made.txt" },
                { "type": "write", "id": "first", "path": "made.txt", "content": "x" },
                { "type": "bash", "id": "bad", "cmd": "exit 1" },
                { "type": "mkdir", "depends_on": ["bad"], "path": "never" }
            ]
        }),
    );
    assert!(
        out.results[0].is_ok(),
        "dependency ran first: {:?}",
        out.results[0]
    );
    assert_eq!(out.results[2].status, StepStatus::Error);
    assert_eq!(out.results[3].status, StepStatus::Skipped);
    assert!(!dir.path().join("never").exists());
    assert_eq!(out.status, "partial");
}

#[test]
fn unknown_dependency_and_cycles_fail_every_step() {
    let dir = TempDir::new().unwrap();
    let out = run_in(
        &dir,
        json!({ "steps": [{ "type": "mkdir", "path": "x", "depends_on": ["ghost"] }] }),
    );
    assert!(out.results[0].error.as_ref().unwrap().contains("ghost"));
    let out = run_in(
        &dir,
        json!({ "steps": [
        { "type": "mkdir", "id": "a", "path": "a", "depends_on": ["b"] },
        { "type": "mkdir", "id": "b", "path": "b", "depends_on": ["a"] }
    ] }),
    );
    assert!(
        out.results
            .iter()
            .all(|r| r.error.as_ref().unwrap().contains("cycle"))
    );
}

#[test]
fn stop_on_error_skips_the_rest() {
    let dir = TempDir::new().unwrap();
    let out = run_in(
        &dir,
        json!({ "steps": [
        { "type": "read", "path": "missing.txt" },
        { "type": "mkdir", "path": "should-not-exist" }
    ] }),
    );
    assert_eq!((out.steps_failed, out.steps_skipped), (1, 1));
    assert!(!dir.path().join("should-not-exist").exists());
}

#[test]
fn shell_and_network_need_permission() {
    let dir = TempDir::new().unwrap();
    let out = run_in(&dir, json!({ "steps": [echo("hi")] }));
    assert!(
        out.results[0]
            .error
            .as_ref()
            .unwrap()
            .contains("allow_shell")
    );
    let out = run_in(
        &dir,
        json!({ "steps": [{ "type": "http", "url": "http://127.0.0.1:9" }] }),
    );
    assert!(out.results[0].error.as_ref().unwrap().contains("allow_net"));
    let out = run_in(
        &dir,
        json!({ "options": { "allowShell": true }, "steps": [echo("hi")] }),
    );
    assert!(out.is_ok());
}

#[test]
fn if_branches_on_conditions() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("exists.txt"), "needle").unwrap();
    let out = run_in(
        &dir,
        json!({ "steps": [
        { "type": "grep", "id": "g", "pattern": "needle", "path": "." },
        { "type": "if",
          "condition": { "type": "and", "conditions": [
              { "type": "exists", "path": "exists.txt" },
              { "type": "grepHasResults", "ref": "g" },
              { "type": "not", "condition": { "type": "exists", "path": "nope" } },
              { "type": "contains", "path": "exists.txt", "pattern": "ne+dle", "regex": true }
          ] },
          "then": [{ "type": "mkdir", "path": "then" }],
          "else": [{ "type": "mkdir", "path": "else" }] }
    ] }),
    );
    assert!(out.is_ok());
    assert_eq!(out.results[1].output.as_ref().unwrap()["branch"], "then");
    assert!(dir.path().join("then").exists() && !dir.path().join("else").exists());
}

#[test]
fn each_over_list_and_reference() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.txt"), "TODO").unwrap();
    fs::write(dir.path().join("b.txt"), "TODO").unwrap();
    let out = run_in(
        &dir,
        json!({ "steps": [
        { "type": "each", "over": ["x", "y"], "as": "name", "parallel": false,
          "step": { "type": "mkdir", "path": "dirs/{{name}}" } },
        { "type": "grep", "pattern": "TODO", "path": "." },
        { "type": "each", "over": { "ref": 1, "pick": "matches[*].path" },
          "step": { "type": "cp", "from": "{{item}}", "to": "copies/{{item}}" } }
    ] }),
    );
    assert!(out.is_ok(), "{}", out.to_text());
    assert!(dir.path().join("dirs/x").is_dir() && dir.path().join("dirs/y").is_dir());
    assert!(dir.path().join("copies/a.txt").is_file() && dir.path().join("copies/b.txt").is_file());
    assert_eq!(out.results[2].results.len(), 2);
}

#[test]
fn parallel_reports_partial_failure() {
    let dir = TempDir::new().unwrap();
    let out = run_in(
        &dir,
        json!({ "options": { "stopOnError": false }, "steps": [
        { "type": "parallel", "steps": [{ "type": "mkdir", "path": "p" }, { "type": "read", "path": "missing" }] }
    ] }),
    );
    assert_eq!(out.results[0].status, StepStatus::Partial);
    assert_eq!(out.results[0].results.len(), 2);
}

#[test]
fn env_substitution() {
    let dir = TempDir::new().unwrap();
    let out = run_in(
        &dir,
        json!({
            "options": { "env": { "TARGET": "from-env" } },
            "steps": [{ "type": "mkdir", "path": "{{env.TARGET}}" }]
        }),
    );
    assert!(out.is_ok());
    assert!(dir.path().join("from-env").is_dir());
}

#[test]
fn not_enabled_and_unknown_tools() {
    let dir = TempDir::new().unwrap();
    let out = run_in(&dir, json!({ "steps": [{ "type": "frobnicate" }] }));
    assert!(
        out.results[0]
            .error
            .as_ref()
            .unwrap()
            .contains("unknown tool")
    );
}

#[test]
fn retry_and_timeout() {
    let dir = TempDir::new().unwrap();
    let out = run_in(
        &dir,
        json!({ "options": { "allowShell": true, "stopOnError": false }, "steps": [
        { "type": "bash", "cmd": "exit 1", "retry": { "count": 2, "delayMs": 1 } },
        { "type": "bash", "cmd": if cfg!(windows) { "ping -n 3 127.0.0.1 >NUL" } else { "sleep 2" }, "timeout_ms": 100 }
    ] }),
    );
    assert_eq!(out.results[0].status, StepStatus::Error);
    assert!(out.results[1].error.as_ref().unwrap().contains("timed out"));
}

#[test]
fn checkpoint_and_resume() {
    let dir = TempDir::new().unwrap();
    let first = run_in(
        &dir,
        json!({ "steps": [
        { "type": "write", "path": "count.txt", "content": "1" },
        { "type": "checkpoint", "checkpoint_id": "cp1" },
        { "type": "read", "path": "missing.txt" }
    ] }),
    );
    assert_eq!(first.steps_failed, 1);
    let cp = load_checkpoint(dir.path(), "cp1").unwrap();
    assert_eq!(cp.completed.len(), 1);

    fs::write(dir.path().join("missing.txt"), "now here").unwrap();
    fs::write(dir.path().join("count.txt"), "edited").unwrap();
    let resumed = run_in(
        &dir,
        json!({ "options": { "resume": "cp1" }, "steps": [
        { "type": "write", "path": "count.txt", "content": "1" },
        { "type": "checkpoint", "checkpoint_id": "cp1" },
        { "type": "read", "path": "missing.txt" }
    ] }),
    );
    assert!(resumed.is_ok(), "{}", resumed.to_text());
    assert_eq!(
        fs::read_to_string(dir.path().join("count.txt")).unwrap(),
        "edited",
        "step 0 was not re-run"
    );
}

#[test]
fn cache_reuses_successful_results() {
    let dir = TempDir::new().unwrap();
    let payload =
        json!({ "options": { "cache": true }, "steps": [{ "type": "loc", "path": "." }] });
    fs::write(dir.path().join("a.rs"), "fn a() {}\n").unwrap();
    let first = run_in(&dir, payload.clone());
    fs::write(dir.path().join("b.rs"), "fn b() {}\n").unwrap();
    let second = run_in(&dir, payload);
    assert_eq!(
        first.results[0].output, second.results[0].output,
        "served from cache"
    );
}

#[test]
fn incremental_skips_unchanged_paths() {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/a.rs"), "fn a() {}\n").unwrap();
    let payload =
        json!({ "options": { "incremental": true }, "steps": [{ "type": "loc", "path": "src" }] });
    assert_eq!(
        run_in(&dir, payload.clone()).results[0].status,
        StepStatus::Ok
    );
    assert_eq!(run_in(&dir, payload).results[0].status, StepStatus::Skipped);
}

#[test]
fn history_is_recorded() {
    let dir = TempDir::new().unwrap();
    let payload =
        Payload::from_json(r#"{"name":"h","steps":[{"type":"mkdir","path":"x"}]}"#).unwrap();
    Runner::in_dir(payload, dir.path()).unwrap().run();
    let h = history(dir.path());
    assert_eq!(h.len(), 1);
    assert_eq!(h[0].name.as_deref(), Some("h"));
}

#[test]
fn plan_lists_steps_in_execution_order() {
    let payload = Payload::from_json(
        r#"{"steps":[{"type":"read","depends_on":["w"],"path":"a"},{"type":"write","id":"w","path":"a","content":""}]}"#,
    )
    .unwrap();
    let plan = Runner::in_dir(payload, &std::env::current_dir().unwrap())
        .unwrap()
        .plan();
    assert!(plan[0].starts_with("1: write (w)"));
    assert!(plan[1].starts_with("0: read"));
}
