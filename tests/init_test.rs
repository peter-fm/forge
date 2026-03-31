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
    assert!(dir.path().join(".forge/instructions/.gitkeep").exists());
    assert!(dir.path().join("AGENTS.md").exists());

    let gitignore = fs::read_to_string(dir.path().join(".gitignore")).expect("read gitignore");
    assert!(gitignore.contains(".forge/instructions/*"));
    assert!(gitignore.contains("!.forge/instructions/.gitkeep"));
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
