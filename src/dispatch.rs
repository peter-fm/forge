use crate::error::ForgeError;
use crate::runner::{ExecutionOutput, Runtime};
use nix::pty::{Winsize, openpty};
use nix::unistd::{close, dup2};
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Debug, Default)]
pub struct ProcessRuntime;

impl Runtime for ProcessRuntime {
    fn run_command(
        &mut self,
        _step_name: &str,
        command: &str,
        env: &BTreeMap<String, String>,
        log_path: Option<&Path>,
    ) -> Result<ExecutionOutput, ForgeError> {
        run_shell(command, env, log_path)
    }

    fn run_agent(
        &mut self,
        step_name: &str,
        agent: &str,
        model: &str,
        prompt: &str,
        env: &BTreeMap<String, String>,
        log_path: Option<&Path>,
    ) -> Result<ExecutionOutput, ForgeError> {
        match agent {
            "claude-code" => run_claude(step_name, model, prompt, env, log_path),
            "codex" => run_codex(step_name, model, prompt, env, log_path),
            other => Err(ForgeError::message(format!("unsupported agent `{other}`"))),
        }
    }
}

fn run_shell(
    command: &str,
    env: &BTreeMap<String, String>,
    log_path: Option<&Path>,
) -> Result<ExecutionOutput, ForgeError> {
    let pty = openpty(
        Some(&Winsize {
            ws_row: 24,
            ws_col: 80,
            ws_xpixel: 0,
            ws_ypixel: 0,
        }),
        None,
    )
    .map_err(io::Error::from)?;
    let master_fd = pty.master.as_raw_fd();
    let slave_fd = pty.slave.as_raw_fd();
    let stdin_file = File::open("/dev/null")?;
    let stdin_fd = stdin_file.as_raw_fd();
    let mut process = Command::new("bash");
    let child = apply_env(&mut process, env);
    if !env.contains_key("TERM") {
        child.env("TERM", "xterm-256color");
    }
    unsafe {
        child.pre_exec(move || {
            dup2(stdin_fd, 0).map_err(io::Error::from)?;
            dup2(slave_fd, 1).map_err(io::Error::from)?;
            dup2(slave_fd, 2).map_err(io::Error::from)?;
            close(master_fd).map_err(io::Error::from)?;
            if slave_fd > 2 {
                close(slave_fd).map_err(io::Error::from)?;
            }
            if stdin_fd > 2 {
                close(stdin_fd).map_err(io::Error::from)?;
            }
            Ok(())
        });
    }
    let mut child = child
        .arg("-lc")
        .arg(command)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    drop(pty.slave);
    drop(stdin_file);
    let sink = match log_path {
        Some(path) => Some(Arc::new(Mutex::new(
            OpenOptions::new().create(true).append(true).open(path)?,
        ))),
        None => None,
    };
    let output_handle = spawn_reader(File::from(pty.master), sink);
    let status = child.wait()?;
    let stdout = output_handle
        .join()
        .map_err(|_| ForgeError::message("failed to join process output reader"))?
        .map_err(ForgeError::from)?;
    if let Some(path) = log_path {
        if let Ok(raw) = std::fs::read(path) {
            let normalized = String::from_utf8_lossy(&raw).replace("\r\n", "\n");
            std::fs::write(path, normalized)?;
        }
    }
    Ok(ExecutionOutput {
        exit_code: status.code().unwrap_or(1),
        // PTYs expose a single terminal stream, so stdout and stderr are merged.
        stdout: String::from_utf8_lossy(&stdout).replace("\r\n", "\n"),
        stderr: String::new(),
    })
}

fn run_claude(
    step_name: &str,
    model: &str,
    prompt: &str,
    env: &BTreeMap<String, String>,
    log_path: Option<&Path>,
) -> Result<ExecutionOutput, ForgeError> {
    let repo_path = infer_repo_path(step_name, prompt, env)?;
    let command = format!(
        "cd {} && claude --model {} --permission-mode bypassPermissions --print {}",
        shell_quote(&repo_path),
        shell_quote(model),
        shell_quote(prompt)
    );
    run_shell(&command, env, log_path)
}

fn run_codex(
    step_name: &str,
    model: &str,
    prompt: &str,
    env: &BTreeMap<String, String>,
    log_path: Option<&Path>,
) -> Result<ExecutionOutput, ForgeError> {
    let repo_path = infer_repo_path(step_name, prompt, env)?;
    let command = build_codex_command(step_name, model, prompt, env)?;
    let _ = run_shell(&command, env, log_path)?;

    let diff = run_shell(
        &format!(
            "cd {} && {{ git diff --stat; printf '\\n'; git diff; }}",
            shell_quote(&repo_path)
        ),
        env,
        None,
    )?;
    Ok(diff)
}

