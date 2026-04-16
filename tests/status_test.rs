use forge::model::{Blueprint, BlueprintMeta, RunContext, StepResult, StepStatus, StepType};
use forge::run_status::write_snapshot;
use std::collections::BTreeMap;
use std::fs;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn forge_status_prints_snapshot_summary() {
    let dir = tempdir().expect("tempdir");
    fs::create_dir_all(dir.path().join(".forge/runs")).expect("forge dir");
    let mut context = RunContext::new();
    context.run_id = Some("new-feature-a3f2".to_string());
    context.instruction_file = Some("add-status-output.2026-03-31T1325.codex.md".to_string());
    context.run_started_at = Some(10);
    context
        .variables
        .insert("target_agent".to_string(), "codex".to_string());
    context.step_started_at =
        BTreeMap::from([("step-0001".to_string(), 10), ("step-0002".to_string(), 12)]);
    context.step_results.insert(
        "step-0001".to_string(),
        StepResult {
            step_id: "step-0001".to_string(),
            name: "implement".to_string(),
            step_type: StepType::Agentic,
            status: StepStatus::Succeeded,
            exit_code: 0,
            stdout: String::new(),
            stderr: String::new(),
            attempts: 1,
            agent_session_id: None,
            log_file: None,
        },
    );
    write_snapshot(
        &dir.path().join(".forge/runs/new-feature-a3f2.json"),
        &Blueprint {
            blueprint: BlueprintMeta {
                name: "new-feature".to_string(),
                description: "x".to_string(),
                repos: Vec::new(),
            },
            steps: Vec::new(),
            source_path: Some(dir.path().join(".forge/blueprints/new-feature.toml")),
        },
        &["implement".to_string(), "test".to_string()],
        &context,
        Some("step-0002"),
        "running",
    )
    .expect("write snapshot");

    let output = Command::new(env!("CARGO_BIN_EXE_forge"))
        .arg("status")
        .current_dir(dir.path())
        .output()
        .expect("run forge status");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("run id: new-feature-a3f2"));
    assert!(stdout.contains("blueprint: new-feature"));
    assert!(stdout.contains("status: running"));
    assert!(stdout.contains("instruction file: add-status-output.2026-03-31T1325.codex.md"));
    assert!(stdout.contains("agent: codex"));
    assert!(stdout.contains("current step: test"));
    assert!(stdout.contains("implement: succeeded"));
    assert!(stdout.contains("test: running"));
}

#[test]
fn forge_status_shows_multiple_runs_and_all_flag() {
    let dir = tempdir().expect("tempdir");
    fs::create_dir_all(dir.path().join(".forge/runs")).expect("forge dir");

    let running = serde_json::json!({
        "id": "new-feature-a3f2",
        "blueprint": "new-feature",
        "instruction_file": "task-one.2026-03-31T1325.codex.md",
        "agent": "codex",
        "status": "running",
        "started_at": "2026-03-31T13:25:00Z",
        "updated_at": "2026-03-31T13:30:00Z",
        "finished_at": null,
        "steps": [{ "name": "implement", "status": "running", "started_at": "2026-03-31T13:25:00Z", "finished_at": null, "attempts": 1 }]
    });
    let finished = serde_json::json!({
        "id": "fix-bug-b7c1",
        "blueprint": "fix-bug",
        "instruction_file": "task-two.2026-03-31T1330.codex.md",
        "agent": "codex",
        "status": "succeeded",
        "started_at": "2026-03-31T13:30:00Z",
        "updated_at": "2026-03-31T13:40:00Z",
        "finished_at": "2026-03-31T13:40:00Z",
        "steps": [{ "name": "fix", "status": "succeeded", "started_at": "2026-03-31T13:30:00Z", "finished_at": "2026-03-31T13:40:00Z", "attempts": 1 }]
    });
    fs::write(
        dir.path().join(".forge/runs/new-feature-a3f2.json"),
        serde_json::to_string_pretty(&running).expect("serialize"),
    )
    .expect("write running");
    fs::write(
        dir.path().join(".forge/runs/fix-bug-b7c1.json"),
        serde_json::to_string_pretty(&finished).expect("serialize"),
    )
    .expect("write finished");

    let output = Command::new(env!("CARGO_BIN_EXE_forge"))
        .arg("status")
        .current_dir(dir.path())
        .output()
        .expect("run forge status");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("run id: new-feature-a3f2"));
    assert!(!stdout.contains("run id: fix-bug-b7c1"));

    let output = Command::new(env!("CARGO_BIN_EXE_forge"))
        .args(["status", "--all"])
        .current_dir(dir.path())
        .output()
        .expect("run forge status --all");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("run id: new-feature-a3f2"));
    assert!(stdout.contains("run id: fix-bug-b7c1"));
    assert!(stdout.find("run id: fix-bug-b7c1") < stdout.find("run id: new-feature-a3f2"));
}

