use crate::detect::{DetectedProject, ProjectType, detect_project};
use crate::error::ForgeError;
use crate::workspace::{ensure_workspace_layout, stale_root_instruction_files};
use std::fs;
use std::path::Path;

const GENERATED_HEADER: &str = "# forge-generated\n";
const DEFAULT_BRANCH_VAR: &str = "{default_branch}";
const INSTRUCTION_GUIDE: &str = include_str!("instruction_guide.md");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitOptions {
    pub project_type: Option<ProjectType>,
    pub force: bool,
}

pub fn init_project(root: &Path, options: &InitOptions) -> Result<DetectedProject, ForgeError> {
    let detected = detect_project(root, options.project_type)?;
    write_generated_files(root, &detected, options.force)?;
    ensure_instructions_gitignore(root)?;
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
    for (filename, contents) in render_default_blueprints(detected) {
        write_generated_file(
            &root.join(".forge/blueprints").join(filename),
            &contents,
            force,
        )?;
    }

    write_instruction_guide(&root.join(".forge/INSTRUCTION_GUIDE.md"), force)?;

    let config = crate::config::load_forge_config_str(&render_config(detected))?;
    ensure_workspace_layout(root, &config)?;

    Ok(())
}

fn render_default_blueprints(detected: &DetectedProject) -> Vec<(&'static str, String)> {
    let mut blueprints = vec![
        ("lint-and-test.toml", render_lint_and_test_blueprint(detected)),
        ("verify-base.toml", render_verify_base_blueprint()),
        ("build.toml", render_build_blueprint(detected)),
        ("fix-bug.toml", render_fix_bug_blueprint(detected)),
        ("pr-review.toml", render_pr_review_blueprint(detected)),
        (
            "review-codebase.toml",
            render_review_codebase_blueprint(detected),
        ),
        ("phase.toml", render_phase_blueprint(detected)),
        ("finalize.toml", render_finalize_blueprint(detected)),
    ];
    if detected.commands.test.is_some() {
        blueprints.push(("test.toml", render_test_blueprint(detected)));
    }
    blueprints
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
    // Archiving is handled inside branching blueprints (archive-instruction step)
    // so the file move rides along with the PR and nothing touches local main
    // after the run completes. The post-run auto-archive would move the file on
    // main and leave uncommitted changes, breaking the next run's clean-tree.
    output.push_str("auto_archive = false\n");
    output.push_str("\n[dashboard]\n");
    output.push_str("# Frontend hasn't shipped yet, so the dashboard is opt-in for now.\n");
    output.push_str("enabled = false\n");
    output
}

pub fn render_verify_base_blueprint() -> String {
    let mut output = String::from(GENERATED_HEADER);
    output.push_str("[blueprint]\n");
    output.push_str("name = \"verify-base\"\n");
    output.push_str(&format!(
        "description = \"Assert local HEAD matches origin/{DEFAULT_BRANCH_VAR} before branching\"\n\n"
    ));
    append_command_step(
        &mut output,
        "verify",
        &format!(
            "git fetch origin {DEFAULT_BRANCH_VAR} --quiet && if [ \"$(git rev-parse HEAD)\" != \"$(git rev-parse origin/{DEFAULT_BRANCH_VAR})\" ]; then echo 'error: local HEAD differs from origin/{DEFAULT_BRANCH_VAR}. Push local commits or reset before branching — otherwise a squash-merge will absorb those commits and cause rebase conflicts on post-merge pull.' >&2; exit 1; fi"
        ),
        false,
    );
    output
}

