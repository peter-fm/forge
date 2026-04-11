use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use clap::Parser;
use forge::cli::{Cli, Commands};
use forge::commands::init::{InitOptions, init_project};
use forge::condition::evaluate_condition;
use forge::config::{
    auto_branch_name, build_run_variables, load_forge_config_str, resolve_blueprint_for_run,
};
use forge::error::ForgeError;
use forge::logger::RunLogger;
use forge::model::{Blueprint, RunContext, Step, StepResult, StepStatus, StepType};
use forge::notify::{format_run_summary, openclaw_command_args, resolve_backends};
use forge::parser::{parse_blueprint_file, parse_blueprint_str};
use forge::runner::{BlueprintLoader, Engine, ExecutionOutput, Runtime};
use forge::vars::{build_variable_scope, substitute_text};
use tempfile::{TempDir, tempdir};

#[test]
fn parses_generated_refactor_blueprint() {
    let dir = init_generated_project();
    let path = dir.path().join(".forge/blueprints/refactor.toml");
    let blueprint = parse_blueprint_file(path).expect("lint blueprint should parse");

    assert_eq!(blueprint.blueprint.name, "refactor");
    assert_branching_step_sequence(
        &blueprint,
        &[
            "clean-tree",
            "create-branch",
            "refactor",
            "commit-backstop",
            "lint",
            "fix-lint",
            "test",
            "fix-tests",
            "docs-check",
            "docs-commit-backstop",
            "push-branch",
            "write-pr",
            "create-pr",
            "checkout-main",
        ],
    );
}

#[test]
fn parses_generated_new_feature_blueprint() {
    let dir = init_generated_project();
    let path = dir.path().join(".forge/blueprints/new-feature.toml");
    let blueprint = parse_blueprint_file(path).expect("new-feature should parse");

    assert_branching_step_sequence(
        &blueprint,
        &[
            "clean-tree",
            "create-branch",
            "implement",
            "commit-backstop",
            "lint",
            "fix-lint",
            "test",
            "fix-tests",
            "docs-check",
            "docs-commit-backstop",
            "push-branch",
            "write-pr",
            "create-pr",
            "checkout-main",
        ],
    );
    assert!(
        blueprint
            .steps
            .iter()
            .any(|step| step.max_retries == Some(2))
    );
    assert!(
        blueprint.steps.iter().any(|step| step.name == "fix-lint"
            && step.condition.as_deref() == Some("lint.exit_code != 0"))
    );
    assert!(blueprint.steps.iter().any(|step| step.name == "docs-check"));
    assert!(
        blueprint
            .steps
            .iter()
            .any(|step| step.name == "docs-check" && step.allow_failure)
    );
    assert!(
        !blueprint.steps[2]
            .prompt
            .as_deref()
            .unwrap_or_default()
            .contains("Commit your changes")
    );
}

#[test]
fn generated_fix_bug_blueprint_uses_deterministic_branching_skeleton() {
    let dir = init_generated_project();
    let path = dir.path().join(".forge/blueprints/fix-bug.toml");
    let blueprint = parse_blueprint_file(path).expect("fix-bug should parse");

    assert_branching_step_sequence(
        &blueprint,
        &[
            "clean-tree",
            "create-branch",
            "fix",
            "commit-backstop",
            "lint",
            "fix-lint",
            "test",
            "fix-tests",
            "docs-check",
            "docs-commit-backstop",
            "push-branch",
            "write-pr",
            "create-pr",
            "checkout-main",
        ],
    );
    assert!(
        !blueprint.steps[2]
            .prompt
            .as_deref()
            .unwrap_or_default()
            .contains("Commit your changes")
    );
}

#[test]
fn parses_generated_code_review_blueprint() {
    let dir = init_generated_project();
    let path = dir.path().join(".forge/blueprints/code-review.toml");
    let blueprint = parse_blueprint_file(path).expect("code-review should parse");

    assert_eq!(blueprint.blueprint.name, "code-review");
    assert_branching_step_sequence(&blueprint, &["checkout-pr", "review"]);
}

#[test]
fn parses_generated_refactor_phase_blueprint() {
    let dir = init_generated_project();
    let path = dir.path().join(".forge/blueprints/refactor-phase.toml");
    let blueprint = parse_blueprint_file(path).expect("refactor-phase should parse");

    assert_eq!(blueprint.blueprint.name, "refactor-phase");
    assert_branching_step_sequence(
        &blueprint,
        &[
            "checkout-or-create-branch",
            "implement-phase",
            "commit-backstop",
            "lint",
            "fix-lint",
            "test",
            "fix-tests",
        ],
    );
    assert!(
        blueprint.steps.iter().any(|step| step.name == "fix-lint"
            && step.condition.as_deref() == Some("lint.exit_code != 0"))
    );
    assert!(
        blueprint.steps.iter().any(|step| step.name == "fix-tests"
            && step.condition.as_deref() == Some("test.exit_code != 0"))
    );
}

#[test]
fn parses_generated_refactor_finalize_blueprint() {
    let dir = init_generated_project();
    let path = dir.path().join(".forge/blueprints/refactor-finalize.toml");
    let blueprint = parse_blueprint_file(path).expect("refactor-finalize should parse");

    assert_eq!(blueprint.blueprint.name, "refactor-finalize");
    assert_branching_step_sequence(
        &blueprint,
        &[
            "checkout-branch",
            "final-lint",
            "final-test",
            "docs-check",
            "docs-commit-backstop",
            "push-branch",
            "write-pr",
            "create-pr",
            "checkout-main",
        ],
    );
}

#[test]
fn parses_blueprint_params() {
    let blueprint = parse_blueprint_str(
        r#"
        [blueprint]
        name = "root"
        description = "desc"

        [[step]]
        type = "blueprint"
        name = "nested"
        blueprint = "common/lint"
        params = { repo = "warrant-core", repo_path = "{core_path}" }
        "#,
    )
    .expect("blueprint should parse");

    let params = &blueprint.steps[0].params;
    assert_eq!(params.get("repo").map(String::as_str), Some("warrant-core"));
    assert_eq!(
        params.get("repo_path").map(String::as_str),
        Some("{core_path}")
    );
}

