use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use forge::cli::{Cli, Commands};
use forge::config::{build_run_variables, load_forge_config, resolve_blueprint_for_run};
use forge::dispatch::ProcessRuntime;
use forge::error::ForgeError;
use forge::logger::{JsonlRunLogger, RunEnd, RunLogger, RunMeta};
use forge::model::{Blueprint, RunContext, StepStatus};
use forge::notify::{build_partial_summary, format_run_summary, resolve_backends};
use forge::parser::parse_blueprint_file;
use forge::runner::{BlueprintLoader, Engine};

struct FileBlueprintLoader;

impl BlueprintLoader for FileBlueprintLoader {
    fn load(&self, path: &Path) -> Result<Blueprint, ForgeError> {
        parse_blueprint_file(path)
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), ForgeError> {
    let cli = Cli::parse_args();
    match &cli.command {
        Commands::Run {
            blueprint,
            repo,
            dry_run,
            notify,
            verbose,
            ..
        } => {
            let forge_root = std::env::current_dir()?;
            let config = load_forge_config(forge_root.join("forge.toml"))?;
            let date = today_string()?;
            let blueprint_path = resolve_blueprint_for_run(
                &forge_root.join("blueprints"),
                blueprint,
                repo.as_deref(),
            )?;
            let blueprint = parse_blueprint_file(&blueprint_path)?;

            let mut context = RunContext::new();
            context.variables = build_run_variables(&config, &cli.command, &forge_root, &date)?;
            context.dry_run = *dry_run;
            context.verbose = *verbose;

            let mut logger = JsonlRunLogger::new("runs")?;
            let started_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
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
                blueprint_root: PathBuf::from("blueprints"),
            };

            let run_result = engine.run_blueprint(&blueprint, &mut context);
            let summary = match &run_result {
                Ok(summary) => summary.clone(),
                Err(_) => build_failure_summary(&blueprint, &context),
            };

            let finished_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);

            let _ = engine.logger.log_run_end(&RunEnd {
                entry_type: "run_end",
                success: run_result.is_ok(),
                steps_total: summary.steps.len(),
                steps_passed: summary.steps.iter().filter(|s| s.status == StepStatus::Succeeded).count(),
                steps_failed: summary.steps.iter().filter(|s| s.status == StepStatus::Failed).count(),
                steps_skipped: summary.steps.iter().filter(|s| s.status == StepStatus::Skipped).count(),
                duration_secs: finished_at.saturating_sub(started_at),
                finished_at,
            });

            let notification = format_run_summary(&blueprint.blueprint.name, &summary);

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
    }
}

fn build_failure_summary(
    blueprint: &Blueprint,
    context: &RunContext,
) -> forge::model::RunSummary {
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
