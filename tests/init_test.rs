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
            .join(".forge/blueprints/lint-and-test.toml")
            .exists()
    );
    assert!(
        dir.path()
            .join(".forge/blueprints/verify-base.toml")
            .exists()
    );
    assert!(dir.path().join(".forge/blueprints/build.toml").exists());
    assert!(dir.path().join(".forge/blueprints/fix-bug.toml").exists());
    assert!(dir.path().join(".forge/blueprints/pr-review.toml").exists());
    assert!(
        dir.path()
            .join(".forge/blueprints/code-review.toml")
            .exists()
    );
    assert!(dir.path().join(".forge/blueprints/phase.toml").exists());
    assert!(dir.path().join(".forge/blueprints/finalize.toml").exists());
    assert!(dir.path().join(".forge/blueprints/test.toml").exists());
    assert!(dir.path().join(".forge/instructions/.gitkeep").exists());
    assert!(dir.path().join(".forge/archive").exists());
    assert!(dir.path().join(".forge/.gitignore").exists());
    assert!(dir.path().join(".forge/INSTRUCTION_GUIDE.md").exists());
    assert!(!dir.path().join("AGENTS.md").exists());

    let gitignore = fs::read_to_string(dir.path().join(".gitignore")).expect("read gitignore");
    assert!(gitignore.contains(".forge/instructions/*"));
    assert!(gitignore.contains("!.forge/instructions/.gitkeep"));
    assert!(!gitignore.contains(".forge/archive/"));
    assert!(gitignore.contains(".forge/runs/"));

    let forge_gitignore =
        fs::read_to_string(dir.path().join(".forge/.gitignore")).expect("read forge gitignore");
    assert!(forge_gitignore.contains("instructions/*"));
    assert!(forge_gitignore.contains("!instructions/.gitkeep"));
    assert!(!forge_gitignore.contains("archive/"));
    assert!(forge_gitignore.contains("runs/"));

    let blueprint = fs::read_to_string(dir.path().join(".forge/blueprints/build.toml"))
        .expect("read build blueprint");
    assert!(blueprint.contains("Read your task instructions from {instruction_path}."));
    assert!(blueprint.contains("type = \"blueprint\""));
    assert!(blueprint.contains("blueprint = \"lint-and-test\""));
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
    assert!(pr_review.contains("name = \"post-merge-verify\""));
    assert!(pr_review.contains("blueprint = \"lint-and-test\""));

    let code_review = fs::read_to_string(dir.path().join(".forge/blueprints/code-review.toml"))
        .expect("read code-review blueprint");
    assert!(code_review.contains("name = \"code-review\""));
    assert!(code_review.contains("name = \"checkout-pr\""));
    assert!(code_review.contains("command = \"gh pr checkout {pr}\""));
    assert!(code_review.contains("name = \"review\""));

    let phase = fs::read_to_string(dir.path().join(".forge/blueprints/phase.toml"))
        .expect("read phase blueprint");
    assert!(phase.contains("name = \"phase\""));
    assert!(phase.contains("name = \"checkout-or-create-branch\""));
    assert!(phase.contains("name = \"implement-phase\""));
    assert!(phase.contains("name = \"commit-backstop\""));
    assert!(phase.contains("name = \"verify\""));
    assert!(phase.contains("type = \"blueprint\""));
    assert!(phase.contains("blueprint = \"lint-and-test\""));
    assert!(phase.contains("{phase_branch}"));

    let finalize = fs::read_to_string(dir.path().join(".forge/blueprints/finalize.toml"))
        .expect("read finalize blueprint");
    assert!(finalize.contains("name = \"final-verify\""));
    assert!(finalize.contains("blueprint = \"lint-and-test\""));
    assert!(finalize.contains("{phase_branch}"));

    let lint_and_test =
        fs::read_to_string(dir.path().join(".forge/blueprints/lint-and-test.toml"))
            .expect("read lint-and-test blueprint");
    assert!(lint_and_test.contains("name = \"lint-and-test\""));
    assert!(lint_and_test.contains("name = \"lint\""));
    assert!(lint_and_test.contains("name = \"fix-lint\""));
    assert!(lint_and_test.contains("{lint.log_file}"));
    assert!(lint_and_test.contains("{lint_command}"));
    assert!(lint_and_test.contains("name = \"test\""));
    assert!(lint_and_test.contains("name = \"fix-tests\""));
    assert!(lint_and_test.contains("{test.log_file}"));
    assert!(lint_and_test.contains("{test_command}"));

    for branching in ["build", "fix-bug"] {
        let blueprint = fs::read_to_string(
            dir.path()
                .join(".forge/blueprints")
                .join(format!("{branching}.toml")),
        )
        .expect("read branching blueprint");
        assert!(blueprint.contains("name = \"verify\""));
        assert!(blueprint.contains("blueprint = \"lint-and-test\""));
        assert!(blueprint.contains("name = \"verify-base\""));
        assert!(blueprint.contains("blueprint = \"verify-base\""));
        assert!(blueprint.contains("name = \"archive-instruction\""));
        assert!(blueprint.contains("git mv .forge/instructions/{instruction_file}"));
    }

    let verify_base = fs::read_to_string(dir.path().join(".forge/blueprints/verify-base.toml"))
        .expect("read verify-base blueprint");
    assert!(verify_base.contains("name = \"verify-base\""));
    assert!(verify_base.contains("git fetch origin {default_branch}"));
    assert!(verify_base.contains("git rev-parse origin/{default_branch}"));

    let config = fs::read_to_string(dir.path().join(".forge/config.toml")).expect("read config");
    assert!(config.contains("auto_archive = false"));

    let test_blueprint = fs::read_to_string(dir.path().join(".forge/blueprints/test.toml"))
        .expect("read test blueprint");
    assert!(test_blueprint.contains("name = \"test\""));
    assert!(test_blueprint.contains("description = \"Run the project's test command and try to fix failures\""));
    assert!(test_blueprint.contains("command = \"cargo test\""));
    assert!(test_blueprint.contains("name = \"fix-tests\""));
    assert!(test_blueprint.contains("{test.log_file}"));
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
        dir.path().join(".forge/blueprints/build.toml"),
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

    assert!(error.to_string().contains("build.toml"));
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
