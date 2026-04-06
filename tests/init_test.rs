use forge::commands::init::{InitOptions, init_project};
use forge::detect::ProjectType;
use std::fs;
use tempfile::tempdir;

#[test]
fn init_creates_forge_layout_and_gitignore_entries() {
    let dir = tempdir().expect("tempdir");
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("write cargo");
    fs::write(dir.path().join(".gitignore"), "target/\n").expect("write gitignore");

    let detected = init_project(
        dir.path(),
        &InitOptions {
            project_type: None,
            force: false,
        },
    )
    .expect("init project");

    assert_eq!(detected.project_type, ProjectType::Rust);
    assert!(dir.path().join(".forge/config.toml").exists());
    assert!(
        dir.path()
            .join(".forge/blueprints/new-feature.toml")
            .exists()
    );
    assert!(dir.path().join(".forge/blueprints/fix-bug.toml").exists());
    assert!(dir.path().join(".forge/blueprints/refactor.toml").exists());
    assert!(dir.path().join(".forge/blueprints/pr-review.toml").exists());
    assert!(
        dir.path()
            .join(".forge/blueprints/code-review.toml")
            .exists()
    );
    assert!(
        dir.path()
            .join(".forge/blueprints/refactor-phase.toml")
            .exists()
    );
    assert!(
        dir.path()
            .join(".forge/blueprints/refactor-finalize.toml")
            .exists()
    );
    assert!(dir.path().join(".forge/blueprints/test.toml").exists());
    assert!(dir.path().join(".forge/instructions/.gitkeep").exists());
    assert!(dir.path().join(".forge/archive").exists());
    assert!(dir.path().join(".forge/.gitignore").exists());
    assert!(!dir.path().join("AGENTS.md").exists());

    let gitignore = fs::read_to_string(dir.path().join(".gitignore")).expect("read gitignore");
    assert!(gitignore.contains(".forge/instructions/*"));
    assert!(gitignore.contains("!.forge/instructions/.gitkeep"));
    assert!(gitignore.contains(".forge/archive/"));
    assert!(gitignore.contains(".forge/runs/"));

    let forge_gitignore =
        fs::read_to_string(dir.path().join(".forge/.gitignore")).expect("read forge gitignore");
    assert!(forge_gitignore.contains("instructions/*"));
    assert!(forge_gitignore.contains("!instructions/.gitkeep"));
    assert!(forge_gitignore.contains("archive/"));
    assert!(forge_gitignore.contains("runs/"));

    let blueprint = fs::read_to_string(dir.path().join(".forge/blueprints/new-feature.toml"))
        .expect("read new-feature blueprint");
    assert!(blueprint.contains("Read your task instructions from {instruction_path}."));
    assert!(blueprint.contains("name = \"docs-check\""));
    assert!(blueprint.contains("git diff {default_branch}...HEAD --name-only"));
    assert!(blueprint.contains("allow_failure = true"));
    assert!(blueprint.contains("name = \"write-pr\""));
    assert!(blueprint.contains("name = \"create-pr\""));
    assert!(blueprint.contains("name = \"checkout-main\""));
    assert!(blueprint.contains(".forge/pr-body.md"));
    assert!(!blueprint.contains("{run_id}"));

    let pr_review = fs::read_to_string(dir.path().join(".forge/blueprints/pr-review.toml"))
        .expect("read pr-review blueprint");
    assert!(pr_review.contains("name = \"pr-review\""));
    assert!(pr_review.contains("gh pr checkout {pr}"));
    assert!(pr_review.contains("name = \"post-merge-test\""));
    assert!(pr_review.contains("name = \"post-merge-lint\""));

    let code_review = fs::read_to_string(dir.path().join(".forge/blueprints/code-review.toml"))
        .expect("read code-review blueprint");
    assert!(code_review.contains("name = \"code-review\""));
    assert!(code_review.contains("name = \"checkout-pr\""));
    assert!(code_review.contains("command = \"gh pr checkout {pr}\""));
    assert!(code_review.contains("name = \"review\""));

    let refactor_phase =
        fs::read_to_string(dir.path().join(".forge/blueprints/refactor-phase.toml"))
            .expect("read refactor-phase blueprint");
    assert!(refactor_phase.contains("name = \"refactor-phase\""));
    assert!(refactor_phase.contains("name = \"checkout-or-create-branch\""));
    assert!(refactor_phase.contains("name = \"implement-phase\""));
    assert!(refactor_phase.contains("name = \"commit-backstop\""));
    assert!(refactor_phase.contains("name = \"test\""));
    assert!(refactor_phase.contains("allow_failure = true"));
    assert!(refactor_phase.contains("name = \"fix-tests\""));
    assert!(refactor_phase.contains("condition = \"test.exit_code != 0\""));

    for branching in ["new-feature", "fix-bug", "refactor"] {
        let blueprint = fs::read_to_string(
            dir.path()
                .join(".forge/blueprints")
                .join(format!("{branching}.toml")),
        )
        .expect("read branching blueprint");
        assert!(blueprint.contains("name = \"fix-lint\""));
        assert!(blueprint.contains("condition = \"lint.exit_code != 0\""));
    }

    let test_blueprint = fs::read_to_string(dir.path().join(".forge/blueprints/test.toml"))
        .expect("read test blueprint");
    assert!(test_blueprint.contains("name = \"test\""));
    assert!(test_blueprint.contains("description = \"Run the project's test command\""));
    assert!(test_blueprint.contains("command = \"cargo test\""));
}

#[test]
fn init_refuses_to_overwrite_manual_blueprint_without_force() {
    let dir = tempdir().expect("tempdir");
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("write cargo");
    fs::create_dir_all(dir.path().join(".forge/blueprints")).expect("blueprints");
    fs::write(
        dir.path().join(".forge/blueprints/new-feature.toml"),
        "[blueprint]\nname = \"manual\"\ndescription = \"manual\"\n",
    )
    .expect("write manual blueprint");

    let error = init_project(
        dir.path(),
        &InitOptions {
            project_type: None,
            force: false,
        },
    )
    .expect_err("init should refuse overwrite");

    assert!(error.to_string().contains("new-feature.toml"));
}

#[test]
fn init_warns_about_stale_instruction_like_files_in_repo_root() {
    let dir = tempdir().expect("tempdir");
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("write cargo");
    fs::write(dir.path().join("CODEX_BRIEF_feature.md"), "task").expect("write stale file");
    fs::write(dir.path().join("CODEX_INSTRUCTIONS_fix.md"), "task").expect("write stale file");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_forge"))
        .arg("init")
        .current_dir(dir.path())
        .output()
        .expect("run forge init");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Found 2 instruction-like files in repo root."));
    assert!(stdout.contains(".forge/instructions/ or .forge/archive/"));
}
