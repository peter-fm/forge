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
    // Disable TTY-interactive behaviour in every descendant process.
    // The PTY we provide so the agent can render its TUI is inherited
    // by grandchildren (git, cargo, ssh, apt). Tools that auto-detect
    // a terminal will switch to interactive mode — e.g. `git diff` pipes
    // into `less`, which then blocks forever waiting for user input.
    // These env vars force pagers to `cat` and fail fast on prompts.
    // Applied AFTER the caller's env so they always win, even if the
    // caller set a conflicting value by mistake.
    for (key, value) in hardened_env_for_agent() {
        child.env(key, value);
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
    // `-c`, not `-lc`: a login shell sources ~/.profile / ~/.bash_profile,
    // which typically *prepend* system/user bin dirs (mise, bun, cargo) to PATH.
    // That silently reorders any PATH the caller set — breaking tests that
    // stage a mock `claude`/`codex` in a tempdir bin/ and prepend it. The
    // inherited PATH already has the user's shell setup when forge is run
    // from a terminal, so `-l` bought us nothing here.
    let mut child = child
        .arg("-c")
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
    let command = build_claude_command(step_name, model, prompt, env)?;
    run_shell(&command, env, log_path)
}

fn build_claude_command(
    step_name: &str,
    model: &str,
    prompt: &str,
    env: &BTreeMap<String, String>,
) -> Result<String, ForgeError> {
    let repo_path = infer_repo_path(step_name, prompt, env)?;
    let resume = env.get("FORGE_AGENT_RESUME_ID");
    let session = env.get("FORGE_AGENT_SESSION_ID");
    if let Some(session_id) = resume {
        Ok(format!(
            "cd {} && claude --model {} --permission-mode bypassPermissions --print --resume {} {}",
            shell_quote(&repo_path),
            shell_quote(model),
            shell_quote(session_id),
            shell_quote(prompt)
        ))
    } else if let Some(session_id) = session {
        Ok(format!(
            "cd {} && claude --model {} --permission-mode bypassPermissions --print --session-id {} {}",
            shell_quote(&repo_path),
            shell_quote(model),
            shell_quote(session_id),
            shell_quote(prompt)
        ))
    } else {
        Ok(format!(
            "cd {} && claude --model {} --permission-mode bypassPermissions --print {}",
            shell_quote(&repo_path),
            shell_quote(model),
            shell_quote(prompt)
        ))
    }
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
    let execution = run_shell(&command, env, log_path)?;
    if !should_collect_codex_diff(&execution) {
        return Ok(execution);
    }

    let diff = run_shell(
        &format!(
            "cd {} && {{ git diff --stat; printf '\\n'; git diff; }}",
            shell_quote(&repo_path)
        ),
        env,
        None,
    )?;
    Ok(select_codex_output(execution, diff))
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
    // Effort is agent-neutral and set via `[agent].effort` in forge config
    // (surfaced as FORGE_AGENT_EFFORT). For codex it maps to
    // `-c model_reasoning_effort=...`. Defaults to medium; empty disables the flag.
    let effort = env
        .get("FORGE_AGENT_EFFORT")
        .map(String::as_str)
        .unwrap_or("medium");
    let reasoning_flag = if effort.is_empty() {
        String::new()
    } else {
        format!(" -c model_reasoning_effort={}", shell_quote(effort))
    };
    // Exec-level flags go AFTER `exec`. `--json` MUST live here — codex rejects
    // `--json` at the top level. Streaming JSONL events is why we're here:
    // spawn_reader flushes chunks to the step log as they arrive, making
    // long-running agentic phases observable instead of silent.
    let exec_flags = env
        .get("FORGE_CODEX_EXEC_FLAGS")
        .cloned()
        .unwrap_or_else(|| "--json".to_string());
    let resume = env.get("FORGE_AGENT_RESUME_ID");

    if let Some(session_id) = resume {
        Ok(format!(
            "cd {} && codex {}{} --model {} exec resume {} {} {}",
            shell_quote(&repo_path),
            top_flags,
            reasoning_flag,
            shell_quote(model),
            exec_flags,
            shell_quote(session_id),
            shell_quote(prompt)
        ))
    } else {
        Ok(format!(
            "cd {} && codex {}{} --model {} exec {} {}",
            shell_quote(&repo_path),
            top_flags,
            reasoning_flag,
            shell_quote(model),
            exec_flags,
            shell_quote(prompt)
        ))
    }
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

/// Env overrides that disable TTY-interactive behaviour in descendant processes.
///
/// See the caller in `run_shell` for the full rationale. In short: the agent
/// child runs under a PTY so its TUI renders correctly, and every grandchild
/// (git, cargo, ssh, apt) inherits that PTY. These overrides force pagers to
/// `cat` and make credential / debconf prompts fail fast instead of blocking
/// forever on empty stdin.
const fn hardened_env_for_agent() -> &'static [(&'static str, &'static str)] {
    &[
        // Kills git's pager. `git diff` with long output pipes into `less`,
        // which sits on empty stdin forever — this is the one that bit us.
        ("GIT_PAGER", "cat"),
        // General pager fallback (man, systemctl, etc).
        ("PAGER", "cat"),
        // Explicit man pager override — belt and braces.
        ("MANPAGER", "cat"),
        // Makes git credential prompts fail fast instead of blocking.
        ("GIT_TERMINAL_PROMPT", "0"),
        // Silences debconf / apt interactive prompts.
        ("DEBIAN_FRONTEND", "noninteractive"),
    ]
}

