use crate::detect::{DetectedProject, ProjectType, detect_project};
use crate::error::ForgeError;
use crate::workspace::{ensure_workspace_layout, stale_root_instruction_files};
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
    handle_stale_instruction_files(root)?;
    Ok(detected)
}

pub fn write_generated_files(
    root: &Path,
    detected: &DetectedProject,
    force: bool,
) -> Result<(), ForgeError> {
    fs::create_dir_all(root.join(".forge/blueprints"))?;

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
    write_generated_file(
        &root.join(".forge/blueprints/pr-review.toml"),
        &render_pr_review_blueprint(detected),
        force,
    )?;

    let config = crate::config::load_forge_config_str(&render_config(detected))?;
    ensure_workspace_layout(root, &config)?;

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
    output.push_str("\n[workspace]\n");
    output.push_str("instructions = \"instructions\"\n");
    output.push_str("archive = \"archive\"\n");
    output.push_str("auto_archive = true\n");
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
    output.push_str("prompt = \"\"\"Read your task instructions from {instruction_path}. Read ONLY your instruction file, not other agents' instructions. Implement the feature described there. Make sure to add tests for new functionality. Commit your changes.\"\"\"\n");
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
        output.push_str(&format!("command = \"{}\"\n\n", escape_toml(command)));
    }
    append_docs_check_step(&mut output);
    append_write_pr_steps(&mut output);
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
    output.push_str("prompt = \"\"\"Read your task instructions from {instruction_path}. Read ONLY your instruction file, not other agents' instructions. Fix the bug described there. Add a regression test that would have caught this bug. Commit your changes.\"\"\"\n");
    output.push_str("max_retries = 3\n\n");
    if let Some(command) = &detected.commands.test {
        output.push_str("[[step]]\n");
        output.push_str("type = \"deterministic\"\n");
        output.push_str("name = \"test\"\n");
        output.push_str(&format!("command = \"{}\"\n\n", escape_toml(command)));
    }
    append_docs_check_step(&mut output);
    append_write_pr_steps(&mut output);
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
    output.push_str("prompt = \"\"\"Read your task instructions from {instruction_path}. Read ONLY your instruction file, not other agents' instructions. Refactor the code described there without changing intended behavior. Commit your changes once verification passes.\"\"\"\n");
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
        output.push_str(&format!("command = \"{}\"\n\n", escape_toml(command)));
    }
    append_docs_check_step(&mut output);
    append_write_pr_steps(&mut output);
    output
}

pub fn render_pr_review_blueprint(detected: &DetectedProject) -> String {
    let mut output = String::from(GENERATED_HEADER);
    output.push_str("[blueprint]\n");
    output.push_str("name = \"pr-review\"\n");
    output.push_str(
        "description = \"Senior engineer review of a PR — review implementation, merge to main, run final tests\"\n\n",
    );
    output.push_str("[[step]]\n");
    output.push_str("type = \"agentic\"\n");
    output.push_str("name = \"review\"\n");
    output.push_str("agent = \"codex\"\n");
    output.push_str("model = \"gpt-5.4\"\n");
    output.push_str("prompt = \"\"\"You are a senior engineer reviewing PR #{pr}.\n\n1. Read the PR description:\n   gh pr view {pr} --json title,body,additions,deletions,changedFiles\n\n2. Check out the branch and read the full diff:\n   gh pr checkout {pr}\n   git diff main...HEAD\n\n3. Review the implementation from the standpoint of the system as a whole:\n   - Does the design make sense in the context of the broader codebase?\n   - Are there architectural concerns, coupling issues, or missed edge cases?\n   - Is the code consistent with existing patterns and conventions?\n   - Are tests adequate — do they cover the new behaviour and edge cases?\n   - Is there anything the implementing agent missed or got wrong?\n\n4. If you find issues:\n   - Leave review comments via gh pr review {pr} --comment --body \\\"...\\\"\n   - Be specific: reference files, lines, and explain why it matters\n   - Distinguish blocking issues from suggestions\n\n5. If the code is good (or after addressing issues):\n   - Approve: gh pr review {pr} --approve --body \\\"LGTM — <brief summary>\\\"\"\"\"\n");
    output.push_str("max_retries = 1\n\n");
    output.push_str("[[step]]\n");
    output.push_str("type = \"agentic\"\n");
    output.push_str("name = \"merge\"\n");
    output.push_str("agent = \"codex\"\n");
    output.push_str("model = \"gpt-5.4\"\n");
    output.push_str("prompt = \"\"\"Merge PR #{pr} to main.\n\n1. First, try a clean merge:\n   gh pr merge {pr} --squash --auto\n\n2. If there are merge conflicts:\n   - Check out the PR branch\n   - Merge main into it: git merge main\n   - Resolve conflicts carefully — understand both sides before choosing\n   - Preserve the intent of both the PR and the conflicting changes\n   - Commit the resolution and push\n   - Then merge the PR\n\n3. If conflicts are too complex to resolve safely, do NOT force merge.\n   Instead, report what conflicts exist and stop.\"\"\"\n");
    output.push_str("max_retries = 1\n\n");
    output.push_str("[[step]]\n");
    output.push_str("type = \"deterministic\"\n");
    output.push_str("name = \"checkout-main\"\n");
    output.push_str("command = \"git checkout main && git pull\"\n\n");
    output.push_str("[[step]]\n");
    output.push_str("type = \"deterministic\"\n");
    output.push_str("name = \"post-merge-test\"\n");
    output.push_str(&format!(
        "command = \"{}\"\n\n",
        escape_toml(
            detected
                .commands
                .test
                .as_deref()
                .unwrap_or("{test_command}")
        )
    ));
    output.push_str("[[step]]\n");
    output.push_str("type = \"deterministic\"\n");
    output.push_str("name = \"post-merge-lint\"\n");
    output.push_str(&format!(
        "command = \"{}\"\n",
        escape_toml(
            detected
                .commands
                .lint
                .as_deref()
                .unwrap_or("{lint_command}")
        )
    ));
    output.push_str("allow_failure = true\n");
    output
}

