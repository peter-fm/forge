use crate::error::ForgeError;
use crate::model::StepResult;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io;
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
        let path = create_unique_run_log(base_dir, timestamp)?;
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

fn create_unique_run_log(base_dir: &Path, timestamp: u64) -> Result<PathBuf, ForgeError> {
    let mut attempt = 0;
    loop {
        let file_name = if attempt == 0 {
            format!("run-{timestamp}.jsonl")
        } else {
            format!("run-{timestamp}-{attempt}.jsonl")
        };
        let path = base_dir.join(file_name);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(_) => return Ok(path),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                attempt += 1;
            }
            Err(error) => return Err(error.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::create_unique_run_log;
    use tempfile::tempdir;

    #[test]
    fn creates_distinct_jsonl_files_for_same_timestamp() {
        let dir = tempdir().expect("tempdir");

        let first = create_unique_run_log(dir.path(), 123).expect("first logger path");
        let second = create_unique_run_log(dir.path(), 123).expect("second logger path");

        assert_ne!(first, second);
        assert_eq!(
            first.file_name().and_then(|value| value.to_str()),
            Some("run-123.jsonl")
        );
        assert_eq!(
            second.file_name().and_then(|value| value.to_str()),
            Some("run-123-1.jsonl")
        );
    }
}
