use crate::config::ForgeConfig;
use crate::error::ForgeError;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstructionFile {
    pub file_name: String,
    pub path: PathBuf,
    pub path_display: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanOptions {
    pub archive: bool,
    pub dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CleanReport {
    pub listed: Vec<PathBuf>,
    pub archived: Vec<PathBuf>,
}

pub fn ensure_workspace_layout(root: &Path, config: &ForgeConfig) -> Result<(), ForgeError> {
    fs::create_dir_all(instructions_dir(root, config))?;
    fs::create_dir_all(archive_dir(root, config))?;

    let gitkeep = instructions_dir(root, config).join(".gitkeep");
    if !gitkeep.exists() {
        fs::write(gitkeep, "")?;
    }

    let gitignore = root.join(".forge/.gitignore");
    let content = format!(
        "{instructions}/*\n!{instructions}/.gitkeep\n{archive}/\nruns/\n",
        instructions = config.workspace_instructions_dir(),
        archive = config.workspace_archive_dir()
    );
    fs::write(gitignore, content)?;

    Ok(())
}

pub fn create_instruction_file(
    root: &Path,
    config: &ForgeConfig,
    task: &str,
    agent: &str,
) -> Result<InstructionFile, ForgeError> {
    fs::create_dir_all(instructions_dir(root, config))?;
    let timestamp = local_timestamp("%Y-%m-%dT%H%M")?;
    let file_name = next_instruction_file_name(root, config, task, &timestamp, agent)?;
    let path = instructions_dir(root, config).join(&file_name);
    fs::write(&path, task)?;
    let path_display = instruction_path_display(config, &file_name);
    Ok(InstructionFile {
        file_name,
        path,
        path_display,
    })
}

pub fn build_instruction_file_name(task: &str, timestamp: &str, agent: &str) -> String {
    format!(
        "{}.{}.{}.md",
        slugify_instruction(task, 50),
        timestamp,
        sanitize_agent(agent)
    )
}

pub fn resolve_instruction_file(
    root: &Path,
    config: &ForgeConfig,
    input: &str,
) -> Result<InstructionFile, ForgeError> {
    let candidate = PathBuf::from(input);
    let resolved = if candidate.is_absolute() {
        candidate
    } else {
        let in_workspace = instructions_dir(root, config).join(&candidate);
        if in_workspace.exists() {
            in_workspace
        } else {
            root.join(&candidate)
        }
    };

    if !resolved.exists() {
        return Err(ForgeError::message(format!(
            "instruction file `{}` does not exist",
            resolved.display()
        )));
    }

    let file_name = resolved
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| ForgeError::message("invalid instruction file name"))?
        .to_string();
    let path_display = instruction_path_display_for_path(root, &resolved);

    Ok(InstructionFile {
        file_name,
        path: resolved,
        path_display,
    })
}

pub fn archive_instruction_file(
    root: &Path,
    config: &ForgeConfig,
    instruction: &InstructionFile,
    suffix_kind: &str,
) -> Result<Option<PathBuf>, ForgeError> {
    let source = &instruction.path;
    if !source.exists() {
        return Ok(None);
    }

    fs::create_dir_all(archive_dir(root, config))?;
    let archived_name = archive_name(
        &instruction.file_name,
        suffix_kind,
        &local_timestamp("%H%M")?,
    );
    let destination = archive_dir(root, config).join(archived_name);
    fs::rename(source, &destination)?;
    Ok(Some(destination))
}

pub fn list_instruction_files(
    root: &Path,
    config: &ForgeConfig,
) -> Result<Vec<InstructionFile>, ForgeError> {
    let mut files = Vec::new();
    for entry in fs::read_dir(instructions_dir(root, config))? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if file_name == ".gitkeep" {
            continue;
        }
        let file_name = file_name.to_string();
        let path_display = instruction_path_display(config, &file_name);
        files.push(InstructionFile {
            file_name,
            path,
            path_display,
        });
    }
    files.sort_by(|left, right| left.file_name.cmp(&right.file_name));
    Ok(files)
}