fn build_codex_command(
    step_name: &str,
    model: &str,
    prompt: &str,
    env: &BTreeMap<String, String>,
) -> Result<String, ForgeError> {
    let repo_path = infer_repo_path(step_name, prompt, env)?;
    // Top-level flags go BEFORE the `exec` subcommand (e.g. `--yolo`).
    let top_flags = env
        .get("FORGE_CODEX_FLAGS")
        .cloned()
        .unwrap_or_else(|| "--yolo".to_string());
    // Exec-level flags go AFTER `exec`. `--json` MUST live here — codex rejects
    // `--json` at the top level. Streaming JSONL events is why we're here:
    // spawn_reader flushes chunks to the step log as they arrive, making
    // long-running agentic phases observable instead of silent.
    let exec_flags = env
        .get("FORGE_CODEX_EXEC_FLAGS")
        .cloned()
        .unwrap_or_else(|| "--json".to_string());

    Ok(format!(
        "cd {} && codex {} --model {} exec {} {}",
        shell_quote(&repo_path),
        top_flags,
        shell_quote(model),
        exec_flags,
        shell_quote(prompt)
    ))
}

fn spawn_reader<R>(
    mut reader: R,
    sink: Option<Arc<Mutex<std::fs::File>>>,
) -> thread::JoinHandle<io::Result<Vec<u8>>>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut chunk = [0_u8; 4096];

        loop {
            let read = match reader.read(&mut chunk) {
                Ok(read) => read,
                // PTY masters report EIO after the slave closes; treat that as EOF.
                Err(err) if err.raw_os_error() == Some(5) => break,
                Err(err) => return Err(err),
            };
            if read == 0 {
                break;
            }

            bytes.extend_from_slice(&chunk[..read]);
            if let Some(sink) = &sink {
                let mut sink = sink
                    .lock()
                    .map_err(|_| io::Error::other("failed to lock step log"))?;
                sink.write_all(&chunk[..read])?;
                sink.flush()?;
            }
        }

        Ok(bytes)
    })
}

fn infer_repo_path(
    step_name: &str,
    prompt: &str,
    env: &BTreeMap<String, String>,
) -> Result<String, ForgeError> {
    if let Some(path) = env.get("FORGE_TARGET_REPO_PATH") {
        return Ok(path.clone());
    }

    for (env_key, path) in env {
        if !env_key.starts_with("FORGE_") || !env_key.ends_with("_PATH") {
            continue;
        }
        if env_key == "FORGE_TARGET_REPO_PATH" {
            continue;
        }
        let Some(repo_path) = repo_root_for_path(path, env.get("PWD").map(String::as_str)) else {
            continue;
        };

        if prompt.contains(path) {
            return Ok(repo_path);
        }

        if matches_repo_hint(step_name, prompt, env_key) {
            return Ok(repo_path);
        }
    }

    env.get("PWD")
        .cloned()
        .ok_or_else(|| ForgeError::message("unable to infer working directory for agent step"))
}

fn repo_root_for_path(path: &str, pwd: Option<&str>) -> Option<String> {
    let path = resolve_path(path, pwd);
    let search_start = if path.is_dir() {
        path.as_path()
    } else {
        path.parent()?
    };

    for candidate in search_start.ancestors() {
        if candidate
            .file_name()
            .is_some_and(|name| name == OsStr::new(".forge"))
        {
            return candidate
                .parent()
                .map(|parent| parent.display().to_string());
        }

        if candidate.join(".forge").is_dir() || candidate.join(".git").exists() {
            return Some(candidate.display().to_string());
        }
    }

    path.is_dir().then(|| path.display().to_string())
}

fn resolve_path(path: &str, pwd: Option<&str>) -> std::path::PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
        path.to_path_buf()
    } else if let Some(pwd) = pwd {
        Path::new(pwd).join(path)
    } else {
        path.to_path_buf()
    }
}

fn matches_repo_hint(step_name: &str, prompt: &str, env_key: &str) -> bool {
    let hint = env_key
        .trim_start_matches("FORGE_")
        .trim_end_matches("_PATH")
        .to_ascii_lowercase();
    let hyphenated = hint.replace('_', "-");
    if step_name.contains(&hyphenated) || prompt.contains(&hyphenated) {
        return true;
    }

    hint.split('_').any(|fragment| {
        !fragment.is_empty()
            && fragment != "target"
            && fragment != "repo"
            && (step_name.contains(fragment) || prompt.contains(fragment))
    })
}

fn apply_env<'a>(command: &'a mut Command, env: &BTreeMap<String, String>) -> &'a mut Command {
    for (key, value) in env {
        command.env(key, value);
    }
    command
}

fn shell_quote(input: &str) -> String {
    format!("'{}'", input.replace('\'', "'\"'\"'"))
}

#[cfg(test)]
mod tests {
    use super::{build_codex_command, infer_repo_path, run_shell};
    use std::collections::BTreeMap;
    use std::fs;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread;
    use std::time::Duration;
    use tempfile::tempdir;

