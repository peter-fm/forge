use crate::detect::{DetectedProject, ProjectType, detect_project};
use crate::error::ForgeError;
use std::fs;
use std::path::Path;

const GENERATED_HEADER: &str = "# forge-generated\n";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitOptions {
    pub project_type: Option<ProjectType>,
    pub force: bool,
}

pub fn init_project(root: &Path, options: &InitOptions) -> Result<DetectedProject, ForgeError> {
    let detected = detect_project(root, options.project_type)?;
    write_generated_files(root, &detected, options.force)?;
    ensure_instructions_gitignore(root)?;
    ensure_agents_md(root)?;
    Ok(detected)
}

pub fn write_generated_files(
    root: &Path,
    detected: &DetectedProject,
    force: bool,
) -> Result<(), ForgeError> {
    fs::create_dir_all(root.join(".forge/blueprints"))?;
    fs::create_dir_all(root.join(".forge/instructions"))?;

    write_generated_file(
        &root.join(".forge/config.toml"),
        &render_config(detected),
        force,
    )?;
    write_generated_file(
        &root.join(".forge/blueprints/new-feature.toml"),
        &render_new_feature_blueprint(detected),
        force,
    )?;
    write_generated_file(
        &root.join(".forge/blueprints/fix-bug.toml"),
        &render_fix_bug_blueprint(detected),
        force,
    )?;
    write_generated_file(
        &root.join(".forge/blueprints/refactor.toml"),
        &render_refactor_blueprint(detected),
        force,
    )?;

    let gitkeep = root.join(".forge/instructions/.gitkeep");
    if !gitkeep.exists() {
        fs::write(gitkeep, "")?;
    }

    Ok(())
}

pub fn render_config(detected: &DetectedProject) -> String {
    let mut output = String::from(GENERATED_HEADER);
    output.push_str("[project]\n");
    output.push_str(&format!("type = \"{}\"\n", detected.project_type.as_str()));
    output.push_str(&format!("name = \"{}\"\n\n", escape_toml(&detected.name)));
    output.push_str("[commands]\n");
    if let Some(command) = &detected.commands.test {
        output.push_str(&format!("test = \"{}\"\n", escape_toml(command)));
    }
    if let Some(command) = &detected.commands.lint {
        output.push_str(&format!("lint = \"{}\"\n", escape_toml(command)));
    }
    if let Some(command) = &detected.commands.build {
        output.push_str(&format!("build = \"{}\"\n", escape_toml(command)));
    }
    output.push_str("\n[agent]\n");
    output.push_str("default = \"codex\"\n");
    output.push_str("model = \"gpt-5.4\"\n\n");
    output.push_str("[instructions]\n");
    output.push_str("directory = \"instructions\"\n");
    output.push_str("gitignore = true\n");
    if detected.agents_md_present {
        output.push_str("agents_md = \"AGENTS.md\"\n");
    }
    output
}

pub fn render_new_feature_blueprint(detected: &DetectedProject) -> String {
    let mut output = String::from(GENERATED_HEADER);
    output.push_str("[blueprint]\n");
    output.push_str("name = \"new-feature\"\n");
    output.push_str("description = \"Implement a new feature with lint and test gates\"\n\n");
    output.push_str("[[step]]\n");
    output.push_str("type = \"agentic\"\n");
    output.push_str("name = \"implement\"\n");
    output.push_str("agent = \"{target_agent}\"\n");
    output.push_str("model = \"{target_model}\"\n");
    output.push_str("prompt = \"\"\"Read the task instructions in .forge/instructions/. Implement the feature described there. Make sure to add tests for new functionality. Commit your changes.\"\"\"\n");
    output.push_str("max_retries = 2\n\n");
    if let Some(command) = &detected.commands.lint {
        output.push_str("[[step]]\n");
        output.push_str("type = \"deterministic\"\n");
        output.push_str("name = \"lint\"\n");
        output.push_str(&format!("command = \"{}\"\n\n", escape_toml(command)));
    }
    if let Some(command) = &detected.commands.test {
        output.push_str("[[step]]\n");
        output.push_str("type = \"deterministic\"\n");
        output.push_str("name = \"test\"\n");
        output.push_str(&format!("command = \"{}\"\n", escape_toml(command)));
    }
    output
}