pub fn clean_workspace(
    root: &Path,
    config: &ForgeConfig,
    options: &CleanOptions,
) -> Result<CleanReport, ForgeError> {
    fs::create_dir_all(instructions_dir(root, config))?;
    fs::create_dir_all(archive_dir(root, config))?;

    let files = list_instruction_files(root, config)?;
    let mut report = CleanReport {
        listed: files.iter().map(|file| file.path.clone()).collect(),
        archived: Vec::new(),
    };

    if options.archive {
        for file in files {
            let destination = archive_dir(root, config).join(archive_name(
                &file.file_name,
                "archived",
                &local_timestamp("%H%M")?,
            ));
            if !options.dry_run {
                fs::rename(&file.path, &destination)?;
            }
            report.archived.push(destination);
        }
    }

    Ok(report)
}

pub fn stale_root_instruction_files(root: &Path) -> Result<Vec<PathBuf>, ForgeError> {
    let mut stale = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if matches_stale_instruction_name(name) {
            stale.push(path);
        }
    }
    stale.sort();
    Ok(stale)
}

pub fn instructions_dir(root: &Path, config: &ForgeConfig) -> PathBuf {
    root.join(".forge")
        .join(config.workspace_instructions_dir())
}

pub fn archive_dir(root: &Path, config: &ForgeConfig) -> PathBuf {
    root.join(".forge").join(config.workspace_archive_dir())
}

fn matches_stale_instruction_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    let prefixes = ["CODEX_BRIEF", "CODEX_INSTRUCTIONS"];
    prefixes.iter().any(|prefix| upper.starts_with(prefix)) && upper.ends_with(".MD")
}

fn archive_name(file_name: &str, suffix_kind: &str, hhmm: &str) -> String {
    match file_name.strip_suffix(".md") {
        Some(stem) => format!("{stem}.{suffix_kind}-{hhmm}.md"),
        None => format!("{file_name}.{suffix_kind}-{hhmm}"),
    }
}

fn sanitize_agent(agent: &str) -> String {
    let mut output = String::new();
    let mut last_dash = false;
    for ch in agent.chars() {
        if ch.is_ascii_alphanumeric() {
            output.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash && !output.is_empty() {
            output.push('-');
            last_dash = true;
        }
    }
    while output.ends_with('-') {
        output.pop();
    }
    if output.is_empty() {
        "agent".to_string()
    } else {
        output
    }
}

fn slugify_instruction(task: &str, max_len: usize) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for ch in task.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash && !slug.is_empty() {
            slug.push('-');
            last_dash = true;
        }
        if slug.len() >= max_len {
            break;
        }
    }

    slug.truncate(max_len);
    while slug.ends_with('-') {
        slug.pop();
    }

    if slug.is_empty() {
        "work".to_string()
    } else {
        slug
    }
}

fn local_timestamp(format: &str) -> Result<String, ForgeError> {
    let output = Command::new("date").arg(format!("+{format}")).output()?;
    if !output.status.success() {
        return Err(ForgeError::message("failed to determine current timestamp"));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn next_instruction_file_name(
    root: &Path,
    config: &ForgeConfig,
    task: &str,
    timestamp: &str,
    agent: &str,
) -> Result<String, ForgeError> {
    let base = build_instruction_file_name(task, timestamp, agent);
    let base_path = instructions_dir(root, config).join(&base);
    if !base_path.exists() {
        return Ok(base);
    }

    let stem = base
        .strip_suffix(".md")
        .ok_or_else(|| ForgeError::message("instruction file must end in .md"))?;
    for index in 2.. {
        let candidate = format!("{stem}-{index}.md");
        if !instructions_dir(root, config).join(&candidate).exists() {
            return Ok(candidate);
        }
    }

    Err(ForgeError::message(
        "unable to allocate instruction file name",
    ))
}

fn instruction_path_display(config: &ForgeConfig, file_name: &str) -> String {
    format!(
        ".forge/{}/{}",
        config.workspace_instructions_dir(),
        file_name
    )
}

fn instruction_path_display_for_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map(|relative| relative.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}
