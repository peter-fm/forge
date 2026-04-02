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
            "--no-dashboard",
        ])
        .current_dir(dir.path())
        .output()
        .expect("run forge");

    assert!(!output.status.success());
    let files = instruction_file_names(dir.path().join(".forge/instructions").as_path());
    assert_eq!(files.len(), 1);
    assert!(files[0].starts_with("do-not-archive-me."));
    assert!(archive_file_names(dir.path().join(".forge/archive").as_path()).is_empty());
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
    let mut files = fs::read_dir(root.join(".forge/runs"))
        .expect("run entries")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("jsonl"))
        .collect::<Vec<_>>();
    files.sort();
    assert_eq!(files.len(), 1, "expected exactly one run log");
    files.remove(0)
}
