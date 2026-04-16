use crate::error::ForgeError;
use crate::run_status::{list_snapshots, read_snapshot, snapshot_path};
use std::path::Path;

pub fn print_status(
    root: &Path,
    run_id: Option<&str>,
    include_completed: bool,
    latest_only: bool,
    limit: Option<usize>,
) -> Result<(), ForgeError> {
    if run_id.is_some() && (include_completed || latest_only || limit.is_some()) {
        return Err(ForgeError::message(
            "when providing a run id, do not combine it with --all, --latest, or --limit",
        ));
    }

    if let Some(run_id) = run_id {
        let path = snapshot_path(root, run_id);
        if !path.exists() {
            return Err(ForgeError::message(format!("unknown run `{run_id}`")));
        }
        return print_snapshot(&read_snapshot(&path)?);
    }

    let include_completed = include_completed || latest_only || limit.is_some();
    let mut snapshots = list_snapshots(root)?;
    if !include_completed {
        snapshots.retain(|snapshot| snapshot.status == "running");
    }
    let effective_limit = if latest_only { Some(1) } else { limit };
    if let Some(limit) = effective_limit {
        snapshots.truncate(limit);
    }
    if snapshots.is_empty() {
        println!("no recorded forge runs");
        return Ok(());
    }

    for (index, snapshot) in snapshots.iter().enumerate() {
        if index > 0 {
            println!();
        }
        print_snapshot(snapshot)?;
    }

    Ok(())
}

fn print_snapshot(snapshot: &crate::run_status::RunStatusSnapshot) -> Result<(), ForgeError> {
    if !snapshot.id.is_empty() {
        println!("run id: {}", snapshot.id);
    }
    println!("blueprint: {}", snapshot.blueprint);
    println!("status: {}", snapshot.status);
    if let Some(instruction_file) = &snapshot.instruction_file {
        println!("instruction file: {instruction_file}");
    }
    if let Some(agent) = &snapshot.agent {
        println!("agent: {agent}");
    }
    if let Some(current_step) = &snapshot.current_step {
        println!("current step: {current_step}");
    }
    for step in &snapshot.steps {
        println!(
            "{}: {} (attempts: {})",
            step.name, step.status, step.attempts
        );
    }
    Ok(())
}
