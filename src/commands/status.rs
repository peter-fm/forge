use crate::error::ForgeError;
use crate::run_status::{list_snapshots, read_snapshot, snapshot_path};
use std::path::Path;

pub fn print_status(
    root: &Path,
    run_id: Option<&str>,
    include_completed: bool,
) -> Result<(), ForgeError> {
    if let Some(run_id) = run_id {
        let path = snapshot_path(root, run_id);
        if !path.exists() {
            return Err(ForgeError::message(format!("unknown run `{run_id}`")));
        }
        return print_snapshot(&read_snapshot(&path)?);
    }

    let mut snapshots = list_snapshots(root)?;
    if !include_completed {
        snapshots.retain(|snapshot| snapshot.status == "running");
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
