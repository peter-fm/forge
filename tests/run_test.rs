use forge::commands::init::{InitOptions, init_project};
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn run_with_task_creates_instruction_file_with_task_content() {
    let dir = tempdir().expect("tempdir");
    write_run_fixture(
        dir.path(),
        false,
        r#"
[blueprint]
name = "demo"
description = "x"

[[step]]
type = "agentic"
name = "implement"
agent = "{target_agent}"
model = "{target_model}"
prompt = "Read {instruction_path}"
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_forge"))
        .args([
            "run",
            "--blueprint",
            ".forge/blueprints/demo.toml",
            "--task",
            "Add WebSocket support",
            "--branch",
            "feat/add-websocket-support",
            "--no-dashboard",
            "--dry-run",
        ])
        .current_dir(dir.path())
        .output()
        .expect("run forge");

    assert!(output.status.success());
    let files = instruction_file_names(dir.path().join(".forge/instructions").as_path());
    assert_eq!(files.len(), 1);
    assert!(files[0].starts_with("add-websocket-support."));
    assert!(files[0].ends_with(".codex.md"));
    assert_eq!(
        fs::read_to_string(dir.path().join(".forge/instructions").join(&files[0]))
            .expect("read instruction"),
        "Add WebSocket support"
    );
}

#[test]
fn run_with_instruction_uses_existing_file_without_creating_new_one() {
    let dir = tempdir().expect("tempdir");
    write_run_fixture(
        dir.path(),
        false,
        r#"
[blueprint]
name = "demo"
description = "x"

[[step]]
type = "agentic"
name = "implement"
agent = "{target_agent}"
model = "{target_model}"
prompt = "Read {instruction_path}"
"#,
    );
    fs::create_dir_all(dir.path().join(".forge/instructions")).expect("instructions");
    fs::write(
        dir.path().join(".forge/instructions/existing-task.md"),
        "Use the human-written instructions",
    )
    .expect("write instruction");

    let output = Command::new(env!("CARGO_BIN_EXE_forge"))
        .args([
            "run",
            "--blueprint",
            ".forge/blueprints/demo.toml",
            "--instruction",
            "existing-task.md",
            "--no-dashboard",
            "--dry-run",
            "--verbose",
        ])
        .current_dir(dir.path())
        .output()
        .expect("run forge");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Read .forge/instructions/existing-task.md"));
    let files = instruction_file_names(dir.path().join(".forge/instructions").as_path());
    assert_eq!(files, vec!["existing-task.md".to_string()]);
}

#[test]
fn successful_run_archives_instruction_file() {
    let dir = tempdir().expect("tempdir");
    write_run_fixture(
        dir.path(),
        true,
        r#"
[blueprint]
name = "demo"
description = "x"

[[step]]
type = "agentic"
name = "implement"
agent = "{target_agent}"
model = "{target_model}"
prompt = "Read {instruction_path}"
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_forge"))
        .args([
            "run",
            "--blueprint",
            ".forge/blueprints/demo.toml",
            "--task",
            "Archive me",
            "--branch",
            "feat/archive-me",
            "--no-dashboard",
            "--dry-run",
        ])
        .current_dir(dir.path())
        .output()
        .expect("run forge");

    assert!(output.status.success());
    assert!(instruction_file_names(dir.path().join(".forge/instructions").as_path()).is_empty());
    let archived = archive_file_names(dir.path().join(".forge/archive").as_path());
    assert_eq!(archived.len(), 1);
    assert!(archived[0].starts_with("archive-me."));
    assert!(archived[0].contains(".done-"));
}

#[test]
fn failed_run_leaves_instruction_file_in_place() {
    let dir = tempdir().expect("tempdir");
    write_run_fixture(
        dir.path(),
        true,
        r#"
[blueprint]
name = "demo"
description = "x"

[[step]]
type = "deterministic"
name = "fail"
command = "false"
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_forge"))
        .args([
            "run",
            "--blueprint",
            ".forge/blueprints/demo.toml",
            "--task",
            "Do not archive me",
            "--branch",
            "feat/do-not-archive-me",
            "--no-dashboard",
        ])
        .current_dir(dir.path())
        .output()
        .expect("run forge");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("▶ fail ..."));
    assert!(stderr.contains("✗ fail (exit 1, "));
    let files = instruction_file_names(dir.path().join(".forge/instructions").as_path());
    assert_eq!(files.len(), 1);
    assert!(files[0].starts_with("do-not-archive-me."));
    assert!(archive_file_names(dir.path().join(".forge/archive").as_path()).is_empty());
}

#[test]
fn explicit_branch_uses_branch_leaf_for_instruction_file_slug() {
    let dir = tempdir().expect("tempdir");
    write_run_fixture(
        dir.path(),
        false,
        r#"
[blueprint]
name = "demo"
description = "x"

[[step]]
type = "agentic"
name = "implement"
agent = "{target_agent}"
model = "{target_model}"
prompt = "Read {instruction_path}"
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_forge"))
        .args([
            "run",
            "--blueprint",
            ".forge/blueprints/demo.toml",
            "--task",
            "Use explicit branch",
            "--branch",
            "fix/custom-instruction-slug",
            "--no-dashboard",
            "--dry-run",
        ])
        .current_dir(dir.path())
        .output()
        .expect("run forge");

    assert!(output.status.success());
    let files = instruction_file_names(dir.path().join(".forge/instructions").as_path());
    assert_eq!(files.len(), 1);
    assert!(files[0].starts_with("custom-instruction-slug."));
}

#[test]
fn generated_test_blueprint_runs_without_instruction_input() {
    let dir = tempdir().expect("tempdir");
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("write cargo");
    init_project(
        dir.path(),
        &InitOptions {
            project_type: None,
            force: false,
        },
    )
    .expect("init project");

    let output = Command::new(env!("CARGO_BIN_EXE_forge"))
        .args(["run", "test", "--no-dashboard", "--dry-run", "--verbose"])
        .current_dir(dir.path())
        .output()
        .expect("run forge test");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("[Deterministic] test -> Succeeded (0)"));
    assert!(stdout.contains("cargo test"));
}

#[test]
fn pr_review_next_uses_lowest_open_pull_request_number() {
    let dir = tempdir().expect("tempdir");
    write_run_fixture(
        dir.path(),
        false,
        r#"
[blueprint]
name = "demo"
description = "x"
"#,
    );
    fs::write(
        dir.path().join(".forge/blueprints/pr-review.toml"),
        r#"
[blueprint]
name = "pr-review"
description = "x"

[[step]]
type = "deterministic"
name = "show-pr"
command = "printf '{pr}\n'"
"#
        .trim_start(),
    )
    .expect("write pr-review blueprint");
    let bin_dir = dir.path().join("bin");
    fs::create_dir_all(&bin_dir).expect("create bin dir");
    fs::write(
        bin_dir.join("gh"),
        "#!/bin/sh\nif [ \"$1\" = \"pr\" ] && [ \"$2\" = \"list\" ]; then\n  printf '[{\"number\":4},{\"number\":5},{\"number\":6}]'\n  exit 0\nfi\necho \"unexpected gh command\" >&2\nexit 1\n",
    )
    .expect("write gh mock");
    let chmod = Command::new("chmod")
        .args(["+x", &bin_dir.join("gh").to_string_lossy()])
        .status()
        .expect("chmod gh mock");
    assert!(chmod.success());

    let path = std::env::var("PATH").unwrap_or_default();
    let output = Command::new(env!("CARGO_BIN_EXE_forge"))
        .args(["run", "pr-review", "--next", "--no-dashboard", "--dry-run"])
        .current_dir(dir.path())
        .env("PATH", format!("{}:{path}", bin_dir.to_string_lossy()))
        .output()
        .expect("run forge");

    assert!(output.status.success(), "{output:?}");
    let log_path = only_run_log(dir.path());
    let log = fs::read_to_string(log_path).expect("read run log");
    let entries = log
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("jsonl entry"))
        .collect::<Vec<_>>();
    assert_eq!(entries[0]["variables"]["pr"], "4");
}

#[test]
fn pr_review_latest_uses_highest_open_pull_request_number() {
    let dir = tempdir().expect("tempdir");
    write_run_fixture(
        dir.path(),
        false,
        r#"
[blueprint]
name = "demo"
description = "x"
"#,
    );
    fs::write(
        dir.path().join(".forge/blueprints/pr-review.toml"),
        r#"
[blueprint]
name = "pr-review"
description = "x"

[[step]]
type = "deterministic"
name = "show-pr"
command = "printf '{pr}\n'"
"#
        .trim_start(),
    )
    .expect("write pr-review blueprint");
    let bin_dir = dir.path().join("bin");
    fs::create_dir_all(&bin_dir).expect("create bin dir");
    fs::write(
        bin_dir.join("gh"),
        "#!/bin/sh\nif [ \"$1\" = \"pr\" ] && [ \"$2\" = \"list\" ]; then\n  printf '[{\"number\":4},{\"number\":5},{\"number\":6}]'\n  exit 0\nfi\necho \"unexpected gh command\" >&2\nexit 1\n",
    )
    .expect("write gh mock");
    let chmod = Command::new("chmod")
        .args(["+x", &bin_dir.join("gh").to_string_lossy()])
        .status()
        .expect("chmod gh mock");
    assert!(chmod.success());

    let path = std::env::var("PATH").unwrap_or_default();
    let output = Command::new(env!("CARGO_BIN_EXE_forge"))
        .args([
            "run",
            "pr-review",
            "--latest",
            "--no-dashboard",
            "--dry-run",
        ])
        .current_dir(dir.path())
        .env("PATH", format!("{}:{path}", bin_dir.to_string_lossy()))
        .output()
        .expect("run forge");

    assert!(output.status.success(), "{output:?}");
    let log_path = only_run_log(dir.path());
    let log = fs::read_to_string(log_path).expect("read run log");
    let entries = log
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("jsonl entry"))
        .collect::<Vec<_>>();
    assert_eq!(entries[0]["variables"]["pr"], "6");
}

