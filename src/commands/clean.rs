use crate::config::{ForgeConfig, load_forge_config_if_exists};
use crate::error::ForgeError;
use crate::workspace::{CleanOptions, clean_workspace};
use std::path::Path;

pub fn clean_command(
    root: &Path,
    archive: bool,
    dry_run: bool,
) -> Result<(), ForgeError> {
    let config = load_config(root)?;
    let report = clean_workspace(
        root,
        &config,
        &CleanOptions {
            archive,
            dry_run,
        },
    )?;

    if archive {
        let verb = if dry_run { "would archive" } else { "archived" };
        if report.archived.is_empty() {
            println!("no instruction files found");
            return Ok(());
        }
        for path in report.archived {
            println!("{verb}: {}", path.display());
        }
    } else if report.listed.is_empty() {
        println!("no instruction files found");
    } else {
        for path in report.listed {
            println!("{}", path.display());
        }
    }

    Ok(())
}

fn load_config(root: &Path) -> Result<ForgeConfig, ForgeError> {
    load_forge_config_if_exists(root.join(".forge/config.toml"))
}