#[test]
fn rejects_invalid_toml_cleanly() {
    let error = parse_blueprint_str("[blueprint").expect_err("invalid TOML should fail");
    assert!(error.to_string().contains("TOML"));
}

#[test]
fn rejects_missing_required_fields_cleanly() {
    let error = parse_blueprint_str(
        r#"
        [blueprint]
        name = "bad"
        description = "missing"

        [[step]]
        type = "deterministic"
        "#,
    )
    .expect_err("missing name/command should fail");

    assert!(error.to_string().contains("missing"));
}

#[test]
fn substitutes_simple_variables() {
    let variables = BTreeMap::from([("repo_path".to_string(), "/home/pete/core".to_string())]);
    let output =
        substitute_text("cd {repo_path} && cargo check", &variables).expect("substitution");
    assert_eq!(output, "cd /home/pete/core && cargo check");
}

#[test]
fn substitutes_nested_params_into_sub_blueprint_commands() {
    let mut variables = BTreeMap::new();
    variables.insert("repo_path".to_string(), "/tmp/core".to_string());
    variables.insert("core_path".to_string(), "/tmp/core".to_string());
    let output = substitute_text("cd {repo_path} && cargo test -- {core_path}", &variables)
        .expect("substitution");
    assert_eq!(output, "cd /tmp/core && cargo test -- /tmp/core");
}

#[test]
fn rejects_missing_variables() {
    let error = substitute_text("cd {missing}", &BTreeMap::new()).expect_err("should fail");
    assert!(error.to_string().contains("missing"));
}

#[test]
fn exposes_step_output_variables() {
    let mut context = RunContext::new();
    context.step_results.insert(
        "step-0001".to_string(),
        StepResult {
            step_id: "step-0001".to_string(),
            name: "lint".to_string(),
            step_type: StepType::Deterministic,
            status: StepStatus::Succeeded,
            exit_code: 0,
            stdout: "clippy clean".to_string(),
            stderr: String::new(),
            attempts: 1,
            agent_session_id: None,
            log_file: None,
        },
    );

    let variables = build_variable_scope(&context);
    assert_eq!(
        variables.get("lint_output").map(String::as_str),
        Some("clippy clean")
    );
    assert_eq!(
        variables.get("lint.exit_code").map(String::as_str),
        Some("0")
    );
}

#[test]
fn evaluates_boolean_conditions() {
    let variables = BTreeMap::from([("core_changed".to_string(), "true".to_string())]);
    assert!(evaluate_condition("core_changed == true", &variables).expect("condition"));
}

#[test]
fn evaluates_false_boolean_conditions() {
    let variables = BTreeMap::from([("core_changed".to_string(), "false".to_string())]);
    assert!(!evaluate_condition("core_changed == true", &variables).expect("condition"));
}

#[test]
fn evaluates_exit_code_conditions() {
    let variables = BTreeMap::from([("lint.exit_code".to_string(), "1".to_string())]);
    assert!(evaluate_condition("lint.exit_code != 0", &variables).expect("condition"));
}

#[test]
fn evaluates_or_conditions() {
    let variables = BTreeMap::from([
        ("a".to_string(), "0".to_string()),
        ("b".to_string(), "1".to_string()),
    ]);
    assert!(evaluate_condition("a != 0 || b != 0", &variables).expect("condition"));
}

#[test]
fn evaluates_string_conditions() {
    let variables = BTreeMap::from([("target_repo".to_string(), "warrant-shell".to_string())]);
    assert!(evaluate_condition("target_repo != 'registry'", &variables).expect("condition"));
}

#[test]
fn rejects_undefined_condition_variables() {
    let error = evaluate_condition("missing == true", &BTreeMap::new()).expect_err("should fail");
    assert!(error.to_string().contains("missing"));
}

#[test]
fn runs_steps_in_order() {
    let blueprint = blueprint_with_steps(vec![
        deterministic_step("first", "echo first"),
        deterministic_step("second", "echo second"),
    ]);
    let runtime = MockRuntime::default();
    runtime.push_command(0, "one", "");
    runtime.push_command(0, "two", "");

    let mut engine = test_engine(
        runtime.clone(),
        MockLoader::default(),
        MemoryLogger::default(),
    );
    let summary = engine
        .run_blueprint(&blueprint, &mut RunContext::new())
        .expect("run should pass");

    assert_eq!(
        runtime.command_names(),
        vec!["first".to_string(), "second".to_string()]
    );
    assert_eq!(summary.steps.len(), 2);
}

#[test]
fn skips_conditional_step_when_condition_is_false() {
    let mut conditional = deterministic_step("second", "echo second");
    conditional.condition = Some("run_second == true".to_string());
    let blueprint =
        blueprint_with_steps(vec![deterministic_step("first", "echo first"), conditional]);

    let runtime = MockRuntime::default();
    runtime.push_command(0, "one", "");
    let mut context = RunContext::new();
    context
        .variables
        .insert("run_second".to_string(), "false".to_string());

    let mut engine = test_engine(
        runtime.clone(),
        MockLoader::default(),
        MemoryLogger::default(),
    );
    let summary = engine
        .run_blueprint(&blueprint, &mut context)
        .expect("run should pass");

    assert_eq!(runtime.command_names(), vec!["first".to_string()]);
    assert_eq!(summary.steps[1].status, StepStatus::Skipped);
}

#[test]
fn gate_failure_aborts_remaining_steps() {
    let mut gate = deterministic_step("gate", "false");
    gate.step_type = StepType::Gate;
    let blueprint = blueprint_with_steps(vec![gate, deterministic_step("third", "echo third")]);

    let runtime = MockRuntime::default();
    runtime.push_command(1, "", "boom");
    let mut engine = test_engine(
        runtime.clone(),
        MockLoader::default(),
        MemoryLogger::default(),
    );

    let error = engine
        .run_blueprint(&blueprint, &mut RunContext::new())
        .expect_err("gate should abort");
    assert!(error.to_string().contains("gate"));
    assert_eq!(runtime.command_names(), vec!["gate".to_string()]);
}