#[test]
fn run_without_dry_run_logs_false_and_executes_steps() {
    let dir = tempdir().expect("tempdir");
    write_run_fixture(
        dir.path(),
        false,
        r#"
[blueprint]
name = "demo"
description = "x"

[[step]]
type = "deterministic"
name = "echo"
command = "printf 'ran\\n'"
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_forge"))
        .args([
            "run",
            "--blueprint",
            ".forge/blueprints/demo.toml",
            "--task",
            "Execute for real",
            "--no-dashboard",
        ])
        .current_dir(dir.path())
        .output()
        .expect("run forge");

    assert!(output.status.success(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("▶ echo ..."));
    assert!(stderr.contains("✓ echo ("));

    let log_path = only_run_log(dir.path());
    let log = fs::read_to_string(log_path).expect("read run log");
    let entries = log
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("jsonl entry"))
        .collect::<Vec<_>>();

    assert_eq!(entries[0]["type"], "run_start");
    assert_eq!(entries[0]["dry_run"], Value::Bool(false));
    assert_eq!(entries[1]["name"], "echo");
    assert_eq!(entries[1]["stdout"], "ran\n");
    let step_log = entries[1]["log_file"].as_str().expect("step log path");
    assert!(step_log.starts_with(".forge/runs/run-"));
    assert_eq!(
        fs::read_to_string(dir.path().join(step_log)).expect("read step log"),
        "ran\n"
    );
}

#[test]
fn run_injects_default_branch_variable() {
    let dir = tempdir().expect("tempdir");
    write_run_fixture(
        dir.path(),
        false,
        r#"
[blueprint]
name = "demo"
description = "x"

[[step]]
type = "deterministic"
name = "show-default-branch"
command = "echo {default_branch}"
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_forge"))
        .args([
            "run",
            "--blueprint",
            ".forge/blueprints/demo.toml",
            "--no-dashboard",
            "--dry-run",
            "--verbose",
        ])
        .current_dir(dir.path())
        .output()
        .expect("run forge");

    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("echo main"));
}

#[test]
fn resume_continues_from_first_unfinished_step() {
    let dir = tempdir().expect("tempdir");
    write_run_fixture(
        dir.path(),
        false,
        r#"
[blueprint]
name = "demo"
description = "x"

[[step]]
type = "deterministic"
name = "prepare"
command = "count=$(cat .forge/prepare-count 2>/dev/null || echo 0); count=$((count + 1)); printf '%s' \"$count\" > .forge/prepare-count"

[[step]]
type = "deterministic"
name = "finish"
command = "if [ \"$FORGE_TEST_FAIL\" = \"1\" ]; then echo fail >&2; exit 1; fi; printf 'done\\n'"
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_forge"))
        .args([
            "run",
            "--blueprint",
            ".forge/blueprints/demo.toml",
            "--no-dashboard",
        ])
        .current_dir(dir.path())
        .env("FORGE_TEST_FAIL", "1")
        .output()
        .expect("run forge");

    assert!(!output.status.success(), "{output:?}");
    let run_id = only_snapshot_id(dir.path());
    assert_eq!(
        fs::read_to_string(dir.path().join(".forge/prepare-count")).expect("prepare count"),
        "1"
    );

    let output = Command::new(env!("CARGO_BIN_EXE_forge"))
        .args(["resume", &run_id, "--no-dashboard"])
        .current_dir(dir.path())
        .output()
        .expect("resume forge");

    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        fs::read_to_string(dir.path().join(".forge/prepare-count")).expect("prepare count"),
        "1"
    );

    let snapshot = fs::read_to_string(
        dir.path()
            .join(".forge/runs")
            .join(format!("{run_id}.json")),
    )
    .expect("read snapshot");
    let snapshot: Value = serde_json::from_str(&snapshot).expect("snapshot json");
    assert_eq!(snapshot["status"], "succeeded");
}

#[test]
fn resume_reuses_codex_thread_id_when_available() {
    let dir = tempdir().expect("tempdir");
    write_run_fixture(
        dir.path(),
        false,
        r#"
[blueprint]
name = "demo"
description = "x"

[[step]]
type = "agentic"
name = "implement"
agent = "codex"
model = "gpt-5.4"
prompt = "Implement the feature"
"#,
    );
    let init = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status()
        .expect("git init");
    assert!(init.success());

    let bin_dir = dir.path().join("bin");
    fs::create_dir_all(&bin_dir).expect("create bin dir");
    fs::write(
        bin_dir.join("codex"),
        format!(
            "#!/bin/sh\nset -eu\nprintf '%s\\n' \"$*\" >> \"{}/codex-args.log\"\nif printf '%s' \"$*\" | grep -q 'exec resume'; then\n  printf '{{\"type\":\"turn.started\"}}\\n'\n  exit 0\nfi\nprintf '{{\"type\":\"thread.started\",\"thread_id\":\"thread-123\"}}\\n'\nprintf '{{\"type\":\"turn.started\"}}\\n'\nprintf '{{\"type\":\"turn.failed\",\"error\":{{\"message\":\"usage\"}}}}\\n'\nexit 1\n",
            dir.path().display()
        ),
    )
    .expect("write codex mock");
    let chmod = Command::new("chmod")
        .args(["+x", &bin_dir.join("codex").to_string_lossy()])
        .status()
        .expect("chmod codex mock");
    assert!(chmod.success());

    let path = std::env::var("PATH").unwrap_or_default();
    let first = Command::new(env!("CARGO_BIN_EXE_forge"))
        .args([
            "run",
            "--blueprint",
            ".forge/blueprints/demo.toml",
            "--no-dashboard",
        ])
        .current_dir(dir.path())
        .env("PATH", format!("{}:{path}", bin_dir.to_string_lossy()))
        .output()
        .expect("run forge");
    assert!(!first.status.success(), "{first:?}");

    let run_id = only_snapshot_id(dir.path());
    let snapshot = fs::read_to_string(
        dir.path()
            .join(".forge/runs")
            .join(format!("{run_id}.json")),
    )
    .expect("read snapshot");
    let snapshot: Value = serde_json::from_str(&snapshot).expect("snapshot json");
    assert_eq!(
        snapshot["step_results"][0]["agent_session_id"],
        "thread-123"
    );

    let resumed = Command::new(env!("CARGO_BIN_EXE_forge"))
        .args(["resume", &run_id, "--no-dashboard"])
        .current_dir(dir.path())
        .env("PATH", format!("{}:{path}", bin_dir.to_string_lossy()))
        .output()
        .expect("resume forge");
    assert!(resumed.status.success(), "{resumed:?}");

    let args = fs::read_to_string(dir.path().join("codex-args.log")).expect("read codex args");
    assert!(args.contains("exec --json Implement the feature"));
    assert!(args.contains("exec resume --json thread-123 Implement the feature"));
}

#[test]
fn resume_reuses_claude_session_id_when_available() {
    let dir = tempdir().expect("tempdir");
    write_run_fixture(
        dir.path(),
        false,
        r#"
[blueprint]
name = "demo"
description = "x"

[[step]]
type = "agentic"
name = "implement"
agent = "claude-code"
model = "claude-sonnet-4-6"
prompt = "Implement the feature"
"#,
    );

    let bin_dir = dir.path().join("bin");
    fs::create_dir_all(&bin_dir).expect("create bin dir");
    fs::write(
        bin_dir.join("claude"),
        format!(
            "#!/bin/sh\nset -eu\nprintf '%s\\n' \"$*\" >> \"{}/claude-args.log\"\nif printf '%s' \"$*\" | grep -q -- '--resume'; then\n  printf 'done\\n'\n  exit 0\nfi\nprintf 'usage exhausted\\n' >&2\nexit 1\n",
            dir.path().display()
        ),
    )
    .expect("write claude mock");
    let chmod = Command::new("chmod")
        .args(["+x", &bin_dir.join("claude").to_string_lossy()])
        .status()
        .expect("chmod claude mock");
    assert!(chmod.success());

    let path = std::env::var("PATH").unwrap_or_default();
    let first = Command::new(env!("CARGO_BIN_EXE_forge"))
        .args([
            "run",
            "--blueprint",
            ".forge/blueprints/demo.toml",
            "--no-dashboard",
        ])
        .current_dir(dir.path())
        .env("PATH", format!("{}:{path}", bin_dir.to_string_lossy()))
        .output()
        .expect("run forge");
    assert!(!first.status.success(), "{first:?}");

    let run_id = only_snapshot_id(dir.path());
    let snapshot = fs::read_to_string(
        dir.path()
            .join(".forge/runs")
            .join(format!("{run_id}.json")),
    )
    .expect("read snapshot");
    let snapshot: Value = serde_json::from_str(&snapshot).expect("snapshot json");
    let session_id = snapshot["step_results"][0]["agent_session_id"]
        .as_str()
        .expect("session id")
        .to_string();
    assert!(!session_id.is_empty());

    let resumed = Command::new(env!("CARGO_BIN_EXE_forge"))
        .args(["resume", &run_id, "--no-dashboard"])
        .current_dir(dir.path())
        .env("PATH", format!("{}:{path}", bin_dir.to_string_lossy()))
        .output()
        .expect("resume forge");
    assert!(resumed.status.success(), "{resumed:?}");

    let args = fs::read_to_string(dir.path().join("claude-args.log")).expect("read claude args");
    assert!(args.contains("--session-id"));
    assert!(args.contains(&session_id));
    assert!(args.contains(&format!("--resume {session_id}")));
}

fn write_run_fixture(root: &Path, auto_archive: bool, blueprint: &str) {
    fs::create_dir_all(root.join(".forge/blueprints")).expect("blueprints");
    fs::write(
        root.join(".forge/config.toml"),
        format!(
            r#"[agent]
default = "codex"
model = "gpt-5.4"

[workspace]
auto_archive = {auto_archive}
"#
        ),
    )
    .expect("write config");
    fs::write(
        root.join(".forge/blueprints/demo.toml"),
        blueprint.trim_start(),
    )
    .expect("write blueprint");
}

fn instruction_file_names(path: &Path) -> Vec<String> {
    let mut files = if path.exists() {
        fs::read_dir(path)
            .expect("instruction entries")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name != ".gitkeep")
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    files.sort();
    files
}

fn archive_file_names(path: &Path) -> Vec<String> {
    let mut files = if path.exists() {
        fs::read_dir(path)
            .expect("archive entries")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    files.sort();
    files
}

fn only_run_log(root: &Path) -> std::path::PathBuf {
    let mut files = walk(root.join(".forge/runs"))
        .into_iter()
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("jsonl"))
        .collect::<Vec<_>>();
    files.sort();
    assert_eq!(files.len(), 1, "expected exactly one run log");
    files.remove(0)
}

fn only_snapshot_id(root: &Path) -> String {
    let mut files = fs::read_dir(root.join(".forge/runs"))
        .expect("snapshot entries")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
        .collect::<Vec<_>>();
    files.sort();
    assert_eq!(files.len(), 1, "expected exactly one snapshot");
    files[0]
        .file_stem()
        .and_then(|value| value.to_str())
        .expect("snapshot id")
        .to_string()
}

fn walk(root: std::path::PathBuf) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![root];

    while let Some(path) = stack.pop() {
        for entry in fs::read_dir(path)
            .expect("walk entries")
            .filter_map(|entry| entry.ok())
        {
            let entry_path = entry.path();
            if entry_path.is_dir() {
                stack.push(entry_path);
            } else {
                files.push(entry_path);
            }
        }
    }

    files
}
