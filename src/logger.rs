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

    fn create_step_log(&mut self, _step_name: &str) -> Result<Option<StepLog>, ForgeError> {
        Ok(None)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepLog {
    pub path: PathBuf,
    pub relative_path: String,
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
    run_dir: PathBuf,
    started_at: u64,
    next_step_index: usize,
    root_dir: PathBuf,
}

impl JsonlRunLogger {
    pub fn new(root_dir: impl AsRef<Path>, base_dir: impl AsRef<Path>) -> Result<Self, ForgeError> {
        let root_dir = root_dir.as_ref().to_path_buf();
        let base_dir = base_dir.as_ref();
        fs::create_dir_all(base_dir)?;
        let timestamp = now_secs()?;
        let (run_dir, path) = create_unique_run_log(base_dir, timestamp)?;
        Ok(Self {
            path,
            run_dir,
            started_at: timestamp,
            next_step_index: 1,
            root_dir,
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

    fn create_step_log(&mut self, step_name: &str) -> Result<Option<StepLog>, ForgeError> {
        let file_name = format!(
            "step-{}-{}.log",
            self.next_step_index,
            sanitize_step_name(step_name)
        );
        self.next_step_index += 1;
        let path = self.run_dir.join(file_name);
        OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&path)?;
        let relative_path = self
            .relative_path(&path)
            .unwrap_or_else(|| path.display().to_string());
        Ok(Some(StepLog {
            path,
            relative_path,
        }))
    }
}

impl JsonlRunLogger {
    fn relative_path(&self, path: &Path) -> Option<String> {
        path.strip_prefix(&self.root_dir)
            .ok()
            .map(|relative| relative.display().to_string())
    }
}

fn now_secs() -> Result<u64, ForgeError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|error| ForgeError::message(error.to_string()))
}

fn create_unique_run_log(base_dir: &Path, timestamp: u64) -> Result<(PathBuf, PathBuf), ForgeError> {
    let mut attempt = 0;
    loop {
        let run_name = if attempt == 0 {
            format!("run-{timestamp}")
        } else {
            format!("run-{timestamp}-{attempt}")
        };
        let run_dir = base_dir.join(&run_name);
        match fs::create_dir(&run_dir) {
            Ok(_) => {
                let path = run_dir.join("run.jsonl");
                OpenOptions::new().write(true).create_new(true).open(&path)?;
                return Ok((run_dir, path));
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                attempt += 1;
            }
            Err(error) => return Err(error.into()),
        }
    }
}

fn sanitize_step_name(input: &str) -> String {
    let mut output = String::new();
    let mut last_dash = false;

    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            output.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash && !output.is_empty() {
            output.push('-');
            last_dash = true;
        }
    }

    while output.ends_with('-') {
        output.pop();
    }

    if output.is_empty() {
        "step".to_string()
    } else {
        output
    }
}

#[cfg(test)]
mod tests {
    use super::{create_unique_run_log, sanitize_step_name, JsonlRunLogger, RunLogger};
    use tempfile::tempdir;

    #[test]
    fn creates_distinct_jsonl_files_for_same_timestamp() {
        let dir = tempdir().expect("tempdir");

        let (first_dir, first) = create_unique_run_log(dir.path(), 123).expect("first logger path");
        let (second_dir, second) = create_unique_run_log(dir.path(), 123).expect("second logger path");

        assert_ne!(first, second);
        assert_eq!(
            first_dir.file_name().and_then(|value| value.to_str()),
            Some("run-123")
        );
        assert_eq!(
            second_dir.file_name().and_then(|value| value.to_str()),
            Some("run-123-1")
        );
        assert_eq!(first.file_name().and_then(|value| value.to_str()), Some("run.jsonl"));
        assert_eq!(second.file_name().and_then(|value| value.to_str()), Some("run.jsonl"));
    }

    #[test]
    fn creates_sanitized_step_log_files_in_run_directory() {
        let dir = tempdir().expect("tempdir");
        let runs_dir = dir.path().join(".forge/runs");
        let mut logger = JsonlRunLogger::new(dir.path(), &runs_dir).expect("logger");

        let step_log = logger
            .create_step_log("Cargo Test / All")
            .expect("step log")
            .expect("step log metadata");

        assert!(step_log.path.exists());
        assert_eq!(
            step_log.path.file_name().and_then(|value| value.to_str()),
            Some("step-1-cargo-test-all.log")
        );
        assert!(step_log.relative_path.starts_with(".forge/runs/run-"));
    }

    #[test]
    fn sanitizes_empty_step_names() {
        assert_eq!(sanitize_step_name("***"), "step");
    }
}
