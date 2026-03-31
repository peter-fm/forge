use forge::model::{RunContext, StepResult, StepStatus, StepType};
use forge::run_status::write_snapshot;
use std::collections::BTreeMap;
use std::fs;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn forge_status_prints_snapshot_summary() {
    let dir = tempdir().expect("tempdir");
    fs::create_dir_all(dir.path().join(".forge")).expect("forge dir");
    let mut context = RunContext::new();
    context.run_started_at = Some(10);
    context.step_started_at =
        BTreeMap::from([("implement".to_string(), 10), ("test".to_string(), 12)]);
    context.step_results.insert(
        "implement".to_string(),
        StepResult {
            name: "implement".to_string(),
            step_type: StepType::Agentic,
            status: StepStatus::Succeeded,
            exit_code: 0,
            stdout: String::new(),
            stderr: String::new(),
            attempts: 1,
        },
    );
    write_snapshot(
        &dir.path().join(".forge/.run-status.json"),
        "new-feature",
        &["implement".to_string(), "test".to_string()],
        &context,
        Some("test"),
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
    assert!(stdout.contains("blueprint: new-feature"));
    assert!(stdout.contains("state: running"));
    assert!(stdout.contains("current step: test"));
    assert!(stdout.contains("implement: succeeded"));
    assert!(stdout.contains("test: running"));
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
