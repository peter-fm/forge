use forge::cli::Commands;
use forge::config::{build_run_variables, load_forge_config_str};
use std::path::Path;

#[test]
fn loads_dot_forge_config_sections() {
    let config = load_forge_config_str(
        r#"
        [project]
        type = "rust"
        name = "forge"

        [commands]
        test = "cargo test"
        lint = "cargo clippy -- -D warnings"
        build = "cargo build"

        [agent]
        default = "codex"
        model = "gpt-5.4"

        [instructions]
        directory = "instructions"
        gitignore = true
        agents_md = "AGENTS.md"
        "#,
    )
    .expect("config should parse");

    assert_eq!(config.project.project_type.as_deref(), Some("rust"));
    assert_eq!(config.project.name.as_deref(), Some("forge"));
    assert_eq!(config.commands.test.as_deref(), Some("cargo test"));
    assert_eq!(
        config.commands.lint.as_deref(),
        Some("cargo clippy -- -D warnings")
    );
    assert_eq!(config.commands.build.as_deref(), Some("cargo build"));
    assert_eq!(config.defaults.agent.as_deref(), Some("codex"));
    assert_eq!(config.defaults.model.as_deref(), Some("gpt-5.4"));
    assert_eq!(
        config.instructions.directory.as_deref(),
        Some("instructions")
    );
    assert_eq!(config.instructions.gitignore, Some(true));
    assert_eq!(config.instructions.agents_md.as_deref(), Some("AGENTS.md"));
}

#[test]
fn build_run_variables_exposes_command_settings() {
    let config = load_forge_config_str(
        r#"
        [project]
        type = "rust"
        name = "forge"

        [commands]
        test = "cargo test"
        lint = "cargo clippy -- -D warnings"

        [agent]
        default = "codex"
        model = "gpt-5.4"
        "#,
    )
    .expect("config should parse");

    let command = Commands::Run {
        blueprint_name: Some("new-feature".to_string()),
        blueprint: None,
        repo: None,
        task: Some("Add status output".to_string()),
        issue: None,
        round: None,
        pr: None,
        agent: None,
        model: None,
        branch: None,
        dry_run: false,
        notify: Vec::new(),
        verbose: false,
        vars: Vec::new(),
    };

    let variables = build_run_variables(&config, &command, Path::new("/work/forge"), "2026-03-31")
        .expect("variables should resolve");

    assert_eq!(
        variables.get("project_type").map(String::as_str),
        Some("rust")
    );
    assert_eq!(
        variables.get("project_name").map(String::as_str),
        Some("forge")
    );
    assert_eq!(
        variables.get("test_command").map(String::as_str),
        Some("cargo test")
    );
    assert_eq!(
        variables.get("lint_command").map(String::as_str),
        Some("cargo clippy -- -D warnings")
    );
    assert_eq!(
        variables.get("target_agent").map(String::as_str),
        Some("codex")
    );
    assert_eq!(
        variables.get("target_model").map(String::as_str),
        Some("gpt-5.4")
    );
    assert_eq!(
        variables.get("branch").map(String::as_str),
        Some("feat/add-status-output")
    );
}
