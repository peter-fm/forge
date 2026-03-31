use forge::detect::{ProjectType, detect_project};
use std::fs;
use tempfile::tempdir;

#[test]
fn detects_rust_over_other_indicators() {
    let dir = tempdir().expect("tempdir");
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"forge-test\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("write cargo");
    fs::write(
        dir.path().join("package.json"),
        r#"{"name":"web-app","scripts":{"lint":"eslint .","build":"vite build"}}"#,
    )
    .expect("write package");
    fs::create_dir(dir.path().join("node_modules")).expect("node_modules");

    let detected = detect_project(dir.path(), None).expect("detect project");
    assert_eq!(detected.project_type, ProjectType::Rust);
    assert_eq!(detected.name, "forge-test");
    assert_eq!(detected.commands.test.as_deref(), Some("cargo test"));
}

#[test]
fn detects_bun_scripts_and_ci_hints() {
    let dir = tempdir().expect("tempdir");
    fs::write(
        dir.path().join("package.json"),
        r#"{"name":"ui","scripts":{"check":"biome check .","build:web":"vite build"}}"#,
    )
    .expect("write package");
    fs::write(dir.path().join("bun.lock"), "").expect("write bun lock");
    fs::create_dir_all(dir.path().join(".github/workflows")).expect("workflow dir");
    fs::write(
        dir.path().join(".github/workflows/ci.yml"),
        "jobs:\n  test:\n    steps:\n      - run: bun test\n",
    )
    .expect("write workflow");

    let detected = detect_project(dir.path(), None).expect("detect project");
    assert_eq!(detected.project_type, ProjectType::Bun);
    assert_eq!(detected.commands.test.as_deref(), Some("bun test"));
    assert_eq!(detected.commands.lint.as_deref(), Some("bun run check"));
    assert_eq!(
        detected.commands.build.as_deref(),
        Some("bun run build:web")
    );
}

#[test]
fn detects_make_optional_targets() {
    let dir = tempdir().expect("tempdir");
    fs::write(
        dir.path().join("Makefile"),
        ".PHONY: test build\n\ntest:\n\t@echo test\n\nbuild:\n\t@echo build\n",
    )
    .expect("write makefile");

    let detected = detect_project(dir.path(), None).expect("detect project");
    assert_eq!(detected.project_type, ProjectType::Make);
    assert_eq!(detected.commands.test.as_deref(), Some("make test"));
    assert_eq!(detected.commands.lint, None);
    assert_eq!(detected.commands.build.as_deref(), Some("make build"));
}