fn shell_quote(input: &str) -> String {
    format!("'{}'", input.replace('\'', "'\"'\"'"))
}

fn select_codex_output(execution: ExecutionOutput, diff: ExecutionOutput) -> ExecutionOutput {
    debug_assert_eq!(execution.exit_code, 0);
    diff
}

fn should_collect_codex_diff(execution: &ExecutionOutput) -> bool {
    execution.exit_code == 0
}

#[cfg(test)]
mod tests {
    use super::{
        build_claude_command, build_codex_command, infer_repo_path, run_shell, select_codex_output,
        should_collect_codex_diff,
    };
    use crate::runner::ExecutionOutput;
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
            build_codex_command("implement", "gpt-5.5", "fix it", &env).expect("codex command");

        assert!(
            command.contains(
                "codex --yolo -c model_reasoning_effort='medium' --model 'gpt-5.5' exec --json 'fix it'"
            ),
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
            build_codex_command("implement", "gpt-5.5", "fix it", &env).expect("codex command");

        assert!(
            command.contains(
                "codex --ask-for-approval=never -c model_reasoning_effort='medium' --model 'gpt-5.5' exec --json 'fix it'"
            ),
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
            build_codex_command("implement", "gpt-5.5", "fix it", &env).expect("codex command");

        assert!(!command.contains("--json"), "unexpected command: {command}");
        assert!(
            command.contains(
                "codex --yolo -c model_reasoning_effort='medium' --model 'gpt-5.5' exec  'fix it'"
            ),
            "unexpected command: {command}"
        );
    }

    #[test]
    fn codex_command_honors_effort_override() {
        let env = BTreeMap::from([
            ("FORGE_AGENT_EFFORT".to_string(), "xhigh".to_string()),
            ("PWD".to_string(), "/repo".to_string()),
        ]);

        let command =
            build_codex_command("implement", "gpt-5.5", "fix it", &env).expect("codex command");

        assert!(
            command.contains("-c model_reasoning_effort='xhigh'"),
            "unexpected command: {command}"
        );
        assert!(
            !command.contains("model_reasoning_effort='medium'"),
            "unexpected command: {command}"
        );
    }

    #[test]
    fn codex_command_drops_reasoning_flag_when_effort_is_empty() {
        let env = BTreeMap::from([
            ("FORGE_AGENT_EFFORT".to_string(), String::new()),
            ("PWD".to_string(), "/repo".to_string()),
        ]);

        let command =
            build_codex_command("implement", "gpt-5.5", "fix it", &env).expect("codex command");

        assert!(
            !command.contains("model_reasoning_effort"),
            "unexpected command: {command}"
        );
    }

    #[test]
    fn codex_resume_command_uses_exec_resume_subcommand() {
        let env = BTreeMap::from([
            (
                "FORGE_AGENT_RESUME_ID".to_string(),
                "019d7824-b932-77d1-bc55-59db268b05b0".to_string(),
            ),
            ("PWD".to_string(), "/repo".to_string()),
        ]);

        let command =
            build_codex_command("implement", "gpt-5.5", "fix it", &env).expect("codex command");

        assert!(
            command.contains(
                "codex --yolo -c model_reasoning_effort='medium' --model 'gpt-5.5' exec resume --json '019d7824-b932-77d1-bc55-59db268b05b0' 'fix it'"
            ),
            "unexpected command: {command}"
        );
    }

    #[test]
    fn claude_command_prefers_resume_id_over_session_id() {
        let env = BTreeMap::from([
            (
                "FORGE_AGENT_RESUME_ID".to_string(),
                "31e8f4e8-ef7d-4966-afc5-0e8f0f548af0".to_string(),
            ),
            (
                "FORGE_AGENT_SESSION_ID".to_string(),
                "b230d7f8-bfa8-4af5-a664-7e17e53f5b35".to_string(),
            ),
            ("PWD".to_string(), "/repo".to_string()),
        ]);

        let command =
            build_claude_command("implement", "sonnet", "fix it", &env).expect("claude command");

        assert!(command.contains("--resume '31e8f4e8-ef7d-4966-afc5-0e8f0f548af0'"));
        assert!(!command.contains("--session-id"));
    }

    #[test]
    fn claude_command_uses_session_id_for_initial_run() {
        let env = BTreeMap::from([
            (
                "FORGE_AGENT_SESSION_ID".to_string(),
                "b230d7f8-bfa8-4af5-a664-7e17e53f5b35".to_string(),
            ),
            ("PWD".to_string(), "/repo".to_string()),
        ]);

        let command =
            build_claude_command("implement", "sonnet", "fix it", &env).expect("claude command");

        assert!(command.contains("--session-id 'b230d7f8-bfa8-4af5-a664-7e17e53f5b35'"));
    }

    #[test]
    fn failed_codex_run_keeps_agent_exit_code_and_output() {
        let execution = ExecutionOutput {
            exit_code: 1,
            stdout: "{\"type\":\"turn.failed\"}\n".to_string(),
            stderr: String::new(),
        };

        assert!(!should_collect_codex_diff(&execution));
    }

    #[test]
    fn successful_codex_run_returns_diff_output() {
        let execution = ExecutionOutput {
            exit_code: 0,
            stdout: "{\"type\":\"turn.completed\"}\n".to_string(),
            stderr: String::new(),
        };
        let diff = ExecutionOutput {
            exit_code: 0,
            stdout: "diff --git a/file b/file\n".to_string(),
            stderr: String::new(),
        };

        let result = select_codex_output(execution, diff);

        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("diff --git"));
    }

