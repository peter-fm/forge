use crate::error::ForgeError;
use crate::run_status::read_snapshot;
use std::path::Path;

pub fn print_status(root: &Path) -> Result<(), ForgeError> {
    let path = root.join(".forge/.run-status.json");
    if !path.exists() {
        println!("no recorded forge run");
        return Ok(());
    }

    let snapshot = read_snapshot(&path)?;
    println!("blueprint: {}", snapshot.blueprint);
    println!("state: {}", snapshot.state);
    if let Some(current_step) = snapshot.current_step {
        println!("current step: {current_step}");
    }
    for step in snapshot.steps {
        println!("{}: {} ({}s)", step.name, step.status, step.duration_secs);
    }
    Ok(())
}
