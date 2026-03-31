use forge::config::ForgeConfig;
use forge::workspace::{
    archive_instruction_file, build_instruction_file_name, clean_workspace, create_instruction_file,
    list_instruction_files, resolve_instruction_file, CleanOptions, InstructionFile,
};
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn instruction_file_naming_uses_slug_timestamp_and_agent() {
    let file_name = build_instruction_file_name(
        "Add websocket support for API events",
        "2026-03-31T1325",
        "claude-code",
    );

    assert_eq!(
        file_name,
        "add-websocket-support-for-api-events.2026-03-31T1325.claude-code.md"
    );
}

#[test]
fn create_instruction_file_makes_unique_names_for_same_task() {
    let dir = tempdir().expect("tempdir");
    let config = ForgeConfig::default();

    let first =
        create_instruction_file(dir.path(), &config, "Add websocket support", "codex")
            .expect("create first instruction");
    let second =
        create_instruction_file(dir.path(), &config, "Add websocket support", "codex")
            .expect("create second instruction");

    assert_ne!(first.file_name, second.file_name);
    assert!(first.file_name.ends_with(".codex.md"));
    assert!(second.file_name.ends_with(".codex-2.md"));
}

#[test]
fn create_instruction_file_persists_task_text() {
    let dir = tempdir().expect("tempdir");
    let config = ForgeConfig::default();

    let instruction =
        create_instruction_file(dir.path(), &config, "Add websocket support", "codex")
            .expect("create instruction");

    assert!(instruction.path.exists());
    assert_eq!(
        fs::read_to_string(instruction.path).expect("instruction content"),
        "Add websocket support"
    );
}

#[test]
fn resolve_instruction_prefers_workspace_file_for_bare_name() {
    let dir = tempdir().expect("tempdir");
    let config = ForgeConfig::default();
    fs::create_dir_all(dir.path().join(".forge/instructions")).expect("instructions");
    fs::write(
        dir.path().join(".forge/instructions/task.md"),
        "workspace instruction",
    )
    .expect("write workspace instruction");
    fs::write(dir.path().join("task.md"), "root instruction").expect("write root instruction");

    let instruction =
        resolve_instruction_file(dir.path(), &config, "task.md").expect("resolve instruction");

    assert_eq!(
        instruction.path,
        dir.path().join(".forge/instructions/task.md")
    );
    assert_eq!(instruction.path_display, ".forge/instructions/task.md");
}

#[test]
fn archive_moves_instruction_file_into_archive_directory() {
    let dir = tempdir().expect("tempdir");
    let config = ForgeConfig::default();
    fs::create_dir_all(dir.path().join(".forge/instructions")).expect("instructions");
    fs::create_dir_all(dir.path().join(".forge/archive")).expect("archive");
    let path = dir
        .path()
        .join(".forge/instructions/add-websocket-support.2026-03-31T1325.codex.md");
    fs::write(&path, "Implement websocket support").expect("write instruction");

    let archived = archive_instruction_file(
        dir.path(),
        &config,
        &InstructionFile {
            file_name: "add-websocket-support.2026-03-31T1325.codex.md".to_string(),
            path: path.clone(),
            path_display: ".forge/instructions/add-websocket-support.2026-03-31T1325.codex.md"
                .to_string(),
        },
        "done",
    )
    .expect("archive instruction")
    .expect("archived path");

    assert!(!path.exists());
    assert!(archived.exists());
    assert!(
        archived
            .file_name()
            .and_then(|value| value.to_str())
            .expect("archive filename")
            .starts_with("add-websocket-support.2026-03-31T1325.codex.done-")
    );
}

#[test]
fn forge_clean_lists_instruction_files() {
    let dir = tempdir().expect("tempdir");
    fs::create_dir_all(dir.path().join(".forge/instructions")).expect("instructions");
    fs::write(dir.path().join(".forge/instructions/.gitkeep"), "").expect("gitkeep");
    fs::write(dir.path().join(".forge/instructions/one.md"), "one").expect("write first");
    fs::write(dir.path().join(".forge/instructions/two.md"), "two").expect("write second");

    let output = Command::new(env!("CARGO_BIN_EXE_forge"))
        .arg("clean")
        .current_dir(dir.path())
        .output()
        .expect("run forge clean");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(".forge/instructions/one.md"));
    assert!(stdout.contains(".forge/instructions/two.md"));
    assert!(!stdout.contains(".gitkeep"));
}

#[test]
fn forge_clean_archive_moves_all_instruction_files() {
    let dir = tempdir().expect("tempdir");
    fs::create_dir_all(dir.path().join(".forge/instructions")).expect("instructions");
    fs::create_dir_all(dir.path().join(".forge/archive")).expect("archive");
    fs::write(dir.path().join(".forge/instructions/one.md"), "one").expect("write first");
    fs::write(dir.path().join(".forge/instructions/two.md"), "two").expect("write second");

    let output = Command::new(env!("CARGO_BIN_EXE_forge"))
        .args(["clean", "--archive"])
        .current_dir(dir.path())
        .output()
        .expect("run forge clean --archive");

    assert!(output.status.success());
    assert!(list_instruction_files(dir.path(), &ForgeConfig::default())
        .expect("list instruction files")
        .is_empty());
    let archived = archive_file_names(dir.path().join(".forge/archive").as_path());
    assert_eq!(archived.len(), 2);
    assert!(archived.iter().any(|name| name.starts_with("one.archived-")));
    assert!(archived.iter().any(|name| name.starts_with("two.archived-")));
}

#[test]
fn forge_clean_dry_run_does_not_move_files() {
    let dir = tempdir().expect("tempdir");
    let config = ForgeConfig::default();
    fs::create_dir_all(dir.path().join(".forge/instructions")).expect("instructions");
    fs::create_dir_all(dir.path().join(".forge/archive")).expect("archive");
    fs::write(dir.path().join(".forge/instructions/one.md"), "one").expect("write first");

    let report = clean_workspace(
        dir.path(),
        &config,
        &CleanOptions {
            archive: true,
            dry_run: true,
        },
    )
    .expect("clean workspace");

    assert_eq!(report.archived.len(), 1);
    assert!(dir.path().join(".forge/instructions/one.md").exists());
    assert!(archive_file_names(dir.path().join(".forge/archive").as_path()).is_empty());
}

fn archive_file_names(path: &Path) -> Vec<String> {
    fs::read_dir(path)
        .expect("archive entries")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect()
}
