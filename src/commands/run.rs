use crate::cli::Commands;
use crate::config::{
    build_run_variables, load_forge_config, load_forge_config_if_exists, resolve_blueprint_for_run,
};
use crate::dashboard::launch_dashboard;
use crate::dispatch::ProcessRuntime;
use crate::error::ForgeError;
use crate::logger::{JsonlRunLogger, RunEnd, RunLogger, RunMeta};
use crate::model::{Blueprint, RunContext, StepStatus};
use crate::notify::{build_partial_summary, format_run_summary, resolve_backends};
use crate::parser::parse_blueprint_file;
use crate::runner::{BlueprintLoader, Engine};
use crate::workspace::{
    InstructionFile, archive_instruction_file, create_instruction_file, resolve_instruction_file,
};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

struct FileBlueprintLoader;

impl BlueprintLoader for FileBlueprintLoader {
    fn load(&self, path: &Path) -> Result<Blueprint, ForgeError> {
        parse_blueprint_file(path)
    }
}

pub fn run_command(root: &Path, command: &Commands) -> Result<(), ForgeError> {
    let Commands::Run {
        blueprint_name,
        blueprint,
        repo,
        task,
        instruction,
        dry_run,
        no_dashboard,
        port,
        notify,
        verbose,
        ..
    } = command
    else {
        return Err(ForgeError::message("run command expected"));
    };

    let config = load_run_config(root)?;
    let date = today_string()?;
    let blueprint_root = root.join(".forge/blueprints");
    let blueprint_path =
        resolve_run_blueprint_path(&blueprint_root, blueprint_name, blueprint, repo)?;
    let blueprint = parse_blueprint_file(&blueprint_path)?;
    let mut variables = build_run_variables(&config, command, root, &date)?;
    variables.insert("default_branch".to_string(), resolve_default_branch(root));
    if let Some(pr) = resolve_pr_variable(root, command, &blueprint)? {
        variables.insert("pr".to_string(), pr);
    }
    let instruction_slug =
        resolve_instruction_file_slug(command, variables.get("branch").map(String::as_str));
    let instruction = resolve_run_instruction(
        root,
        &config,
        task.as_deref(),
        instruction.as_deref(),
        command,
        instruction_slug.as_deref(),
    )?;

    let mut context = RunContext::new();
    context.variables = std::mem::take(&mut variables);
    context.dry_run = *dry_run;
    context.verbose = *verbose;
    context.instruction_file = instruction.as_ref().map(|file| file.file_name.clone());
    if let Some(instruction) = &instruction {
        context.variables.insert(
            "instruction_file".to_string(),
            instruction.file_name.clone(),
        );
        context.variables.insert(
            "instruction_path".to_string(),
            instruction.path_display.clone(),
        );
    }

    let runs_dir = root.join(".forge/runs");
    let mut logger = JsonlRunLogger::new(root, &runs_dir)?;
    let started_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);

    logger.log_run_start(&RunMeta {
        entry_type: "run_start",
        blueprint: blueprint.blueprint.name.clone(),
        repo: repo.clone(),
        agent: context.variables.get("target_agent").cloned(),
        model: context.variables.get("target_model").cloned(),
        branch: context.variables.get("branch").cloned(),
        dry_run: *dry_run,
        variables: context.variables.clone(),
        started_at,
    })?;

    let mut engine = Engine {
        loader: FileBlueprintLoader,
        runtime: ProcessRuntime,
        logger,
        blueprint_root,
        dashboard: None,
    };
    let mut dashboard = if *no_dashboard {
        None
    } else {
        Some(launch_dashboard(&blueprint, *port)?)
    };
    engine.dashboard = dashboard.as_ref().map(|server| server.observer.clone());

    let run_result = engine.run_blueprint(&blueprint, &mut context);
    if let Some(server) = &dashboard {
        server.observer.complete_run(if run_result.is_ok() {
            "success"
        } else {
            "failure"
        });
        eprintln!(
            "Dashboard available for 60s at http://localhost:{}",
            server.port
        );
    }
    let summary = match &run_result {
        Ok(summary) => summary.clone(),
        Err(_) => build_failure_summary(&blueprint, &context),
    };

    let finished_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);

    let _ = engine.logger.log_run_end(&RunEnd {
        entry_type: "run_end",
        success: run_result.is_ok(),
        steps_total: summary.steps.len(),
        steps_passed: summary
            .steps
            .iter()
            .filter(|step| step.status == StepStatus::Succeeded)
            .count(),
        steps_failed: summary
            .steps
            .iter()
            .filter(|step| step.status == StepStatus::Failed)
            .count(),
        steps_skipped: summary
            .steps
            .iter()
            .filter(|step| step.status == StepStatus::Skipped)
            .count(),
        duration_secs: finished_at.saturating_sub(started_at),
        finished_at,
    });

    let notification = format_run_summary(&blueprint.blueprint.name, &summary);

    match &run_result {
        Ok(_) if config.workspace_auto_archive() => {
            if let Some(instruction) = &instruction {
                archive_instruction_file(root, &config, instruction, "done")?;
            }
        }
        _ => {}
    }

    if !notify.is_empty() {
        let backends = resolve_backends(notify)?;
        for backend in backends {
            if let Err(error) = backend.send(&notification) {
                eprintln!("warning: failed to send notification: {error}");
            }
        }
    }

    if let Ok(summary) = &run_result {
        for step in &summary.steps {
            println!(
                "[{:?}] {} -> {:?} ({})",
                step.step_type, step.name, step.status, step.exit_code
            );
            if *verbose {
                if !step.stdout.is_empty() {
                    println!("{}", step.stdout);
                }
                if !step.stderr.is_empty() {
                    eprintln!("{}", step.stderr);
                }
            }
        }
    }

    if let Some(server) = dashboard.take() {
        server.wait()?;
    }

    run_result.map(|_| ())
}