pub fn render_fix_bug_blueprint(detected: &DetectedProject) -> String {
    let mut output = String::from(GENERATED_HEADER);
    output.push_str("[blueprint]\n");
    output.push_str("name = \"fix-bug\"\n");
    output.push_str("description = \"Fix a bug with test verification\"\n\n");
    output.push_str("[[step]]\n");
    output.push_str("type = \"agentic\"\n");
    output.push_str("name = \"fix\"\n");
    output.push_str("agent = \"{target_agent}\"\n");
    output.push_str("model = \"{target_model}\"\n");
    output.push_str("prompt = \"\"\"Read the task instructions in .forge/instructions/. Fix the bug described there. Add a regression test that would have caught this bug. Commit your changes.\"\"\"\n");
    output.push_str("max_retries = 3\n\n");
    if let Some(command) = &detected.commands.test {
        output.push_str("[[step]]\n");
        output.push_str("type = \"deterministic\"\n");
        output.push_str("name = \"test\"\n");
        output.push_str(&format!("command = \"{}\"\n", escape_toml(command)));
    }
    output
}

pub fn render_refactor_blueprint(detected: &DetectedProject) -> String {
    let mut output = String::from(GENERATED_HEADER);
    output.push_str("[blueprint]\n");
    output.push_str("name = \"refactor\"\n");
    output.push_str("description = \"Refactor code with verification gates\"\n\n");
    output.push_str("[[step]]\n");
    output.push_str("type = \"agentic\"\n");
    output.push_str("name = \"refactor\"\n");
    output.push_str("agent = \"{target_agent}\"\n");
    output.push_str("model = \"{target_model}\"\n");
    output.push_str("prompt = \"\"\"Read the task instructions in .forge/instructions/. Refactor the code described there without changing intended behavior. Commit your changes once verification passes.\"\"\"\n");
    output.push('\n');
    if let Some(command) = &detected.commands.lint {
        output.push_str("[[step]]\n");
        output.push_str("type = \"deterministic\"\n");
        output.push_str("name = \"lint\"\n");
        output.push_str(&format!("command = \"{}\"\n\n", escape_toml(command)));
    }
    if let Some(command) = &detected.commands.test {
        output.push_str("[[step]]\n");
        output.push_str("type = \"deterministic\"\n");
        output.push_str("name = \"test\"\n");
        output.push_str(&format!("command = \"{}\"\n\n", escape_toml(command)));
    }
    if let Some(command) = &detected.commands.build {
        output.push_str("[[step]]\n");
        output.push_str("type = \"deterministic\"\n");
        output.push_str("name = \"build\"\n");
        output.push_str(&format!("command = \"{}\"\n", escape_toml(command)));
    }
    output
}

pub fn ensure_instructions_gitignore(root: &Path) -> Result<(), ForgeError> {
    let path = root.join(".gitignore");
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let mut updated = existing.clone();

    if !updated.contains(".forge/instructions/*") {
        if !updated.ends_with('\n') && !updated.is_empty() {
            updated.push('\n');
        }
        updated.push_str(".forge/instructions/*\n");
    }
    if !updated.contains("!.forge/instructions/.gitkeep") {
        updated.push_str("!.forge/instructions/.gitkeep\n");
    }

    if updated != existing {
        fs::write(path, updated)?;
    }

    Ok(())
}

pub fn ensure_agents_md(root: &Path) -> Result<(), ForgeError> {
    let path = root.join("AGENTS.md");
    if path.exists() {
        return Ok(());
    }
    let content = "# AGENTS\n\nThis repository uses `.forge/` for generated blueprints and task instructions.\n";
    fs::write(path, content)?;
    Ok(())
}

fn write_generated_file(path: &Path, content: &str, force: bool) -> Result<(), ForgeError> {
    if path.exists() {
        let existing = fs::read_to_string(path)?;
        let generated = existing.starts_with(GENERATED_HEADER);
        if !force && !generated {
            return Err(ForgeError::message(format!(
                "refusing to overwrite manually edited file `{}` without --force",
                path.display()
            )));
        }
    }
    fs::write(path, content)?;
    Ok(())
}

fn escape_toml(input: &str) -> String {
    input.replace('\\', "\\\\").replace('"', "\\\"")
}