pub fn render_build_blueprint(detected: &DetectedProject) -> String {
    render_branching_blueprint(
        detected,
        "build",
        "Implement a task (feature or refactor) with lint and test gates",
        "build",
        "Read your task instructions from {instruction_path}. Read ONLY your instruction file, not other agents' instructions. Implement the task described there. The instruction file defines scope — whether new behaviour, a refactor that preserves behaviour, or something else. Follow what it says.",
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

fn render_branching_blueprint(
    _detected: &DetectedProject,
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
    append_blueprint_step(&mut output, "verify-base", "verify-base");
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
    append_blueprint_step(&mut output, "verify", "lint-and-test");
    append_docs_check_step(&mut output);
    append_command_step(
        &mut output,
        "docs-commit-backstop",
        "git add -A && git diff --cached --quiet || git commit -m \"docs: update documentation\"",
        false,
    );
    append_command_step(
        &mut output,
        "archive-instruction",
        "if [ -f .forge/instructions/{instruction_file} ]; then mkdir -p .forge/archive && git mv .forge/instructions/{instruction_file} .forge/archive/{instruction_file} && git commit -m 'chore: archive {instruction_file}'; fi",
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
        &format!(
            "gh pr create --base {DEFAULT_BRANCH_VAR} --head {{branch}} --body-file .forge/pr-body.md --title \"{{commit_message}}\" && rm -f .forge/pr-body.md"
        ),
        false,
    );
    append_command_step(
        &mut output,
        "checkout-main",
        &format!("git checkout {DEFAULT_BRANCH_VAR}"),
        false,
    );
    output
}

pub fn render_review_codebase_blueprint(_detected: &DetectedProject) -> String {
    let mut output = String::from(GENERATED_HEADER);
    output.push_str("[blueprint]\n");
    output.push_str("name = \"review-codebase\"\n");
    output.push_str(
        "description = \"Sweep the codebase for dead code, unused exports, stray TODOs, placeholder stubs, and inconsistent patterns; write findings to an instruction file for later triage\"\n\n",
    );
    output.push_str("[[step]]\n");
    output.push_str("type = \"agentic\"\n");
    output.push_str("name = \"sweep\"\n");
    output.push_str("agent = \"{target_agent}\"\n");
    output.push_str("model = \"{target_model}\"\n");
    output.push_str("prompt = \"\"\"Perform a hygiene review of the codebase. Surface issues for later triage — do NOT modify code in this run.\n\n1. Scan for:\n   - Dead code: functions, types, constants, or modules that are never referenced.\n   - Unused imports and exports.\n   - Stray TODO / FIXME / XXX / HACK markers — especially older ones.\n   - Placeholder or stub code: `unimplemented!()`, `todo!()`, `pass`, empty bodies, `return null // TODO`, mock returns left in real paths.\n   - Overly clever, dense, or hard-to-follow code that should be simplified.\n   - Patterns that drift from the rest of the codebase (naming, error handling, module layout).\n   - Misleading names or comments that no longer match behaviour.\n\n2. Prefer tooling where it exists:\n   - Compiler / linter warnings about dead or unused items.\n   - `git grep -nE \\\"TODO|FIXME|XXX|HACK\\\"` for markers.\n   - Language-specific dead-code tools (e.g. `cargo +nightly udeps`, `vulture`, `ts-prune`, `knip`).\n\n3. For each finding, capture:\n   - File path and approximate lines.\n   - What is wrong and why it matters.\n   - Suggested action (delete, implement, refactor, rename, clarify).\n   - Severity: blocker / cleanup / nitpick.\n\n4. Write the report to `.forge/instructions/review-codebase-{date}.md` following `.forge/INSTRUCTION_GUIDE.md` so individual findings can be handed to `build` or `fix-bug` later.\n\n5. Finish by printing the path of the report and a one-line summary (N blockers, N cleanup, N nitpicks). Do not edit source files.\"\"\"\n");
    output.push_str("max_retries = 1\n");
    output
}

pub fn render_pr_review_blueprint(_detected: &DetectedProject) -> String {
    let mut output = String::from(GENERATED_HEADER);
    output.push_str("[blueprint]\n");
    output.push_str("name = \"pr-review\"\n");
    output.push_str(
        "description = \"Senior engineer review of a PR — review implementation, merge to the default branch, run final tests\"\n\n",
    );
    append_command_step(&mut output, "clean-tree", "git diff --quiet && git diff --cached --quiet", false);
    output.push('\n');
    output.push_str("[[step]]\n");
    output.push_str("type = \"agentic\"\n");
    output.push_str("name = \"review\"\n");
    output.push_str("agent = \"codex\"\n");
    output.push_str("model = \"gpt-5.4\"\n");
    output.push_str(&format!("prompt = \"\"\"You are a senior engineer reviewing PR #{{pr}}.\n\n1. Read the PR description:\n   gh pr view {{pr}} --json title,body,additions,deletions,changedFiles\n\n2. Check out the branch and read the full diff:\n   gh pr checkout {{pr}}\n   git diff {DEFAULT_BRANCH_VAR}...HEAD\n\n3. Review the implementation from the standpoint of the system as a whole:\n   - Does the design make sense in the context of the broader codebase?\n   - Are there architectural concerns, coupling issues, or missed edge cases?\n   - Is the code consistent with existing patterns and conventions?\n   - Are tests adequate — do they cover the new behaviour and edge cases?\n   - Is there anything the implementing agent missed or got wrong?\n\n4. If you find issues:\n   - Leave review comments via gh pr review {{pr}} --comment --body \\\"...\\\"\n   - Be specific: reference files, lines, and explain why it matters\n   - Distinguish blocking issues from suggestions\n\n5. If the code is good (or after addressing issues):\n   - Approve: gh pr review {{pr}} --approve --body \\\"LGTM — <brief summary>\\\"\"\"\"\n"));
    output.push_str("max_retries = 1\n\n");
    output.push_str("[[step]]\n");
    output.push_str("type = \"agentic\"\n");
    output.push_str("name = \"merge\"\n");
    output.push_str("agent = \"codex\"\n");
    output.push_str("model = \"gpt-5.4\"\n");
    output.push_str(&format!("prompt = \"\"\"Merge PR #{{pr}} to the default branch.\n\n1. First, try a clean merge:\n   gh pr merge {{pr}} --squash --auto\n\n2. If there are merge conflicts:\n   - Check out the PR branch\n   - Merge the default branch into it: git merge {DEFAULT_BRANCH_VAR}\n   - Resolve conflicts carefully — understand both sides before choosing\n   - Preserve the intent of both the PR and the conflicting changes\n   - Commit the resolution and push\n   - Then merge the PR\n\n3. If conflicts are too complex to resolve safely, do NOT force merge.\n   Instead, report what conflicts exist and stop.\"\"\"\n"));
    output.push_str("max_retries = 1\n\n");
    output.push_str("[[step]]\n");
    output.push_str("type = \"deterministic\"\n");
    output.push_str("name = \"checkout-main\"\n");
    output.push_str(&format!(
        "command = \"git checkout {DEFAULT_BRANCH_VAR} && git pull\"\n\n"
    ));
    append_blueprint_step(&mut output, "post-merge-verify", "lint-and-test");
    output
}

pub fn render_phase_blueprint(_detected: &DetectedProject) -> String {
    let mut output = String::from(GENERATED_HEADER);
    output.push_str("[blueprint]\n");
    output.push_str("name = \"phase\"\n");
    output.push_str(
        "description = \"Execute a single implementation phase of multi-phase work\"\n\n",
    );
    append_command_step(
        &mut output,
        "checkout-or-create-branch",
        "git checkout {phase_branch} 2>/dev/null || git checkout -b {phase_branch}",
        false,
    );
    output.push_str("[[step]]\n");
    output.push_str("type = \"agentic\"\n");
    output.push_str("name = \"implement-phase\"\n");
    output.push_str("agent = \"{target_agent}\"\n");
    output.push_str("model = \"{target_model}\"\n");
    output.push_str("prompt = \"\"\"Read your task instructions from {instruction_path}. Read ONLY your instruction file, not other agents' instructions. Implement this phase. Stay within the scope defined in the instruction file and do not change intended behavior outside that scope.\"\"\"\n\n");
    append_command_step(
        &mut output,
        "commit-backstop",
        "git add -A && git diff --cached --quiet || git commit -m \"{commit_message}\"",
        false,
    );
    append_blueprint_step(&mut output, "verify", "lint-and-test");
    output
}

pub fn render_finalize_blueprint(_detected: &DetectedProject) -> String {
    let mut output = String::from(GENERATED_HEADER);
    output.push_str("[blueprint]\n");
    output.push_str("name = \"finalize\"\n");
    output.push_str(
        "description = \"Finalize multi-phase work and open the pull request\"\n\n",
    );
    append_command_step(
        &mut output,
        "checkout-branch",
        "git checkout {phase_branch}",
        false,
    );
    append_blueprint_step(&mut output, "final-verify", "lint-and-test");
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
        "git push origin {phase_branch}",
        false,
    );
    append_write_pr_steps(&mut output);
    append_command_step(
        &mut output,
        "create-pr",
        &format!(
            "gh pr create --base {DEFAULT_BRANCH_VAR} --head {{phase_branch}} --body-file .forge/pr-body.md --title \"{{commit_message}}\" && rm -f .forge/pr-body.md"
        ),
        false,
    );
    append_command_step(
        &mut output,
        "checkout-main",
        &format!("git checkout {DEFAULT_BRANCH_VAR}"),
        false,
    );
    output
}

pub fn render_test_blueprint(detected: &DetectedProject) -> String {
    let mut output = String::from(GENERATED_HEADER);
    output.push_str("[blueprint]\n");
    output.push_str("name = \"test\"\n");
    output.push_str("description = \"Run the project's test command and try to fix failures\"\n\n");
    append_command_step(
        &mut output,
        "test",
        detected
            .commands
            .test
            .as_deref()
            .unwrap_or("{test_command}"),
        true,
    );
    append_agentic_retry_step(
        &mut output,
        "fix-tests",
        "The test step failed.\n\nRead the failing test log first: {test.log_file}\nThe exact test command is: {test_command}\n\nRequirements:\n1. Work from the repo root.\n2. Read the log before changing anything.\n3. Fix only the failures shown by that test step.\n4. Reproduce with the same command before making broader guesses.\n5. Do not substitute a different test command unless the log makes clear that a narrower repro is needed after you have read the failing output.\n6. Do not change functionality outside what is required to fix the failing tests.\n7. Do not edit unrelated files such as Forge blueprints/instructions unless the failure is in those files.\n\nAfter making changes, rerun exactly: {test_command}",
        "test.exit_code != 0",
    );
    output
}

pub fn render_lint_and_test_blueprint(detected: &DetectedProject) -> String {
    let mut output = String::from(GENERATED_HEADER);
    output.push_str("[blueprint]\n");
    output.push_str("name = \"lint-and-test\"\n");
    output.push_str("description = \"Run lint and tests, then fix failures using the recorded logs\"\n\n");

    append_command_step(
        &mut output,
        "lint",
        detected
            .commands
            .lint
            .as_deref()
            .unwrap_or("{lint_command}"),
        true,
    );
    append_agentic_retry_step(
        &mut output,
        "fix-lint",
        "The lint step failed.\n\nRead the failing lint log first: {lint.log_file}\nThe exact lint command is: {lint_command}\n\nRequirements:\n1. Work from the repo root.\n2. Read the log before changing anything.\n3. Fix only the failures shown by that lint step.\n4. Address the first failing gate before anything else. If formatting fails, fix formatting before investigating deeper lint output.\n5. Do not substitute a different lint command or drift into package-local checks unless the lint log explicitly requires it.\n6. Do not change functionality.\n7. Do not edit unrelated files such as Forge blueprints/instructions unless the lint failure is in those files.\n\nAfter making changes, rerun exactly: {lint_command}",
        "lint.exit_code != 0",
    );
    append_command_step(
        &mut output,
        "test",
        detected
            .commands
            .test
            .as_deref()
            .unwrap_or("{test_command}"),
        true,
    );
    append_agentic_retry_step(
        &mut output,
        "fix-tests",
        "The test step failed.\n\nRead the failing test log first: {test.log_file}\nThe exact test command is: {test_command}\n\nRequirements:\n1. Work from the repo root.\n2. Read the log before changing anything.\n3. Fix only the failures shown by that test step.\n4. Reproduce with the same command before making broader guesses.\n5. Do not substitute a different test command unless the log makes clear that a narrower repro is needed after you have read the failing output.\n6. Do not change functionality outside what is required to fix the failing tests.\n7. Do not edit unrelated files such as Forge blueprints/instructions unless the failure is in those files.\n\nAfter making changes, rerun exactly: {test_command}",
        "test.exit_code != 0",
    );
    output
}

fn append_write_pr_steps(output: &mut String) {
    output.push_str("[[step]]\n");
    output.push_str("type = \"agentic\"\n");
    output.push_str("name = \"write-pr\"\n");
    output.push_str("agent = \"{target_agent}\"\n");
    output.push_str("model = \"{target_model}\"\n");
    output.push_str(&format!("prompt = \"\"\"You have just completed work on this branch. Now write up a pull request description.\n\n1. Run `git diff {DEFAULT_BRANCH_VAR}...HEAD` to see everything you changed.\n2. Read the original task instructions at {{instruction_path}}.\n3. Write a PR description covering:\n   - What problem this solves (from the task brief)\n   - How you solved it (architectural decisions, key changes)\n   - What changed (files modified, new files, removed files)\n   - How to verify (what tests cover this, how to manually check)\n4. Save the PR description to `.forge/pr-body.md`.\n\nDo not create the PR yourself. Only write the contents for `.forge/pr-body.md`.\"\"\"\n\n"));
}

fn append_docs_check_step(output: &mut String) {
    output.push_str("[[step]]\n");
    output.push_str("type = \"agentic\"\n");
    output.push_str("name = \"docs-check\"\n");
    output.push_str("agent = \"{target_agent}\"\n");
    output.push_str("model = \"{target_model}\"\n");
    output.push_str(&format!("prompt = \"\"\"Review the changes you just made and check if the project documentation needs updating.\n\n1. Run `git diff {DEFAULT_BRANCH_VAR}...HEAD --name-only` to see what files changed.\n2. Read README.md (if it exists) and check if any of these are now outdated:\n   - Feature descriptions that no longer match the code\n   - CLI usage examples that have changed\n   - Installation instructions that need updating\n   - Configuration options that were added or removed\n   - Project structure sections that don't reflect new/moved files\n3. Check docs/ directory (if it exists) for any files affected by your changes.\n4. Check AGENTS.md (if it exists) for outdated workflow instructions.\n5. If anything needs updating, make the changes.\n6. If everything is already accurate, do nothing — don't make changes for the sake of it.\n\nOnly update documentation that is genuinely affected by the code changes. Do not rewrite docs that are still correct.\n\"\"\"\n"));
    output.push_str("allow_failure = true\n\n");
}

fn append_agentic_retry_step(output: &mut String, name: &str, prompt: &str, condition: &str) {
    output.push_str("[[step]]\n");
    output.push_str("type = \"agentic\"\n");
    output.push_str(&format!("name = \"{}\"\n", escape_toml(name)));
    output.push_str("agent = \"{target_agent}\"\n");
    output.push_str("model = \"{target_model}\"\n");
    output.push_str(&format!("prompt = \"\"\"{}\"\"\"\n", prompt));
    output.push_str(&format!("condition = \"{}\"\n", escape_toml(condition)));
    output.push_str("max_retries = 2\n\n");
}

fn append_blueprint_step(output: &mut String, name: &str, blueprint: &str) {
    output.push_str("[[step]]\n");
    output.push_str("type = \"blueprint\"\n");
    output.push_str(&format!("name = \"{}\"\n", escape_toml(name)));
    output.push_str(&format!("blueprint = \"{}\"\n\n", escape_toml(blueprint)));
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
    if !updated.contains(".forge/runs/") {
        updated.push_str(".forge/runs/\n");
    }

    if updated != existing {
        fs::write(path, updated)?;
    }

    Ok(())
}

fn write_instruction_guide(path: &Path, force: bool) -> Result<(), ForgeError> {
    if path.exists() && !force {
        // Leave user-edited guides alone unless explicitly forced.
        return Ok(());
    }
    fs::write(path, INSTRUCTION_GUIDE)?;
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