#[test]
fn forge_status_supports_latest_and_limit() {
    let dir = tempdir().expect("tempdir");
    fs::create_dir_all(dir.path().join(".forge/runs")).expect("forge dir");

    let snapshots = [
        (
            "run-old",
            "new-feature",
            "running",
            "2026-03-31T13:25:00Z",
            "2026-03-31T13:30:00Z",
        ),
        (
            "run-mid",
            "fix-bug",
            "succeeded",
            "2026-03-31T13:35:00Z",
            "2026-03-31T13:40:00Z",
        ),
        (
            "run-new",
            "refactor",
            "failed",
            "2026-03-31T13:45:00Z",
            "2026-03-31T13:50:00Z",
        ),
    ];

    for (id, blueprint, status, started_at, updated_at) in snapshots {
        let snapshot = serde_json::json!({
            "id": id,
            "blueprint": blueprint,
            "instruction_file": format!("{id}.md"),
            "agent": "codex",
            "status": status,
            "started_at": started_at,
            "updated_at": updated_at,
            "finished_at": if status == "running" { serde_json::Value::Null } else { serde_json::Value::String(updated_at.to_string()) },
            "steps": [{ "name": "step", "status": status, "started_at": started_at, "finished_at": updated_at, "attempts": 1 }]
        });
        fs::write(
            dir.path().join(".forge/runs").join(format!("{id}.json")),
            serde_json::to_string_pretty(&snapshot).expect("serialize"),
        )
        .expect("write snapshot");
    }

    let output = Command::new(env!("CARGO_BIN_EXE_forge"))
        .args(["status", "--all", "--latest"])
        .current_dir(dir.path())
        .output()
        .expect("run forge status --all --latest");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("run id: run-new"));
    assert!(!stdout.contains("run id: run-mid"));
    assert!(!stdout.contains("run id: run-old"));

    let output = Command::new(env!("CARGO_BIN_EXE_forge"))
        .args(["status", "--all", "-n", "2"])
        .current_dir(dir.path())
        .output()
        .expect("run forge status --all -n 2");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("run id: run-new"));
    assert!(stdout.contains("run id: run-mid"));
    assert!(!stdout.contains("run id: run-old"));
    assert!(stdout.find("run id: run-new") < stdout.find("run id: run-mid"));
}

#[test]
fn forge_status_latest_and_limit_include_completed_by_default() {
    let dir = tempdir().expect("tempdir");
    fs::create_dir_all(dir.path().join(".forge/runs")).expect("forge dir");

    let finished = serde_json::json!({
        "id": "run-finished",
        "blueprint": "fix-bug",
        "instruction_file": "finished.md",
        "agent": "codex",
        "status": "succeeded",
        "started_at": "2026-03-31T13:30:00Z",
        "updated_at": "2026-03-31T13:40:00Z",
        "finished_at": "2026-03-31T13:40:00Z",
        "steps": [{ "name": "fix", "status": "succeeded", "started_at": "2026-03-31T13:30:00Z", "finished_at": "2026-03-31T13:40:00Z", "attempts": 1 }]
    });
    fs::write(
        dir.path().join(".forge/runs/run-finished.json"),
        serde_json::to_string_pretty(&finished).expect("serialize"),
    )
    .expect("write finished");

    let output = Command::new(env!("CARGO_BIN_EXE_forge"))
        .args(["status", "--latest"])
        .current_dir(dir.path())
        .output()
        .expect("run forge status --latest");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("run id: run-finished"));

    let output = Command::new(env!("CARGO_BIN_EXE_forge"))
        .args(["status", "-n", "1"])
        .current_dir(dir.path())
        .output()
        .expect("run forge status -n 1");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("run id: run-finished"));
}

#[test]
fn forge_list_prints_available_blueprints() {
    let dir = tempdir().expect("tempdir");
    fs::create_dir_all(dir.path().join(".forge/blueprints/common")).expect("blueprints");
    fs::write(
        dir.path().join(".forge/blueprints/new-feature.toml"),
        "[blueprint]\nname = \"new-feature\"\ndescription = \"x\"\n",
    )
    .expect("write blueprint");
    fs::write(
        dir.path().join(".forge/blueprints/common/lint.toml"),
        "[blueprint]\nname = \"lint\"\ndescription = \"x\"\n",
    )
    .expect("write blueprint");

    let output = Command::new(env!("CARGO_BIN_EXE_forge"))
        .arg("list")
        .current_dir(dir.path())
        .output()
        .expect("run forge list");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("common/lint"));
    assert!(stdout.contains("new-feature"));
}