#[test]
fn allow_failure_does_not_abort() {
    let mut first = deterministic_step("first", "false");
    first.allow_failure = true;
    let blueprint = blueprint_with_steps(vec![first, deterministic_step("second", "echo second")]);

    let runtime = MockRuntime::default();
    runtime.push_command(1, "", "boom");
    runtime.push_command(0, "ok", "");
    let mut engine = test_engine(
        runtime.clone(),
        MockLoader::default(),
        MemoryLogger::default(),
    );

    let summary = engine
        .run_blueprint(&blueprint, &mut RunContext::new())
        .expect("run should continue");
    assert_eq!(summary.steps.len(), 2);
    assert_eq!(
        runtime.command_names(),
        vec!["first".to_string(), "second".to_string()]
    );
}

#[test]
fn dry_run_populates_sets_variables() {
    let mut step = deterministic_step("check-core-changed", "cd /tmp/core && true");
    step.sets = Some("core_changed".to_string());
    step.allow_failure = true;
    let blueprint = blueprint_with_steps(vec![step]);

    let runtime = MockRuntime::default();
    let mut context = RunContext::new();
    context.dry_run = true;
    let mut engine = test_engine(runtime, MockLoader::default(), MemoryLogger::default());

    engine
        .run_blueprint(&blueprint, &mut context)
        .expect("dry run should pass");
    assert_eq!(
        context.variables.get("core_changed").map(String::as_str),
        Some("true")
    );
}

#[test]
fn sub_blueprint_records_parent_step_result() {
    let child = blueprint_with_steps(vec![
        deterministic_step("format", "cargo fmt"),
        deterministic_step("lint", "cargo clippy"),
    ]);
    let loader =
        MockLoader::default().with_blueprint(PathBuf::from("blueprints/common/lint.toml"), child);
    let runtime = MockRuntime::default();
    runtime.push_command(0, "fmt ok", "");
    runtime.push_command(0, "lint ok", "");
    let mut engine = test_engine(runtime, loader, MemoryLogger::default());
    let mut context = RunContext::new();

    engine
        .run_blueprint(
            &blueprint_with_steps(vec![blueprint_step("lint-core", "common/lint")]),
            &mut context,
        )
        .expect("sub-blueprint should pass");

    let parent = context
        .step_results
        .values()
        .find(|result| result.name == "lint-core")
        .expect("parent result should be recorded");
    assert_eq!(parent.status, StepStatus::Succeeded);
    assert_eq!(parent.exit_code, 0);
}

#[test]
fn sub_blueprint_parent_result_reflects_child_failure() {
    let child = blueprint_with_steps(vec![
        deterministic_step("format", "cargo fmt"),
        deterministic_step("lint", "cargo clippy"),
    ]);
    let loader =
        MockLoader::default().with_blueprint(PathBuf::from("blueprints/common/lint.toml"), child);
    let runtime = MockRuntime::default();
    runtime.push_command(0, "fmt ok", "");
    runtime.push_command(7, "", "lint failed");
    let mut engine = test_engine(runtime, loader, MemoryLogger::default());
    let mut context = RunContext::new();

    let error = engine.run_blueprint(
        &blueprint_with_steps(vec![blueprint_step("lint-core", "common/lint")]),
        &mut context,
    );
    assert!(
        error.is_err(),
        "parent blueprint should fail when child step fails"
    );

    let parent = context
        .step_results
        .values()
        .find(|result| result.name == "lint-core")
        .expect("parent result should be recorded");
    assert_eq!(parent.status, StepStatus::Failed);
    assert_eq!(parent.exit_code, 7);
}

#[test]
fn expect_failure_treats_non_zero_as_success() {
    let mut first = deterministic_step("first", "false");
    first.expect_failure = true;
    let blueprint = blueprint_with_steps(vec![first]);

    let runtime = MockRuntime::default();
    runtime.push_command(1, "", "expected");
    let mut engine = test_engine(runtime, MockLoader::default(), MemoryLogger::default());

    let summary = engine
        .run_blueprint(&blueprint, &mut RunContext::new())
        .expect("run should pass");
    assert_eq!(summary.steps[0].status, StepStatus::Succeeded);
}

#[test]
fn resolves_common_sub_blueprint_paths() {
    let loader = MockLoader::default().with_blueprint(
        PathBuf::from("blueprints/common/lint.toml"),
        blueprint_with_steps(vec![deterministic_step(
            "format",
            "cd {repo_path} && cargo fmt",
        )]),
    );
    let parent = blueprint_with_steps(vec![blueprint_step("lint-core", "common/lint")]);
    let runtime = MockRuntime::default();
    runtime.push_command(0, "", "");
    let mut context = RunContext::new();
    context
        .variables
        .insert("repo_path".to_string(), "/tmp/core".to_string());
    let mut engine = test_engine(runtime.clone(), loader, MemoryLogger::default());

    engine
        .run_blueprint(&parent, &mut context)
        .expect("run should pass");
    assert_eq!(
        runtime.commands(),
        vec!["cd /tmp/core && cargo fmt".to_string()]
    );
}

#[test]
fn resolves_product_sub_blueprint_paths() {
    let loader = MockLoader::default().with_blueprint(
        PathBuf::from("blueprints/warrant-shell/test-chain.toml"),
        blueprint_with_steps(vec![deterministic_step("unit", "cargo test")]),
    );
    let parent = blueprint_with_steps(vec![blueprint_step("chain", "warrant-shell/test-chain")]);
    let runtime = MockRuntime::default();
    runtime.push_command(0, "", "");
    let mut engine = test_engine(runtime, loader, MemoryLogger::default());

    engine
        .run_blueprint(&parent, &mut RunContext::new())
        .expect("run should pass");
}

