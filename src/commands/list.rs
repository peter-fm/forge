use crate::error::ForgeError;
use std::fs;
use std::path::Path;

pub fn list_blueprints(root: &Path) -> Result<(), ForgeError> {
    let blueprint_root = root.join(".forge/blueprints");
    let mut blueprints = Vec::new();
    collect_blueprints(&blueprint_root, &blueprint_root, &mut blueprints)?;
    blueprints.sort();

    for blueprint in blueprints {
        println!("{blueprint}");
    }

    Ok(())
}

fn collect_blueprints(
    root: &Path,
    directory: &Path,
    blueprints: &mut Vec<String>,
) -> Result<(), ForgeError> {
    if !directory.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_blueprints(root, &path, blueprints)?;
            continue;
        }
        if path.extension().and_then(|extension| extension.to_str()) != Some("toml") {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|error| ForgeError::message(error.to_string()))?;
        let relative = relative.with_extension("");
        blueprints.push(relative.to_string_lossy().replace('\\', "/"));
    }

    Ok(())
}
