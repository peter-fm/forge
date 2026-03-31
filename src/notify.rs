use std::fmt::Debug;
use std::process::Command;

use crate::error::ForgeError;
use crate::model::{RunSummary, StepResult, StepStatus, StepType};

pub trait NotifyBackend: Debug {
    fn send(&self, summary: &str) -> Result<(), ForgeError>;
}

#[derive(Debug)]
pub struct OpenClawBackend;

impl NotifyBackend for OpenClawBackend {
    fn send(&self, summary: &str) -> Result<(), ForgeError> {
        let status = Command::new("openclaw")
            .args(openclaw_command_args(summary))
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(ForgeError::message(format!(
                "openclaw notification failed with status {status}"
            )))
        }
    }
}

pub fn resolve_backends(names: &[String]) -> Result<Vec<Box<dyn NotifyBackend>>, ForgeError> {
    names
        .iter()
        .map(|name| match name.as_str() {
            "openclaw" => Ok(Box::new(OpenClawBackend) as Box<dyn NotifyBackend>),
            _ => Err(ForgeError::message(format!(
                "unknown notify backend `{name}`"
            ))),
        })
        .collect()
}

pub fn openclaw_command_args(summary: &str) -> Vec<&str> {
    vec!["system", "event", "--text", summary, "--mode", "now"]
}

pub fn format_run_summary(blueprint_name: &str, summary: &RunSummary) -> String {
    let passed = summary
        .steps
        .iter()
        .filter(|step| step.status == StepStatus::Succeeded)
        .count();
    let total = summary.steps.len();

    if let Some(failed_step) = summary
        .steps
        .iter()
        .find(|step| step.status == StepStatus::Failed)
    {
        return format!(
            "❌ {blueprint_name} failed at step '{}': {passed}/{total} steps completed",
            failed_step.name
        );
    }

    format!("✅ {blueprint_name} completed: {passed}/{total} steps passed")
}

pub fn build_partial_summary(step_names: &[String], recorded_steps: &[StepResult]) -> RunSummary {
    let mut steps = Vec::with_capacity(step_names.len());

    for name in step_names {
        if let Some(result) = recorded_steps.iter().find(|result| result.name == *name) {
            steps.push(result.clone());
        } else {
            steps.push(StepResult {
                name: name.clone(),
                step_type: StepType::Deterministic,
                status: StepStatus::Pending,
                exit_code: 0,
                stdout: String::new(),
                stderr: String::new(),
                attempts: 0,
            });
        }
    }

    RunSummary { steps }
}