#[test]
fn applies_params_to_sub_blueprints() {
    let child = blueprint_with_steps(vec![deterministic_step(
        "format",
        "cd {repo_path} && cargo fmt",
    )]);
    let loader =
        MockLoader::default().with_blueprint(PathBuf::from("blueprints/common/lint.toml"), child);
    let mut step = blueprint_step("lint-core", "common/lint");
    step.params
        .insert("repo_path".to_string(), "/tmp/core".to_string());
    let runtime = MockRuntime::default();
    runtime.push_command(0, "", "");
    let mut engine = test_engine(runtime.clone(), loader, MemoryLogger::default());

    engine
        .run_blueprint(&blueprint_with_steps(vec![step]), &mut RunContext::new())
        .expect("run should pass");
    assert_eq!(
        runtime.commands(),
        vec!["cd /tmp/core && cargo fmt".to_string()]
    );
}

#[test]
fn errors_on_missing_blueprint_file() {
    let parent = blueprint_with_steps(vec![blueprint_step("lint-core", "common/lint")]);
    let mut engine = test_engine(
        MockRuntime::default(),
        MockLoader::default(),
        MemoryLogger::default(),
    );

    let error = engine
        .run_blueprint(&parent, &mut RunContext::new())
        .expect_err("missing blueprint should fail");
    assert!(error.to_string().contains("common/lint"));
}

#[test]
fn detects_circular_blueprint_references() {
    let mut first = blueprint_step("loop", "common/lint");
    first
        .params
        .insert("repo_path".to_string(), "/tmp/core".to_string());
    let child = blueprint_with_steps(vec![blueprint_step("again", "common/lint")]);
    let loader =
        MockLoader::default().with_blueprint(PathBuf::from("blueprints/common/lint.toml"), child);
    let parent = blueprint_with_steps(vec![first]);
    let mut engine = test_engine(MockRuntime::default(), loader, MemoryLogger::default());

    let error = engine
        .run_blueprint(&parent, &mut RunContext::new())
        .expect_err("cycle should fail");
    assert!(error.to_string().contains("circular"));
}

#[test]
fn retries_agentic_steps_until_tests_pass() {
    let mut fix = agentic_step("fix-tests", "repair using {test_output}");
    fix.max_retries = Some(3);
    let test = deterministic_step("test-chain", "cargo test");
    let blueprint = blueprint_with_steps(vec![fix, test]);

    let runtime = MockRuntime::default();
    runtime.push_agent(0, "attempt-1", "");
    runtime.push_command(1, "failing tests", "");
    runtime.push_agent(0, "attempt-2", "");
    runtime.push_command(0, "green", "");

    let mut engine = test_engine(
        runtime.clone(),
        MockLoader::default(),
        MemoryLogger::default(),
    );
    let summary = engine
        .run_blueprint(&blueprint, &mut RunContext::new())
        .expect("retry should recover");

    assert_eq!(
        runtime.agent_prompts(),
        vec![
            "repair using ".to_string(),
            "repair using failing tests".to_string(),
        ]
    );
    assert_eq!(
        summary
            .steps
            .iter()
            .filter(|step| step.name == "fix-tests")
            .count(),
        1
    );
}

#[test]
fn usage_limit_failure_does_not_retry_agent_step() {
    let mut fix = agentic_step("fix-tests", "repair");
    fix.max_retries = Some(3);
    let blueprint = blueprint_with_steps(vec![fix]);

    let runtime = MockRuntime::default();
    runtime.push_agent(
        1,
        "{\"type\":\"error\",\"message\":\"You've hit your usage limit.\"}",
        "",
    );

    let mut engine = test_engine(
        runtime.clone(),
        MockLoader::default(),
        MemoryLogger::default(),
    );
    let error = engine
        .run_blueprint(&blueprint, &mut RunContext::new())
        .expect_err("usage limit should fail immediately");

    assert!(error.to_string().contains("fix-tests"));
    assert_eq!(runtime.agent_names(), vec!["fix-tests".to_string()]);
}

#[test]
fn usage_limit_failure_skips_retry_target_check() {
    let mut fix = agentic_step("fix-tests", "repair using {test_output}");
    fix.max_retries = Some(3);
    let blueprint = blueprint_with_steps(vec![fix, deterministic_step("test-chain", "cargo test")]);

    let runtime = MockRuntime::default();
    runtime.push_agent(
        1,
        "",
        "Error: 429 {\"type\":\"error\",\"error\":{\"type\":\"rate_limit_error\",\"message\":\"This request would exceed your account's rate limit. Please try again later.\"}}",
    );

    let mut engine = test_engine(
        runtime.clone(),
        MockLoader::default(),
        MemoryLogger::default(),
    );
    let error = engine
        .run_blueprint(&blueprint, &mut RunContext::new())
        .expect_err("rate limit should short-circuit retries");

    assert!(error.to_string().contains("fix-tests"));
    assert_eq!(runtime.agent_names(), vec!["fix-tests".to_string()]);
    assert!(runtime.command_names().is_empty());
    assert_eq!(engine.logger.entries.len(), 1);
    assert_eq!(engine.logger.entries[0].name, "fix-tests");
    assert_eq!(engine.logger.entries[0].status, StepStatus::Failed);
}

#[test]
fn conditional_retry_step_retries_failed_previous_step() {
    let mut lint = deterministic_step("lint", "cargo clippy");
    lint.allow_failure = true;
    let mut fix = agentic_step("fix-lint", "repair using {lint_output}");
    fix.max_retries = Some(2);
    fix.condition = Some("lint.exit_code != 0".to_string());
    let test = deterministic_step("test", "cargo test");
    let blueprint = blueprint_with_steps(vec![lint, fix, test]);

    let runtime = MockRuntime::default();
    runtime.push_command(1, "lint-1", "");
    runtime.push_agent(0, "attempt-1", "");
    runtime.push_command(0, "lint-2", "");
    runtime.push_command(0, "tests-ok", "");

    let mut engine = test_engine(
        runtime.clone(),
        MockLoader::default(),
        MemoryLogger::default(),
    );
    let summary = engine
        .run_blueprint(&blueprint, &mut RunContext::new())
        .expect("retry should recover");

    assert_eq!(
        runtime.command_names(),
        vec!["lint".to_string(), "lint".to_string(), "test".to_string()]
    );
    assert_eq!(
        runtime.agent_prompts(),
        vec!["repair using lint-1".to_string()]
    );
    assert_eq!(
        summary
            .steps
            .iter()
            .map(|step| step.name.as_str())
            .collect::<Vec<_>>(),
        vec!["lint", "fix-lint", "lint", "test"]
    );
}

