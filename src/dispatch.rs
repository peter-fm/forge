use crate::error::ForgeError;
use crate::runner::{ExecutionOutput, Runtime};
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;

#[derive(Debug, Default)]
pub struct ProcessRuntime;

impl Runtime for ProcessRuntime {
    fn run_command(
        &mut self,
        _step_name: &str,
        command: &str,
        env: &BTreeMap<String, String>,
    ) -> Result<ExecutionOutput, ForgeError> {
        run_shell(command, env)
    }

    fn run_agent(
        &mut self,
        step_name: &str,
        agent: &str,
        model: &str,
        prompt: &str,
        env: &BTreeMap<String, String>,
    ) -> Result<ExecutionOutput, ForgeError> {
        match agent {
            "claude-code" => run_claude(step_name, model, prompt, env),
            "codex" => run_codex(step_name, model, prompt, env),
            other => Err(ForgeError::message(format!("unsupported agent `{other}`"))),
        }
    }
}

fn run_shell(command: &str, env: &BTreeMap<String, String>) -> Result<ExecutionOutput, ForgeError> {
    let mut process = Command::new("bash");
    let output = apply_env(&mut process, env)
        .arg("-lc")
        .arg(command)
        .output()?;

    Ok(ExecutionOutput {
        exit_code: output.status.code().unwrap_or(1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

fn run_claude(
    step_name: &str,
    model: &str,
    prompt: &str,
    env: &BTreeMap<String, String>,
) -> Result<ExecutionOutput, ForgeError> {
    let repo_path = infer_repo_path(step_name, prompt, env)?;
    let command = format!(
        "cd {} && claude --model {} --permission-mode bypassPermissions --print {}",
        shell_quote(&repo_path),
        shell_quote(model),
        shell_quote(prompt)
    );
    run_shell(&command, env)
}

fn run_codex(
    step_name: &str,
    model: &str,
    prompt: &str,
    env: &BTreeMap<String, String>,
) -> Result<ExecutionOutput, ForgeError> {
    let repo_path = infer_repo_path(step_name, prompt, env)?;
    let session_name = format!("forge-{}", sanitize_session_name(step_name));
    let codex_flags = env
        .get("FORGE_CODEX_FLAGS")
        .cloned()
        .unwrap_or_else(|| "--yolo".to_string());

    // Write prompt to a temp file and launch via a wrapper script to avoid
    // shell quoting issues with complex prompts containing quotes/special chars.
    let prompt_file = std::env::temp_dir().join(format!(
        "forge-prompt-{}.md",
        sanitize_session_name(step_name)
    ));
    std::fs::write(&prompt_file, prompt)
        .map_err(|err| ForgeError::message(format!("failed to write prompt file: {err}")))?;
    let script_file =
        std::env::temp_dir().join(format!("forge-run-{}.sh", sanitize_session_name(step_name)));
    std::fs::write(
        &script_file,
        format!(
            "#!/bin/bash\ncd {}\nexec codex {} --model {} exec \"$(cat {})\"",
            shell_quote(&repo_path),
            codex_flags,
            shell_quote(model),
            shell_quote(&prompt_file.to_string_lossy()),
        ),
    )
    .map_err(|err| ForgeError::message(format!("failed to write script file: {err}")))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script_file, std::fs::Permissions::from_mode(0o755))
            .map_err(|err| ForgeError::message(format!("failed to chmod script: {err}")))?;
    }

    let start = format!(
        "tmux new-session -d -s {} {}",
        shell_quote(&session_name),
        shell_quote(&script_file.to_string_lossy()),
    );
    let start_result = run_shell(&start, env)?;
    if start_result.exit_code != 0 {
        return Ok(start_result);
    }

    loop {
        let exists = run_shell(
            &format!("tmux has-session -t {}", shell_quote(&session_name)),
            env,
        )?;
        if exists.exit_code != 0 {
            break;
        }
        thread::sleep(Duration::from_secs(60));
    }

    let diff = run_shell(
        &format!(
            "cd {} && {{ git diff --stat; printf '\\n'; git diff; }}",
            shell_quote(&repo_path)
        ),
        env,
    )?;
    let _ = run_shell(
        &format!("tmux kill-session -t {}", shell_quote(&session_name)),
        env,
    );
    Ok(diff)
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

fn sanitize_session_name(input: &str) -> String {
    input
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::infer_repo_path;
    use std::collections::BTreeMap;
    use std::fs;
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
}
