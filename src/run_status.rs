use crate::error::ForgeError;
use crate::model::{RunContext, StepResult, StepStatus};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunStatusSnapshot {
    #[serde(default)]
    pub id: String,
    pub blueprint: String,
    #[serde(default)]
    pub instruction_file: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub current_step: Option<String>,
    pub status: String,
    pub started_at: String,
    pub updated_at: String,
    pub finished_at: Option<String>,
    pub steps: Vec<StatusStep>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusStep {
    pub name: String,
    pub status: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub attempts: u32,
}

pub fn write_snapshot(
    path: &Path,
    blueprint: &str,
    step_names: &[String],
    context: &RunContext,
    current_step: Option<&str>,
    state: &str,
) -> Result<(), ForgeError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let now_iso = iso_timestamp(None)?;
    let started_at = iso_timestamp(context.run_started_at)?;
    let mut steps = step_names
        .iter()
        .map(|name| status_step(name, context, current_step))
        .collect::<Vec<_>>();

    for (name, result) in &context.step_results {
        if steps.iter().any(|step| step.name == *name) {
            continue;
        }
        steps.push(status_step_from_result(name, result, context));
    }

    let snapshot = RunStatusSnapshot {
        id: context.run_id.clone().unwrap_or_default(),
        blueprint: blueprint.to_string(),
        instruction_file: context.instruction_file.clone(),
        agent: context.variables.get("target_agent").cloned(),
        current_step: current_step.map(str::to_string),
        status: state.to_string(),
        started_at,
        updated_at: now_iso.clone(),
        finished_at: matches!(state, "succeeded" | "failed").then_some(now_iso),
        steps,
    };
    let output = serde_json::to_string_pretty(&snapshot)?;
    fs::write(path, output)?;
    Ok(())
}

pub fn read_snapshot(path: &Path) -> Result<RunStatusSnapshot, ForgeError> {
    let input = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&input)?)
}

pub fn list_snapshots(root: &Path) -> Result<Vec<RunStatusSnapshot>, ForgeError> {
    let runs_dir = root.join(".forge/runs");
    if !runs_dir.exists() {
        let legacy = root.join(".forge/.run-status.json");
        if legacy.exists() {
            return Ok(vec![read_snapshot(&legacy)?]);
        }
        return Ok(Vec::new());
    }

    let mut snapshots = Vec::new();
    for entry in fs::read_dir(runs_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        snapshots.push(read_snapshot(&path)?);
    }
    snapshots.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(snapshots)
}

pub fn snapshot_path(root: &Path, run_id: &str) -> PathBuf {
    root.join(".forge/runs").join(format!("{run_id}.json"))
}

fn status_step(
    name: &str,
    context: &RunContext,
    current_step: Option<&str>,
) -> StatusStep {
    if let Some(result) = context.step_results.get(name) {
        return status_step_from_result(name, result, context);
    }

    let started_at = context
        .step_started_at
        .get(name)
        .copied()
        .and_then(|value| iso_timestamp(Some(value)).ok());

    let status = if current_step == Some(name) {
        "running"
    } else {
        "pending"
    };

    StatusStep {
        name: name.to_string(),
        status: status.to_string(),
        started_at,
        finished_at: None,
        attempts: 0,
    }
}

fn status_step_from_result(
    name: &str,
    result: &StepResult,
    context: &RunContext,
) -> StatusStep {
    let started_at = context
        .step_started_at
        .get(name)
        .copied()
        .and_then(|value| iso_timestamp(Some(value)).ok());
    let finished_at = matches!(
        result.status,
        StepStatus::Succeeded | StepStatus::Failed | StepStatus::Skipped
    )
        .then(|| iso_timestamp(None))
        .transpose()
        .unwrap_or(None);

    StatusStep {
        name: name.to_string(),
        status: match result.status {
            StepStatus::Pending => "pending",
            StepStatus::Skipped => "skipped",
            StepStatus::Succeeded => "succeeded",
            StepStatus::Failed => "failed",
        }
        .to_string(),
        started_at,
        finished_at,
        attempts: result.attempts,
    }
}

fn iso_timestamp(epoch_secs: Option<u64>) -> Result<String, ForgeError> {
    let mut command = Command::new("date");
    command.arg("-u");
    if let Some(epoch_secs) = epoch_secs {
        command.arg("-d").arg(format!("@{epoch_secs}"));
    }
    command.arg("+%Y-%m-%dT%H:%M:%SZ");
    let output = command.output()?;
    if !output.status.success() {
        return Err(ForgeError::message("failed to determine timestamp"));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