#[test]
fn stops_retrying_after_successful_second_attempt() {
    let mut fix = agentic_step("fix-tests", "repair");
    fix.max_retries = Some(3);
    let blueprint = blueprint_with_steps(vec![fix, deterministic_step("test-chain", "cargo test")]);

    let runtime = MockRuntime::default();
    runtime.push_agent(0, "", "");
    runtime.push_command(1, "red", "");
    runtime.push_agent(0, "", "");
    runtime.push_command(0, "green", "");

    let mut engine = test_engine(
        runtime.clone(),
        MockLoader::default(),
        MemoryLogger::default(),
    );
    engine
        .run_blueprint(&blueprint, &mut RunContext::new())
        .expect("retry should stop early");
    assert_eq!(runtime.agent_names().len(), 2);
}

#[test]
fn aborts_after_exhausting_retries() {
    let mut fix = agentic_step("fix-tests", "repair using {test_output}");
    fix.max_retries = Some(3);
    let blueprint = blueprint_with_steps(vec![fix, deterministic_step("test-chain", "cargo test")]);

    let runtime = MockRuntime::default();
    runtime.push_agent(0, "", "");
    runtime.push_command(1, "fail-1", "");
    runtime.push_agent(0, "", "");
    runtime.push_command(1, "fail-2", "");
    runtime.push_agent(0, "", "");
    runtime.push_command(1, "fail-3", "");

    let mut engine = test_engine(runtime, MockLoader::default(), MemoryLogger::default());
    let error = engine
        .run_blueprint(&blueprint, &mut RunContext::new())
        .expect_err("retries should exhaust");
    assert!(error.to_string().contains("fix-tests"));
    assert_eq!(engine.logger.entries.len(), 2);
    assert_eq!(engine.logger.entries[0].name, "fix-tests");
    assert_eq!(engine.logger.entries[0].status, StepStatus::Succeeded);
    assert_eq!(engine.logger.entries[0].attempts, 3);
    assert_eq!(engine.logger.entries[1].name, "test-chain");
    assert_eq!(engine.logger.entries[1].status, StepStatus::Failed);
}

#[test]
fn dry_run_generated_pipeline() {
    let dir = init_generated_project();
    let path = dir.path().join(".forge/blueprints/new-feature.toml");
    let blueprint = parse_blueprint_file(path).expect("new-feature should parse");
    let mut context = RunContext::new();
    context.dry_run = true;
    context.variables = BTreeMap::from([
        ("task".to_string(), "Add a hello subcommand".to_string()),
        (
            "task_description".to_string(),
            "Add a hello subcommand".to_string(),
        ),
        (
            "commit_message".to_string(),
            "Add a hello subcommand".to_string(),
        ),
        ("branch".to_string(), "feat/hello".to_string()),
        ("target_repo".to_string(), "warrant-shell".to_string()),
        ("target_repo_path".to_string(), "/tmp/shell".to_string()),
        ("target_agent".to_string(), "codex".to_string()),
        ("target_model".to_string(), "gpt-5.4".to_string()),
        ("default_branch".to_string(), "main".to_string()),
        ("run_id".to_string(), "new-feature-a3f2".to_string()),
        (
            "instruction_file".to_string(),
            "add-a-hello-subcommand.2026-03-31T1325.codex.md".to_string(),
        ),
        (
            "instruction_path".to_string(),
            ".forge/instructions/add-a-hello-subcommand.2026-03-31T1325.codex.md".to_string(),
        ),
    ]);
    let mut engine = Engine {
        loader: FilesystemLoader,
        runtime: MockRuntime::default(),
        logger: MemoryLogger::default(),
        blueprint_root: dir.path().join(".forge/blueprints"),
        dashboard: None,
    };

    engine
        .run_blueprint(&blueprint, &mut context)
        .expect("dry run should complete");
}

#[test]
fn repo_aliases_include_short_path_names() {
    let config = load_forge_config_str(
        r#"
        [repos.warrant-core]
        path = "/repos/warrant-core"

        [repos.website]
        path = "/repos/website"
        "#,
    )
    .expect("config should parse");
    let command = Commands::Run {
        blueprint_name: Some("lint".to_string()),
        blueprint: None,
        repo: Some("warrant-core".to_string()),
        task: None,
        instruction: None,
        issue: None,
        round: None,
        pr: None,
        next: false,
        latest: false,
        agent: None,
        model: None,
        branch: None,
        dry_run: false,
        no_dashboard: false,
        port: 8400,
        notify: Vec::new(),
        verbose: false,
        vars: Vec::new(),
    };

    let variables = build_run_variables(&config, &command, Path::new("/forge"), "2026-03-22")
        .expect("variables should resolve");
    assert_eq!(
        variables.get("core_path").map(String::as_str),
        Some("/repos/warrant-core")
    );
    assert_eq!(
        variables.get("website_path").map(String::as_str),
        Some("/repos/website")
    );
}

#[test]
fn resolve_backends_openclaw() {
    let backends = resolve_backends(&["openclaw".to_string()]).expect("openclaw should resolve");
    assert_eq!(backends.len(), 1);
}

#[test]
fn resolve_backends_unknown_errors() {
    let error =
        resolve_backends(&["telegram".to_string()]).expect_err("unknown backend should fail");
    assert!(error.to_string().contains("telegram"));
}