    #[test]
    fn infer_repo_path_ignores_instruction_file_paths() {
        let repo = tempdir().expect("tempdir");
        fs::create_dir_all(repo.path().join(".forge/instructions")).expect("instructions dir");
        let instruction_path = repo.path().join(".forge/instructions/current.md");
        fs::write(&instruction_path, "implement the task").expect("instruction file");

        let env = BTreeMap::from([
            (
                "FORGE_INSTRUCTION_PATH".to_string(),
                instruction_path.display().to_string(),
            ),
            ("PWD".to_string(), repo.path().display().to_string()),
        ]);

        let repo_path = infer_repo_path(
            "implement",
            &format!(
                "Read {} and implement the task.",
                instruction_path.display()
            ),
            &env,
        )
        .expect("repo path");

        assert_eq!(repo_path, repo.path().display().to_string());
    }

    #[test]
    fn infer_repo_path_prefers_explicit_target_repo_path() {
        let env = BTreeMap::from([
            (
                "FORGE_TARGET_REPO_PATH".to_string(),
                "/tmp/worktree".to_string(),
            ),
            ("PWD".to_string(), "/repo".to_string()),
        ]);

        let repo_path = infer_repo_path("implement", "Read .forge/instructions/current.md.", &env)
            .expect("repo path");

        assert_eq!(repo_path, "/tmp/worktree");
    }

    #[test]
    fn codex_command_defaults_to_json_streaming() {
        // With no env overrides, --json must be at the EXEC level (after `exec`),
        // not at the top level. codex's argument parser rejects top-level --json.
        let env = BTreeMap::from([("PWD".to_string(), "/repo".to_string())]);

        let command =
            build_codex_command("implement", "gpt-5.4", "fix it", &env).expect("codex command");

        assert!(
            command.contains("codex --yolo --model 'gpt-5.4' exec --json 'fix it'"),
            "unexpected command: {command}"
        );
    }

    #[test]
    fn codex_command_respects_top_level_opt_out() {
        // Setting FORGE_CODEX_FLAGS overrides top-level flags (e.g. drop --yolo).
        // --json at exec level is unaffected — still streaming by default.
        let env = BTreeMap::from([
            (
                "FORGE_CODEX_FLAGS".to_string(),
                "--ask-for-approval=never".to_string(),
            ),
            ("PWD".to_string(), "/repo".to_string()),
        ]);

        let command =
            build_codex_command("implement", "gpt-5.4", "fix it", &env).expect("codex command");

        assert!(
            command
                .contains("codex --ask-for-approval=never --model 'gpt-5.4' exec --json 'fix it'"),
            "unexpected command: {command}"
        );
    }

    #[test]
    fn codex_command_respects_exec_opt_out() {
        // Setting FORGE_CODEX_EXEC_FLAGS="" drops --json, restoring the old
        // buffered behaviour for users who want it.
        let env = BTreeMap::from([
            ("FORGE_CODEX_EXEC_FLAGS".to_string(), String::new()),
            ("PWD".to_string(), "/repo".to_string()),
        ]);

        let command =
            build_codex_command("implement", "gpt-5.4", "fix it", &env).expect("codex command");

        assert!(!command.contains("--json"), "unexpected command: {command}");
        assert!(
            command.contains("codex --yolo --model 'gpt-5.4' exec  'fix it'"),
            "unexpected command: {command}"
        );
    }

    #[test]
    fn run_shell_streams_output_to_log_while_running() {
        let dir = tempdir().expect("tempdir");
        let log_path = dir.path().join("step.log");
        let done = Arc::new(AtomicBool::new(false));
        let saw_growth = Arc::new(AtomicBool::new(false));
        let poll_done = Arc::clone(&done);
        let poll_growth = Arc::clone(&saw_growth);
        let poll_path = log_path.clone();
        let poller = thread::spawn(move || {
            let mut last_len = 0;
            while !poll_done.load(Ordering::Relaxed) {
                let len = fs::metadata(&poll_path).map(|meta| meta.len()).unwrap_or(0);
                if len > last_len {
                    poll_growth.store(true, Ordering::Relaxed);
                    last_len = len;
                }
                thread::sleep(Duration::from_millis(50));
            }
        });

        let output = run_shell(
            "for i in 1 2 3; do echo line$i; sleep 0.2; done",
            &BTreeMap::new(),
            Some(&log_path),
        )
        .expect("run shell");

        done.store(true, Ordering::Relaxed);
        poller.join().expect("poller");

        assert!(saw_growth.load(Ordering::Relaxed), "log never grew mid-run");
        assert_eq!(output.exit_code, 0);
        assert!(output.stdout.contains("line1"), "missing line1: {}", output.stdout);
        assert!(output.stdout.contains("line2"), "missing line2: {}", output.stdout);
        assert!(output.stdout.contains("line3"), "missing line3: {}", output.stdout);
        assert!(output.stderr.is_empty(), "stderr should be empty for PTY output");
    }
}