    #[test]
    fn run_shell_applies_hardened_env_to_children() {
        // The spawn site in run_shell injects a hardened env that disables
        // TTY-interactive behaviour in every descendant. Verify each override
        // actually reaches the child — regression guard against someone
        // dropping the call or the caller env accidentally shadowing it.
        let output = run_shell(
            "printf 'GIT_PAGER=%s\\nPAGER=%s\\nMANPAGER=%s\\nGIT_TERMINAL_PROMPT=%s\\nDEBIAN_FRONTEND=%s\\n' \
             \"$GIT_PAGER\" \"$PAGER\" \"$MANPAGER\" \"$GIT_TERMINAL_PROMPT\" \"$DEBIAN_FRONTEND\"",
            &BTreeMap::new(),
            None,
        )
        .expect("run shell");

        assert_eq!(output.exit_code, 0, "stdout: {}", output.stdout);
        assert!(
            output.stdout.contains("GIT_PAGER=cat"),
            "missing GIT_PAGER=cat: {}",
            output.stdout
        );
        assert!(
            output.stdout.contains("PAGER=cat"),
            "missing PAGER=cat: {}",
            output.stdout
        );
        assert!(
            output.stdout.contains("MANPAGER=cat"),
            "missing MANPAGER=cat: {}",
            output.stdout
        );
        assert!(
            output.stdout.contains("GIT_TERMINAL_PROMPT=0"),
            "missing GIT_TERMINAL_PROMPT=0: {}",
            output.stdout
        );
        assert!(
            output.stdout.contains("DEBIAN_FRONTEND=noninteractive"),
            "missing DEBIAN_FRONTEND=noninteractive: {}",
            output.stdout
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
        assert!(
            output.stdout.contains("line1"),
            "missing line1: {}",
            output.stdout
        );
        assert!(
            output.stdout.contains("line2"),
            "missing line2: {}",
            output.stdout
        );
        assert!(
            output.stdout.contains("line3"),
            "missing line3: {}",
            output.stdout
        );
        assert!(
            output.stderr.is_empty(),
            "stderr should be empty for PTY output"
        );
    }
}