#[test]
fn resolve_backends_multiple() {
    let backends = resolve_backends(&["openclaw".to_string(), "openclaw".to_string()])
        .expect("multiple backends should resolve");
    assert_eq!(backends.len(), 2);
}

#[test]
fn summary_format_success() {
    let summary = forge::model::RunSummary {
        steps: vec![
            step_result("lint", StepStatus::Succeeded),
            step_result("test", StepStatus::Succeeded),
        ],
    };

    assert_eq!(
        format_run_summary("lint-and-test", &summary),
        "✅ lint-and-test completed: 2/2 steps passed"
    );
}

#[test]
fn summary_format_failure() {
    let summary = forge::model::RunSummary {
        steps: vec![
            step_result("lint", StepStatus::Succeeded),
            step_result("test", StepStatus::Failed),
            step_result("deploy", StepStatus::Pending),
        ],
    };

    assert_eq!(
        format_run_summary("release", &summary),
        "❌ release failed at step 'test': 1/3 steps completed"
    );
}

#[test]
fn cli_parses_notify_flag() {
    let cli = Cli::try_parse_from(["forge", "run", "new-feature", "--notify", "openclaw,slack"])
        .expect("cli should accept --notify");

    match cli.command {
        Commands::Run { notify, .. } => {
            assert_eq!(notify, vec!["openclaw".to_string(), "slack".to_string()]);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn cli_parses_pr_selection_flags() {
    let cli = Cli::try_parse_from(["forge", "run", "pr-review", "--next"])
        .expect("cli should accept --next");
    match cli.command {
        Commands::Run {
            blueprint_name,
            pr,
            next,
            latest,
            ..
        } => {
            assert_eq!(blueprint_name.as_deref(), Some("pr-review"));
            assert_eq!(pr, None);
            assert!(next);
            assert!(!latest);
        }
        other => panic!("unexpected command: {other:?}"),
    }

    let cli = Cli::try_parse_from(["forge", "run", "pr-review", "--latest"])
        .expect("cli should accept --latest");
    match cli.command {
        Commands::Run { next, latest, .. } => {
            assert!(!next);
            assert!(latest);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn openclaw_command_string_is_correct() {
    let args = openclaw_command_args("done");
    assert_eq!(
        args,
        vec!["system", "event", "--text", "done", "--mode", "now"]
    );
}

fn step_result(name: &str, status: StepStatus) -> StepResult {
    StepResult {
        step_id: format!("id-{name}"),
        name: name.to_string(),
        step_type: StepType::Deterministic,
        status,
        exit_code: 0,
        stdout: String::new(),
        stderr: String::new(),
        attempts: 1,
        agent_session_id: None,
        log_file: None,
    }
}

fn assert_branching_step_sequence(blueprint: &Blueprint, expected: &[&str]) {
    let actual = blueprint
        .steps
        .iter()
        .map(|step| step.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

fn blueprint_with_steps(steps: Vec<Step>) -> Blueprint {
    Blueprint {
        blueprint: forge::model::BlueprintMeta {
            name: "test".to_string(),
            description: "test blueprint".to_string(),
            repos: Vec::new(),
        },
        steps,
        source_path: None,
    }
}

fn deterministic_step(name: &str, command: &str) -> Step {
    Step {
        step_type: StepType::Deterministic,
        name: name.to_string(),
        command: Some(command.to_string()),
        agent: None,
        model: None,
        prompt: None,
        blueprint: None,
        params: BTreeMap::new(),
        condition: None,
        sets: None,
        allow_failure: false,
        max_retries: None,
        expect_failure: false,
        env: BTreeMap::new(),
    }
}

fn blueprint_step(name: &str, blueprint: &str) -> Step {
    Step {
        step_type: StepType::Blueprint,
        name: name.to_string(),
        command: None,
        agent: None,
        model: None,
        prompt: None,
        blueprint: Some(blueprint.to_string()),
        params: BTreeMap::new(),
        condition: None,
        sets: None,
        allow_failure: false,
        max_retries: None,
        expect_failure: false,
        env: BTreeMap::new(),
    }
}

fn agentic_step(name: &str, prompt: &str) -> Step {
    Step {
        step_type: StepType::Agentic,
        name: name.to_string(),
        command: None,
        agent: Some("codex".to_string()),
        model: Some("gpt-5.4".to_string()),
        prompt: Some(prompt.to_string()),
        blueprint: None,
        params: BTreeMap::new(),
        condition: None,
        sets: None,
        allow_failure: false,
        max_retries: None,
        expect_failure: false,
        env: BTreeMap::new(),
    }
}

fn test_engine(
    runtime: MockRuntime,
    loader: MockLoader,
    logger: MemoryLogger,
) -> Engine<MockLoader, MockRuntime, MemoryLogger> {
    Engine {
        loader,
        runtime,
        logger,
        blueprint_root: PathBuf::from("blueprints"),
        dashboard: None,
    }
}

#[derive(Clone, Default)]
struct MockRuntime {
    state: Rc<RefCell<MockRuntimeState>>,
}

#[derive(Default)]
struct MockRuntimeState {
    command_outputs: VecDeque<ExecutionOutput>,
    agent_outputs: VecDeque<ExecutionOutput>,
    command_names: Vec<String>,
    commands: Vec<String>,
    agent_names: Vec<String>,
    agent_prompts: Vec<String>,
}

impl MockRuntime {
    fn push_command(&self, exit_code: i32, stdout: &str, stderr: &str) {
        self.state
            .borrow_mut()
            .command_outputs
            .push_back(ExecutionOutput {
                exit_code,
                stdout: stdout.to_string(),
                stderr: stderr.to_string(),
            });
    }

    fn push_agent(&self, exit_code: i32, stdout: &str, stderr: &str) {
        self.state
            .borrow_mut()
            .agent_outputs
            .push_back(ExecutionOutput {
                exit_code,
                stdout: stdout.to_string(),
                stderr: stderr.to_string(),
            });
    }

    fn command_names(&self) -> Vec<String> {
        self.state.borrow().command_names.clone()
    }

    fn commands(&self) -> Vec<String> {
        self.state.borrow().commands.clone()
    }

    fn agent_names(&self) -> Vec<String> {
        self.state.borrow().agent_names.clone()
    }

    fn agent_prompts(&self) -> Vec<String> {
        self.state.borrow().agent_prompts.clone()
    }
}

impl Runtime for MockRuntime {
    fn run_command(
        &mut self,
        step_name: &str,
        command: &str,
        _env: &BTreeMap<String, String>,
        _log_path: Option<&Path>,
    ) -> Result<ExecutionOutput, ForgeError> {
        let mut state = self.state.borrow_mut();
        state.command_names.push(step_name.to_string());
        state.commands.push(command.to_string());
        state
            .command_outputs
            .pop_front()
            .ok_or_else(|| ForgeError::message(format!("missing command output for {step_name}")))
    }

    fn run_agent(
        &mut self,
        step_name: &str,
        _agent: &str,
        _model: &str,
        prompt: &str,
        _env: &BTreeMap<String, String>,
        _log_path: Option<&Path>,
    ) -> Result<ExecutionOutput, ForgeError> {
        let mut state = self.state.borrow_mut();
        state.agent_names.push(step_name.to_string());
        state.agent_prompts.push(prompt.to_string());
        state
            .agent_outputs
            .pop_front()
            .ok_or_else(|| ForgeError::message(format!("missing agent output for {step_name}")))
    }
}

#[derive(Clone, Default)]
struct MockLoader {
    blueprints: Rc<RefCell<BTreeMap<PathBuf, Blueprint>>>,
}

impl MockLoader {
    fn with_blueprint(self, path: PathBuf, blueprint: Blueprint) -> Self {
        self.blueprints.borrow_mut().insert(path, blueprint);
        self
    }
}

impl BlueprintLoader for MockLoader {
    fn load(&self, path: &Path) -> Result<Blueprint, ForgeError> {
        self.blueprints
            .borrow()
            .get(path)
            .cloned()
            .ok_or_else(|| ForgeError::message(format!("missing blueprint: {}", path.display())))
    }
}

#[derive(Clone, Copy, Default)]
struct FilesystemLoader;

impl BlueprintLoader for FilesystemLoader {
    fn load(&self, path: &Path) -> Result<Blueprint, ForgeError> {
        parse_blueprint_file(path)
    }
}

#[derive(Default)]
struct MemoryLogger {
    entries: Vec<StepResult>,
}

impl RunLogger for MemoryLogger {
    fn log_run_start(&mut self, _meta: &forge::logger::RunMeta) -> Result<(), ForgeError> {
        Ok(())
    }

    fn log_step(&mut self, step: &StepResult) -> Result<(), ForgeError> {
        self.entries.push(step.clone());
        Ok(())
    }

    fn log_run_end(&mut self, _result: &forge::logger::RunEnd) -> Result<(), ForgeError> {
        Ok(())
    }
}

#[test]
fn smoke_tempdir_available_for_future_logger_tests() {
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("smoke.txt");
    fs::write(&file, "ok").expect("write");
    assert_eq!(fs::read_to_string(file).expect("read"), "ok");
}

#[test]
fn parses_forge_config_and_expands_repo_paths() {
    let home = std::env::var("HOME").expect("HOME should exist");
    let config = load_forge_config_str(
        r#"
        [defaults]
        agent = "codex"
        model = "gpt-5.4"

        [repos.warrant-core]
        path = "~/warrant-core"
        crate = "warrant-core"
        test_blueprint = "warrant-shell/test-chain"

        [repos.registry]
        path = "/srv/registry"
        agent = "claude-code"

        [[routing]]
        pattern = "src/**/*.rs"
        preferred_agent = "codex"
        reason = "Rust"
        "#,
    )
    .expect("config should parse");

    assert_eq!(config.defaults.agent.as_deref(), Some("codex"));
    assert_eq!(config.defaults.model.as_deref(), Some("gpt-5.4"));
    assert_eq!(
        config.repos["warrant-core"].path,
        format!("{home}/warrant-core")
    );
    assert_eq!(
        config.repos["warrant-core"].crate_name.as_deref(),
        Some("warrant-core")
    );
    assert_eq!(
        config.repos["warrant-core"].test_blueprint.as_deref(),
        Some("warrant-shell/test-chain")
    );
    assert_eq!(
        config.repos["registry"].agent.as_deref(),
        Some("claude-code")
    );
    assert_eq!(config.routing.len(), 1);
}

#[test]
fn cli_parses_repo_run_shape_and_vars() {
    let cli = Cli::try_parse_from([
        "forge",
        "run",
        "new-feature",
        "--repo",
        "warrant-shell",
        "--task",
        "add verbose flag",
        "--issue",
        "42",
        "--round",
        "5",
        "--agent",
        "claude-code",
        "--model",
        "sonnet",
        "--branch",
        "feat/custom",
        "--notify",
        "openclaw,slack",
        "--var",
        "custom=value",
        "--var",
        "branch=override",
    ])
    .expect("cli should parse new run shape");

    match cli.command {
        Commands::Run {
            blueprint_name,
            blueprint,
            repo,
            task,
            issue,
            round,
            agent,
            model,
            branch,
            notify,
            vars,
            ..
        } => {
            assert_eq!(blueprint_name.as_deref(), Some("new-feature"));
            assert_eq!(blueprint, None);
            assert_eq!(repo.as_deref(), Some("warrant-shell"));
            assert_eq!(task.as_deref(), Some("add verbose flag"));
            assert_eq!(issue.as_deref(), Some("42"));
            assert_eq!(round.as_deref(), Some("5"));
            assert_eq!(agent.as_deref(), Some("claude-code"));
            assert_eq!(model.as_deref(), Some("sonnet"));
            assert_eq!(branch.as_deref(), Some("feat/custom"));
            assert_eq!(notify, vec!["openclaw".to_string(), "slack".to_string()]);
            assert_eq!(
                vars,
                vec![
                    ("custom".to_string(), "value".to_string()),
                    ("branch".to_string(), "override".to_string())
                ]
            );
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn build_run_variables_applies_resolution_order() {
    let config = load_forge_config_str(
        r#"
        [defaults]
        agent = "codex"
        model = "gpt-5.4"

        [repos.warrant-core]
        path = "/repos/warrant-core"
        crate = "core-crate"
        test_blueprint = "warrant-shell/test-chain"
        agent = "claude-code"

        [repos.warrant-shell]
        path = "/repos/warrant-shell"
        model = "gpt-5.5"
        "#,
    )
    .expect("config should parse");

    let command = Commands::Run {
        blueprint_name: Some("implement-feature".to_string()),
        blueprint: None,
        repo: Some("warrant-core".to_string()),
        task: Some("Add Verbose Flag".to_string()),
        instruction: None,
        issue: Some("42".to_string()),
        round: Some("5".to_string()),
        pr: None,
        next: false,
        latest: false,
        agent: Some("override-agent".to_string()),
        model: None,
        branch: Some("feat/add-verbose-flag".to_string()),
        dry_run: false,
        no_dashboard: false,
        port: 8400,
        notify: Vec::new(),
        verbose: false,
        vars: vec![
            ("target_model".to_string(), "override-model".to_string()),
            ("repo_path".to_string(), "/override/repo".to_string()),
        ],
    };

    let variables = build_run_variables(&config, &command, Path::new("/work/forge"), "2026-03-22")
        .expect("variables should resolve");

    assert_eq!(
        variables.get("target_repo").map(String::as_str),
        Some("warrant-core")
    );
    assert_eq!(
        variables.get("repo_path").map(String::as_str),
        Some("/override/repo")
    );
    assert_eq!(
        variables.get("target_repo_path").map(String::as_str),
        Some("/repos/warrant-core")
    );
    assert_eq!(
        variables.get("crate").map(String::as_str),
        Some("core-crate")
    );
    assert_eq!(
        variables.get("test_blueprint").map(String::as_str),
        Some("warrant-shell/test-chain")
    );
    assert_eq!(
        variables.get("target_agent").map(String::as_str),
        Some("override-agent")
    );
    assert_eq!(
        variables.get("target_model").map(String::as_str),
        Some("override-model")
    );
    assert_eq!(
        variables.get("task").map(String::as_str),
        Some("Add Verbose Flag")
    );
    assert_eq!(
        variables.get("task_description").map(String::as_str),
        Some("Add Verbose Flag")
    );
    assert_eq!(
        variables.get("commit_message").map(String::as_str),
        Some("feat/add-verbose-flag")
    );
    assert_eq!(variables.get("issue").map(String::as_str), Some("42"));
    assert_eq!(variables.get("issue_id").map(String::as_str), Some("42"));
    assert_eq!(variables.get("round").map(String::as_str), Some("5"));
    assert_eq!(
        variables.get("date").map(String::as_str),
        Some("2026-03-22")
    );
    assert_eq!(
        variables.get("forge_path").map(String::as_str),
        Some("/work/forge")
    );
    assert_eq!(
        variables.get("branch").map(String::as_str),
        Some("feat/add-verbose-flag")
    );
    assert_eq!(
        variables.get("core_path").map(String::as_str),
        Some("/repos/warrant-core")
    );
    assert_eq!(
        variables.get("shell_path").map(String::as_str),
        Some("/repos/warrant-shell")
    );
}

fn init_generated_project() -> TempDir {
    let dir = tempdir().expect("tempdir");
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("write cargo");
    init_project(
        dir.path(),
        &InitOptions {
            project_type: None,
            force: false,
        },
    )
    .expect("init project");
    dir
}

#[test]
fn auto_branch_name_matches_blueprint_conventions() {
    assert_eq!(
        auto_branch_name("code-review", None, None, None, "2026-03-22"),
        "work/cbf29ce"
    );
    assert_eq!(
        auto_branch_name(
            "implement-feature",
            Some("Add verbose flag to the command line"),
            None,
            None,
            "2026-03-22",
        ),
        "feat/65455c0"
    );
    assert_eq!(
        auto_branch_name("fix-bug", None, Some("42"), None, "2026-03-22"),
        "fix/cbf29ce"
    );
    assert_eq!(
        auto_branch_name("red-team", None, None, Some("5"), "2026-03-22"),
        "work/cbf29ce"
    );
    assert_eq!(
        auto_branch_name("lint", None, None, None, "2026-03-22"),
        "work/cbf29ce"
    );
}

#[test]
fn resolves_blueprint_paths_in_common_repo_then_root_order() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::create_dir_all(root.join("common")).expect("create common");
    fs::create_dir_all(root.join("warrant-shell")).expect("create repo");
    fs::write(root.join("common/code-review.toml"), "").expect("write common");
    fs::write(root.join("warrant-shell/red-team.toml"), "").expect("write repo");
    fs::write(root.join("lint.toml"), "").expect("write root");

    let common = resolve_blueprint_for_run(root, "code-review", Some("warrant-shell"))
        .expect("common blueprint should resolve");
    assert_eq!(common, root.join("common/code-review.toml"));

    let repo = resolve_blueprint_for_run(root, "red-team", Some("warrant-shell"))
        .expect("repo blueprint should resolve");
    assert_eq!(repo, root.join("warrant-shell/red-team.toml"));

    let fallback =
        resolve_blueprint_for_run(root, "lint", Some("warrant-shell")).expect("root fallback");
    assert_eq!(fallback, root.join("lint.toml"));
}

#[test]
fn resolve_blueprint_for_run_errors_when_missing() {
    let dir = tempdir().expect("tempdir");
    let error = resolve_blueprint_for_run(dir.path(), "missing", Some("warrant-core"))
        .expect_err("missing blueprint should fail");
    assert!(error.to_string().contains("missing"));
}
