use crate::error::ForgeError;
use crate::model::StepResult;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub trait RunLogger {
    fn log_run_start(&mut self, meta: &RunMeta) -> Result<(), ForgeError>;
    fn log_step(&mut self, step: &StepResult) -> Result<(), ForgeError>;
    fn log_run_end(&mut self, result: &RunEnd) -> Result<(), ForgeError>;
}

#[derive(Debug, Clone, Serialize)]
pub struct RunMeta {
    #[serde(rename = "type")]
    pub entry_type: &'static str,
    pub blueprint: String,
    pub repo: Option<String>,
    pub agent: Option<String>,
    pub model: Option<String>,
    pub branch: Option<String>,
    pub dry_run: bool,
    pub variables: BTreeMap<String, String>,
    pub started_at: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunEnd {
    #[serde(rename = "type")]
    pub entry_type: &'static str,
    pub success: bool,
    pub steps_total: usize,
    pub steps_passed: usize,
    pub steps_failed: usize,
    pub steps_skipped: usize,
    pub duration_secs: u64,
    pub finished_at: u64,
}

#[derive(Debug)]
pub struct JsonlRunLogger {
    path: PathBuf,
    started_at: u64,
}

impl JsonlRunLogger {
    pub fn new(base_dir: impl AsRef<Path>) -> Result<Self, ForgeError> {
        let base_dir = base_dir.as_ref();
        fs::create_dir_all(base_dir)?;
        let timestamp = now_secs()?;
        let path = base_dir.join(format!("run-{timestamp}.jsonl"));
        File::create(&path)?;
        Ok(Self {
            path,
            started_at: timestamp,
        })
    }

    fn append<T: Serialize>(&self, entry: &T) -> Result<(), ForgeError> {
        let mut file = OpenOptions::new().append(true).open(&self.path)?;
        serde_json::to_writer(&mut file, entry)?;
        writeln!(file)?;
        Ok(())
    }
}

impl RunLogger for JsonlRunLogger {
    fn log_run_start(&mut self, meta: &RunMeta) -> Result<(), ForgeError> {
        self.started_at = meta.started_at;
        self.append(meta)
    }

    fn log_step(&mut self, step: &StepResult) -> Result<(), ForgeError> {
        self.append(step)
    }

    fn log_run_end(&mut self, result: &RunEnd) -> Result<(), ForgeError> {
        self.append(result)
    }
}

fn now_secs() -> Result<u64, ForgeError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|error| ForgeError::message(error.to_string()))
}
