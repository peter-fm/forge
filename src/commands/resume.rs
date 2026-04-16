use crate::cli::Commands;
use crate::commands::run::{FileBlueprintLoader, finalize_run, load_run_config};
use crate::dashboard::launch_dashboard;
use crate::dispatch::ProcessRuntime;
use crate::error::ForgeError;
use crate::logger::{JsonlRunLogger, RunLogger, RunMeta};
use crate::model::{Blueprint, RunContext};
use crate::parser::parse_blueprint_file;
use crate::run_status::{list_snapshots, read_snapshot, snapshot_path};
use crate::runner::Engine;
use crate::workspace::resolve_instruction_file;
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command as ProcessCommand;

pub fn resume_command(root: &Path, command: &Commands) -> Result<(), ForgeError> {
    let Commands::Resume {
        run_id,
        latest,
        no_dashboard,
        port,
        notify,
        verbose,
    } = command
    else {
        return Err(ForgeError::message("resume command expected"));
    };

    let run_id = resolve_resume_run_id(root, run_id.as_deref(), *latest)?;

    let snapshot_file = snapshot_path(root, &run_id);
    if !snapshot_file.exists() {
        return Err(ForgeError::message(format!("unknown run `{run_id}`")));
    }

    let snapshot = read_snapshot(&snapshot_file)?;
    if snapshot.status == "succeeded" {
        return Err(ForgeError::message(format!(
            "run `{run_id}` already succeeded"
        )));
    }

    let blueprint_path = snapshot.blueprint_path.as_deref().ok_or_else(|| {
        ForgeError::message(format!(
            "run `{run_id}` cannot be resumed because its blueprint path was not recorded"
        ))
    })?;
    let blueprint = parse_blueprint_file(root.join(blueprint_path))?;
    let resume_step = first_unfinished_step(&blueprint, &snapshot.steps).ok_or_else(|| {
        ForgeError::message(format!("run `{run_id}` has no unfinished steps to resume"))
    })?;

    let config = load_run_config(root)?;
    let instruction = snapshot
        .instruction_file
        .as_deref()
        .map(|name| resolve_instruction_file(root, &config, name))
        .transpose()?;

    let mut context = RunContext::new();
    context.run_id = Some(run_id.clone());
    context.status_path = Some(snapshot_file);
    context.instruction_file = snapshot.instruction_file.clone();
    context.run_started_at = parse_timestamp(&snapshot.started_at)?;
    context.resume_from_step = Some(resume_step.to_string());
    context.dry_run = snapshot.dry_run;
    context.verbose = *verbose;
    context.variables = snapshot.variables.clone();
    context
        .variables
        .insert("run_id".to_string(), run_id.clone());
    context.step_results = snapshot
        .step_results
        .into_iter()
        .map(|result| (result.step_id.clone(), result))
        .collect::<BTreeMap<_, _>>();

    let runs_dir = root.join(".forge/runs");
    let mut logger = JsonlRunLogger::new(root, &runs_dir)?;
    let started_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);

    logger.log_run_start(&RunMeta {
        entry_type: "run_start",
        blueprint: blueprint.blueprint.name.clone(),
        repo: snapshot.repo.clone(),
        agent: snapshot.agent.clone(),
        model: snapshot.model.clone(),
        branch: snapshot.branch.clone(),
        dry_run: snapshot.dry_run,
        variables: context.variables.clone(),
        started_at,
    })?;

    let blueprint_root = root.join(".forge/blueprints");
    let mut engine = Engine {
        loader: FileBlueprintLoader,
        runtime: ProcessRuntime,
        logger,
        blueprint_root,
        dashboard: None,
    };
    let mut dashboard = if *no_dashboard || !config.dashboard_enabled() {
        None
    } else {
        Some(launch_dashboard(&blueprint, *port)?)
    };
    engine.dashboard = dashboard.as_ref().map(|server| server.observer.clone());

    let run_result = engine.run_blueprint(&blueprint, &mut context);
    finalize_run(
        root,
        &config,
        instruction.as_ref(),
        notify,
        &blueprint,
        &context,
        &mut engine,
        &mut dashboard,
        run_result,
        started_at,
    )
}

fn first_unfinished_step(
    blueprint: &Blueprint,
    steps: &[crate::run_status::StatusStep],
) -> Option<String> {
    blueprint
        .steps
        .iter()
        .enumerate()
        .find_map(|(index, _step)| {
            let step_id = format!("step-{:04}", index + 1);
            let status = steps.iter().find(|status| status.id == step_id)?;
            (!matches!(status.status.as_str(), "succeeded" | "skipped")).then_some(step_id)
        })
}

fn parse_timestamp(value: &str) -> Result<Option<u64>, ForgeError> {
    let output = ProcessCommand::new("date")
        .args(["-u", "-d", value, "+%s"])
        .output()?;
    if !output.status.success() {
        return Ok(None);
    }
    let epoch = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u64>()
        .map_err(|error| ForgeError::message(error.to_string()))?;
    Ok(Some(epoch))
}

fn resolve_resume_run_id(
    root: &Path,
    run_id: Option<&str>,
    latest: bool,
) -> Result<String, ForgeError> {
    match (run_id, latest) {
        (Some(_), true) => Err(ForgeError::message(
            "use either a run id or --latest, not both",
        )),
        (Some(run_id), false) => Ok(run_id.to_string()),
        (None, false) => Err(ForgeError::message(
            "missing run id: pass a run id or use --latest",
        )),
        (None, true) => {
            let snapshots = list_snapshots(root)?;
            let latest = snapshots
                .first()
                .ok_or_else(|| ForgeError::message("no recorded forge runs"))?;
            Ok(latest.id.clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::resolve_resume_run_id;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn resolve_resume_run_id_supports_latest() {
        let dir = tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join(".forge/runs")).expect("forge runs");
        fs::write(
            dir.path().join(".forge/runs/older.json"),
            serde_json::json!({
                "id": "older",
                "blueprint": "x",
                "status": "failed",
                "started_at": "2026-03-31T13:25:00Z",
                "updated_at": "2026-03-31T13:30:00Z",
                "finished_at": "2026-03-31T13:30:00Z",
                "steps": []
            })
            .to_string(),
        )
        .expect("write older");
        fs::write(
            dir.path().join(".forge/runs/newer.json"),
            serde_json::json!({
                "id": "newer",
                "blueprint": "x",
                "status": "failed",
                "started_at": "2026-03-31T13:35:00Z",
                "updated_at": "2026-03-31T13:40:00Z",
                "finished_at": "2026-03-31T13:40:00Z",
                "steps": []
            })
            .to_string(),
        )
        .expect("write newer");

        let run_id = resolve_resume_run_id(dir.path(), None, true).expect("resolve latest");
        assert_eq!(run_id, "newer");
    }

    #[test]
    fn resolve_resume_run_id_rejects_missing_selector() {
        let dir = tempdir().expect("tempdir");
        let error = resolve_resume_run_id(dir.path(), None, false).expect_err("missing selector");
        assert!(error
            .to_string()
            .contains("missing run id: pass a run id or use --latest"));
    }
}
