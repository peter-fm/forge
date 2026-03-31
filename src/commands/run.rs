use crate::cli::Commands;
use crate::config::{
    build_run_variables, load_forge_config, load_forge_config_if_exists, resolve_blueprint_for_run,
};
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
    let instruction = resolve_run_instruction(root, &config, task.as_deref(), instruction.as_deref(), command)?;

    let mut context = RunContext::new();
    context.variables = build_run_variables(&config, command, root, &date)?;
    context.dry_run = *dry_run;
    context.verbose = *verbose;
    context.instruction_file = instruction.as_ref().map(|file| file.file_name.clone());
    if let Some(instruction) = &instruction {
        context
            .variables
            .insert("instruction_file".to_string(), instruction.file_name.clone());
        context.variables.insert(
            "instruction_path".to_string(),
            instruction.path_display.clone(),
        );
    }

    let runs_dir = root.join(".forge/runs");
    let mut logger = JsonlRunLogger::new(&runs_dir)?;
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
    };

    let run_result = engine.run_blueprint(&blueprint, &mut context);
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

fn resolve_target_agent<'a>(config: &'a crate::config::ForgeConfig, command: &'a Commands) -> &'a str {
    match command {
        Commands::Run { agent: Some(agent), .. } => agent,
        _ => config.defaults.agent.as_deref().unwrap_or("codex"),
    }
}

fn resolve_run_instruction(
    root: &Path,
    config: &crate::config::ForgeConfig,
    task: Option<&str>,
    instruction: Option<&str>,
    command: &Commands,
) -> Result<Option<InstructionFile>, ForgeError> {
    match (task, instruction) {
        (Some(_), Some(_)) => Err(ForgeError::message(
            "use either --task or --instruction, not both",
        )),
        (Some(task), None) => Ok(Some(create_instruction_file(
            root,
            config,
            task,
            resolve_target_agent(config, command),
        )?)),
        (None, Some(instruction)) => Ok(Some(resolve_instruction_file(root, config, instruction)?)),
        (None, None) => Ok(None),
    }
}