fn append_write_pr_steps(output: &mut String) {
    output.push_str("[[step]]\n");
    output.push_str("type = \"agentic\"\n");
    output.push_str("name = \"write-pr\"\n");
    output.push_str("agent = \"{target_agent}\"\n");
    output.push_str("model = \"{target_model}\"\n");
    output.push_str("prompt = \"\"\"You have just completed work on this branch. Now write up a pull request.\n\n1. Run `git diff main...HEAD` to see everything you changed.\n2. Read the original task instructions at {instruction_path}.\n3. Write a PR description covering:\n   - What problem this solves (from the task brief)\n   - How you solved it (architectural decisions, key changes)\n   - What changed (files modified, new files, removed files)\n   - How to verify (what tests cover this, how to manually check)\n4. Open the PR: `gh pr create --base main --title \\\"<concise title>\\\" --body \\\"<your description>\\\"`\"\"\"\n\n");
    output.push_str("[[step]]\n");
    output.push_str("type = \"deterministic\"\n");
    output.push_str("name = \"verify-pr\"\n");
    output.push_str("command = \"gh pr view --json number,title,url --jq '.url'\"\n");
}

fn append_docs_check_step(output: &mut String) {
    output.push_str("[[step]]\n");
    output.push_str("type = \"agentic\"\n");
    output.push_str("name = \"docs-check\"\n");
    output.push_str("agent = \"{target_agent}\"\n");
    output.push_str("model = \"{target_model}\"\n");
    output.push_str("prompt = \"\"\"Review the changes you just made and check if the project documentation needs updating.\n\n1. Run `git diff main...HEAD --name-only` to see what files changed.\n2. Read README.md (if it exists) and check if any of these are now outdated:\n   - Feature descriptions that no longer match the code\n   - CLI usage examples that have changed\n   - Installation instructions that need updating\n   - Configuration options that were added or removed\n   - Project structure sections that don't reflect new/moved files\n3. Check docs/ directory (if it exists) for any files affected by your changes.\n4. Check AGENTS.md (if it exists) for outdated workflow instructions.\n5. If anything needs updating, make the changes and commit with message \\\"docs: update documentation to reflect recent changes\\\".\n6. If everything is already accurate, do nothing — don't make changes for the sake of it.\n\nOnly update documentation that is genuinely affected by the code changes. Do not rewrite docs that are still correct.\n\"\"\"\n");
    output.push_str("allow_failure = true\n\n");
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
    if !updated.contains(".forge/archive/") {
        updated.push_str(".forge/archive/\n");
    }
    if !updated.contains(".forge/runs/") {
        updated.push_str(".forge/runs/\n");
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

fn handle_stale_instruction_files(root: &Path) -> Result<(), ForgeError> {
    let stale = stale_root_instruction_files(root)?;
    if stale.is_empty() {
        return Ok(());
    }

    println!(
        "Found {} instruction-like files in repo root. Consider moving them to .forge/instructions/ or .forge/archive/.",
        stale.len()
    );
    Ok(())
}