fn load_run_config(root: &Path) -> Result<crate::config::ForgeConfig, ForgeError> {
    let dot_forge = root.join(".forge/config.toml");
    if dot_forge.exists() {
        return load_forge_config(&dot_forge);
    }
    let legacy = root.join("forge.toml");
    if legacy.exists() {
        return load_forge_config(&legacy);
    }
    load_forge_config_if_exists(dot_forge)
}

fn resolve_run_blueprint_path(
    blueprint_root: &Path,
    blueprint_name: &Option<String>,
    blueprint: &Option<String>,
    repo: &Option<String>,
) -> Result<PathBuf, ForgeError> {
    if let Some(path) = blueprint {
        return Ok(PathBuf::from(path));
    }

    let blueprint_name = blueprint_name
        .as_deref()
        .ok_or_else(|| ForgeError::message("missing blueprint name"))?;
    resolve_blueprint_for_run(blueprint_root, blueprint_name, repo.as_deref())
}
fn build_failure_summary(blueprint: &Blueprint, context: &RunContext) -> crate::model::RunSummary {
    let step_names = blueprint
        .steps
        .iter()
        .map(|step| step.name.clone())
        .collect::<Vec<_>>();
    let recorded_steps = context.step_results.values().cloned().collect::<Vec<_>>();
    build_partial_summary(&step_names, &recorded_steps)
}

fn today_string() -> Result<String, ForgeError> {
    let output = ProcessCommand::new("date").arg("+%F").output()?;
    if !output.status.success() {
        return Err(ForgeError::message("failed to determine current date"));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn resolve_target_agent<'a>(
    config: &'a crate::config::ForgeConfig,
    command: &'a Commands,
) -> &'a str {
    match command {
        Commands::Run {
            agent: Some(agent), ..
        } => agent,
        _ => config.defaults.agent.as_deref().unwrap_or("codex"),
    }
}

fn resolve_run_instruction(
    root: &Path,
    config: &crate::config::ForgeConfig,
    task: Option<&str>,
    instruction: Option<&str>,
    command: &Commands,
    instruction_slug: Option<&str>,
) -> Result<Option<InstructionFile>, ForgeError> {
    match (task, instruction) {
        (Some(_), Some(_)) => Err(ForgeError::message(
            "use either --task or --instruction, not both",
        )),
        (Some(task), None) => Ok(Some(create_instruction_file(
            root,
            config,
            task,
            instruction_slug.unwrap_or("work"),
            resolve_target_agent(config, command),
        )?)),
        (None, Some(instruction)) => Ok(Some(resolve_instruction_file(root, config, instruction)?)),
        (None, None) => Ok(None),
    }
}

fn resolve_instruction_file_slug(
    command: &Commands,
    resolved_branch: Option<&str>,
) -> Option<String> {
    let Commands::Run { task, .. } = command else {
        return None;
    };
    if task.is_none() {
        return None;
    }

    resolved_branch.map(|branch| {
        branch
            .rsplit('/')
            .next()
            .filter(|slug| !slug.is_empty())
            .unwrap_or("work")
            .to_string()
    })
}

