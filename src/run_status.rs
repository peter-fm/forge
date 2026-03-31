use crate::error::ForgeError;
use crate::model::{RunContext, StepResult, StepStatus};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunStatusSnapshot {
    pub blueprint: String,
    pub state: String,
    pub current_step: Option<String>,
    pub started_at: u64,
    pub updated_at: u64,
    pub finished_at: Option<u64>,
    pub steps: Vec<StatusStep>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusStep {
    pub name: String,
    pub status: String,
    pub duration_secs: u64,
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

    let now = now_secs()?;
    let started_at = context.run_started_at.unwrap_or(now);
    let mut steps = step_names
        .iter()
        .map(|name| status_step(name, context, current_step, now))
        .collect::<Vec<_>>();

    for (name, result) in &context.step_results {
        if steps.iter().any(|step| step.name == *name) {
            continue;
        }
        steps.push(status_step_from_result(name, result, context, now));
    }

    let snapshot = RunStatusSnapshot {
        blueprint: blueprint.to_string(),
        state: state.to_string(),
        current_step: current_step.map(|step| step.to_string()),
        started_at,
        updated_at: now,
        finished_at: matches!(state, "succeeded" | "failed").then_some(now),
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

fn status_step(
    name: &str,
    context: &RunContext,
    current_step: Option<&str>,
    now: u64,
) -> StatusStep {
    if let Some(result) = context.step_results.get(name) {
        return status_step_from_result(name, result, context, now);
    }

    let status = if current_step == Some(name) {
        "running"
    } else {
        "pending"
    };

    StatusStep {
        name: name.to_string(),
        status: status.to_string(),
        duration_secs: step_duration(name, context, now),
    }
}

fn status_step_from_result(
    name: &str,
    result: &StepResult,
    context: &RunContext,
    now: u64,
) -> StatusStep {
    StatusStep {
        name: name.to_string(),
        status: match result.status {
            StepStatus::Pending => "pending",
            StepStatus::Skipped => "skipped",
            StepStatus::Succeeded => "succeeded",
            StepStatus::Failed => "failed",
        }
        .to_string(),
        duration_secs: step_duration(name, context, now),
    }
}

fn step_duration(name: &str, context: &RunContext, now: u64) -> u64 {
    context
        .step_started_at
        .get(name)
        .map(|started_at| now.saturating_sub(*started_at))
        .unwrap_or(0)
}

fn now_secs() -> Result<u64, ForgeError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|error| ForgeError::message(error.to_string()))
}
