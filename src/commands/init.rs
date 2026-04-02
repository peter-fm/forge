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
    if detected.commands.test.is_some() {
        write_generated_file(
            &root.join(".forge/blueprints/test.toml"),
            &render_test_blueprint(detected),
            force,
        )?;
    }

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
    render_branching_blueprint(
        detected,
        "new-feature",
        "Implement a new feature with lint and test gates",
        "implement",
        "Read your task instructions from {instruction_path}. Read ONLY your instruction file, not other agents' instructions. Implement the feature described there. Make sure to add tests for new functionality.",
        Some(2),
    )
}

pub fn render_fix_bug_blueprint(detected: &DetectedProject) -> String {
    render_branching_blueprint(
        detected,
        "fix-bug",
        "Fix a bug with test verification",
        "fix",
        "Read your task instructions from {instruction_path}. Read ONLY your instruction file, not other agents' instructions. Fix the bug described there. Add a regression test that would have caught this bug.",
        Some(3),
    )
}

pub fn render_refactor_blueprint(detected: &DetectedProject) -> String {
    render_branching_blueprint(
        detected,
        "refactor",
        "Refactor code with verification gates",
        "refactor",
        "Read your task instructions from {instruction_path}. Read ONLY your instruction file, not other agents' instructions. Refactor the code described there without changing intended behavior.",
        None,
    )
}

fn render_branching_blueprint(
    detected: &DetectedProject,
    name: &str,
    description: &str,
    creative_step_name: &str,
    creative_prompt: &str,
    max_retries: Option<u32>,
) -> String {
    let mut output = String::from(GENERATED_HEADER);
    output.push_str("[blueprint]\n");
    output.push_str(&format!("name = \"{}\"\n", escape_toml(name)));
    output.push_str(&format!(
        "description = \"{}\"\n\n",
        escape_toml(description)
    ));

    append_command_step(
        &mut output,
        "clean-tree",
        "git diff --quiet || exit 1",
        false,
    );
    append_command_step(
        &mut output,
        "create-branch",
        "git checkout -b {branch}",
        false,
    );
    output.push_str("[[step]]\n");
    output.push_str("type = \"agentic\"\n");
    output.push_str(&format!("name = \"{}\"\n", escape_toml(creative_step_name)));
    output.push_str("agent = \"{target_agent}\"\n");
    output.push_str("model = \"{target_model}\"\n");
    output.push_str(&format!("prompt = \"\"\"{}\"\"\"\n", creative_prompt));
    if let Some(max_retries) = max_retries {
        output.push_str(&format!("max_retries = {max_retries}\n"));
    }
    output.push('\n');

    append_command_step(
        &mut output,
        "commit-backstop",
        "git add -A && git diff --cached --quiet || git commit -m \"{commit_message}\"",
        false,
    );
    append_command_step(
        &mut output,
        "lint",
        detected
            .commands
            .lint
            .as_deref()
            .unwrap_or("{lint_command}"),
        false,
    );
    append_command_step(
        &mut output,
        "test",
        detected
            .commands
            .test
            .as_deref()
            .unwrap_or("{test_command}"),
        false,
    );
    append_docs_check_step(&mut output);
    append_command_step(
        &mut output,
        "docs-commit-backstop",
        "git add -A && git diff --cached --quiet || git commit -m \"docs: update documentation\"",
        false,
    );
    append_command_step(
        &mut output,
        "push-branch",
        "git push origin {branch}",
        false,
    );
    append_write_pr_steps(&mut output);
    append_command_step(
        &mut output,
        "create-pr",
        "gh pr create --base main --head {branch} --body-file .forge/pr-body.md --title \"{commit_message}\"",
        false,
    );
    append_command_step(&mut output, "checkout-main", "git checkout main", false);
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

pub fn render_test_blueprint(detected: &DetectedProject) -> String {
    let mut output = String::from(GENERATED_HEADER);
    output.push_str("[blueprint]\n");
    output.push_str("name = \"test\"\n");
    output.push_str("description = \"Run the project's test command\"\n\n");
    output.push_str("[[step]]\n");
    output.push_str("type = \"deterministic\"\n");
    output.push_str("name = \"test\"\n");
    output.push_str(&format!(
        "command = \"{}\"\n",
        escape_toml(
            detected
                .commands
                .test
                .as_deref()
                .unwrap_or("{test_command}")
        )
    ));
    output
}

fn append_write_pr_steps(output: &mut String) {
    output.push_str("[[step]]\n");
    output.push_str("type = \"agentic\"\n");
    output.push_str("name = \"write-pr\"\n");
    output.push_str("agent = \"{target_agent}\"\n");
    output.push_str("model = \"{target_model}\"\n");
    output.push_str("prompt = \"\"\"You have just completed work on this branch. Now write up a pull request description.\n\n1. Run `git diff main...HEAD` to see everything you changed.\n2. Read the original task instructions at {instruction_path}.\n3. Write a PR description covering:\n   - What problem this solves (from the task brief)\n   - How you solved it (architectural decisions, key changes)\n   - What changed (files modified, new files, removed files)\n   - How to verify (what tests cover this, how to manually check)\n4. Save the PR description to `.forge/pr-body.md`.\n\nDo not create the PR yourself. Only write the contents for `.forge/pr-body.md`.\"\"\"\n\n");
}

fn append_docs_check_step(output: &mut String) {
    output.push_str("[[step]]\n");
    output.push_str("type = \"agentic\"\n");
    output.push_str("name = \"docs-check\"\n");
    output.push_str("agent = \"{target_agent}\"\n");
    output.push_str("model = \"{target_model}\"\n");
    output.push_str("prompt = \"\"\"Review the changes you just made and check if the project documentation needs updating.\n\n1. Run `git diff main...HEAD --name-only` to see what files changed.\n2. Read README.md (if it exists) and check if any of these are now outdated:\n   - Feature descriptions that no longer match the code\n   - CLI usage examples that have changed\n   - Installation instructions that need updating\n   - Configuration options that were added or removed\n   - Project structure sections that don't reflect new/moved files\n3. Check docs/ directory (if it exists) for any files affected by your changes.\n4. Check AGENTS.md (if it exists) for outdated workflow instructions.\n5. If anything needs updating, make the changes.\n6. If everything is already accurate, do nothing — don't make changes for the sake of it.\n\nOnly update documentation that is genuinely affected by the code changes. Do not rewrite docs that are still correct.\n\"\"\"\n");
    output.push_str("allow_failure = true\n\n");
}

fn append_command_step(output: &mut String, name: &str, command: &str, allow_failure: bool) {
    output.push_str("[[step]]\n");
    output.push_str("type = \"deterministic\"\n");
    output.push_str(&format!("name = \"{}\"\n", escape_toml(name)));
    output.push_str(&format!("command = \"{}\"\n", escape_toml(command)));
    if allow_failure {
        output.push_str("allow_failure = true\n");
    }
    output.push('\n');
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