fn resolve_default_branch(root: &Path) -> String {
    if let Some(remote_head) = git_output(
        root,
        &[
            "symbolic-ref",
            "--quiet",
            "--short",
            "refs/remotes/origin/HEAD",
        ],
    ) {
        if let Some(branch) = parse_remote_head(&remote_head) {
            return branch;
        }
    }

    if let Some(current) = git_output(root, &["branch", "--show-current"]) {
        let current = current.trim();
        if !current.is_empty() {
            return current.to_string();
        }
    }

    if git_success(
        root,
        &["show-ref", "--verify", "--quiet", "refs/heads/main"],
    ) {
        return "main".to_string();
    }
    if git_success(
        root,
        &["show-ref", "--verify", "--quiet", "refs/heads/master"],
    ) {
        return "master".to_string();
    }

    "main".to_string()
}

fn parse_remote_head(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    Some(
        trimmed
            .strip_prefix("origin/")
            .unwrap_or(trimmed)
            .to_string(),
    )
}

fn git_output(root: &Path, args: &[&str]) -> Option<String> {
    let output = ProcessCommand::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).to_string())
}

fn git_success(root: &Path, args: &[&str]) -> bool {
    ProcessCommand::new("git")
        .args(args)
        .current_dir(root)
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PullRequestSelection {
    Next,
    Latest,
}

#[derive(Debug, Deserialize)]
struct PullRequestNumber {
    number: u64,
}

fn resolve_pr_variable(
    root: &Path,
    command: &Commands,
    blueprint: &Blueprint,
) -> Result<Option<String>, ForgeError> {
    let Commands::Run {
        pr, next, latest, ..
    } = command
    else {
        return Ok(None);
    };

    let selector_count = usize::from(pr.is_some()) + usize::from(*next) + usize::from(*latest);
    if selector_count > 1 {
        return Err(ForgeError::message(
            "use only one of --pr, --next, or --latest",
        ));
    }

    if *next || *latest {
        if blueprint.blueprint.name != "pr-review" {
            return Err(ForgeError::message(
                "--next and --latest are only supported when running the `pr-review` blueprint",
            ));
        }

        let selection = if *next {
            PullRequestSelection::Next
        } else {
            PullRequestSelection::Latest
        };
        let selected = resolve_open_pull_request_number(root, selection)?;
        return Ok(Some(selected.to_string()));
    }

    if blueprint.blueprint.name == "pr-review" && pr.is_none() {
        return Err(ForgeError::message(
            "missing PR target: use --pr <number>, --next, or --latest",
        ));
    }

    Ok(pr.clone())
}

fn resolve_open_pull_request_number(
    root: &Path,
    selection: PullRequestSelection,
) -> Result<u64, ForgeError> {
    let output = ProcessCommand::new("gh")
        .arg("pr")
        .arg("list")
        .arg("--state")
        .arg("open")
        .arg("--limit")
        .arg("1000")
        .arg("--json")
        .arg("number")
        .current_dir(root)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let detail = if stderr.is_empty() {
            "unknown error".to_string()
        } else {
            stderr
        };
        return Err(ForgeError::message(format!(
            "failed to query open pull requests via `gh pr list`: {detail}"
        )));
    }

    let numbers = parse_pull_request_numbers(&output.stdout)?;
    select_pull_request_number(&numbers, selection).ok_or_else(|| {
        ForgeError::message("no open pull requests found for --next/--latest selection")
    })
}

fn parse_pull_request_numbers(bytes: &[u8]) -> Result<Vec<u64>, ForgeError> {
    let prs: Vec<PullRequestNumber> = serde_json::from_slice(bytes)?;
    Ok(prs.into_iter().map(|pr| pr.number).collect())
}

fn select_pull_request_number(numbers: &[u64], selection: PullRequestSelection) -> Option<u64> {
    match selection {
        PullRequestSelection::Next => numbers.iter().min().copied(),
        PullRequestSelection::Latest => numbers.iter().max().copied(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        PullRequestSelection, parse_pull_request_numbers, parse_remote_head,
        select_pull_request_number,
    };

    #[test]
    fn parse_pull_request_numbers_extracts_numbers() {
        let numbers = parse_pull_request_numbers(br#"[{"number":4},{"number":5},{"number":6}]"#)
            .expect("json should parse");
        assert_eq!(numbers, vec![4, 5, 6]);
    }

    #[test]
    fn select_pull_request_number_supports_next_and_latest() {
        let numbers = vec![6, 4, 5];
        assert_eq!(
            select_pull_request_number(&numbers, PullRequestSelection::Next),
            Some(4)
        );
        assert_eq!(
            select_pull_request_number(&numbers, PullRequestSelection::Latest),
            Some(6)
        );
    }

    #[test]
    fn parse_remote_head_strips_origin_prefix() {
        assert_eq!(
            parse_remote_head("origin/master\n").as_deref(),
            Some("master")
        );
        assert_eq!(parse_remote_head("main").as_deref(), Some("main"));
        assert_eq!(parse_remote_head(" \n"), None);
    }
}
