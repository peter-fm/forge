use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoSpec {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlueprintMeta {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub repos: Vec<RepoSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StepType {
    Deterministic,
    Agentic,
    Conditional,
    Gate,
    Blueprint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step {
    #[serde(rename = "type")]
    pub step_type: StepType,
    pub name: String,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub blueprint: Option<String>,
    #[serde(default)]
    pub params: BTreeMap<String, String>,
    #[serde(default)]
    pub condition: Option<String>,
    #[serde(default)]
    pub sets: Option<String>,
    #[serde(default)]
    pub allow_failure: bool,
    #[serde(default)]
    pub max_retries: Option<u32>,
    #[serde(default)]
    pub expect_failure: bool,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Blueprint {
    pub blueprint: BlueprintMeta,
    #[serde(rename = "step")]
    pub steps: Vec<Step>,
    #[serde(skip)]
    pub source_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum StepStatus {
    Pending,
    Skipped,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StepResult {
    pub name: String,
    pub step_type: StepType,
    pub status: StepStatus,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub attempts: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_file: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunSummary {
    pub steps: Vec<StepResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RunContext {
    pub variables: BTreeMap<String, String>,
    pub step_results: BTreeMap<String, StepResult>,
    pub blueprint_stack: Vec<String>,
    pub status_path: Option<PathBuf>,
    pub run_id: Option<String>,
    pub instruction_file: Option<String>,
    pub run_started_at: Option<u64>,
    pub step_started_at: BTreeMap<String, u64>,
    pub dry_run: bool,
    pub verbose: bool,
}

impl RunContext {
    pub fn new() -> Self {
        Self::default()
    }
}
