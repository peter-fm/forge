use crate::dashboard::{DashboardObserver, StepStatus as DashboardStepStatus};
use crate::error::ForgeError;
use crate::logger::{RunLogger, StepLog};
use crate::model::{Blueprint, RunContext, RunSummary, Step, StepResult, StepStatus, StepType};
use crate::run_status;
use crate::{condition, vars};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

pub trait Runtime {
    fn run_command(
        &mut self,
        step_name: &str,
        command: &str,
        env: &BTreeMap<String, String>,
        log_path: Option<&Path>,
    ) -> Result<ExecutionOutput, ForgeError>;

    fn run_agent(
        &mut self,
        step_name: &str,
        agent: &str,
        model: &str,
        prompt: &str,
        env: &BTreeMap<String, String>,
        log_path: Option<&Path>,
    ) -> Result<ExecutionOutput, ForgeError>;
}

pub trait BlueprintLoader {
    fn load(&self, path: &Path) -> Result<Blueprint, ForgeError>;
}

pub struct Engine<L, R, G> {
    pub loader: L,
    pub runtime: R,
    pub logger: G,
    pub blueprint_root: PathBuf,
    pub dashboard: Option<DashboardObserver>,
}

impl<L, R, G> Engine<L, R, G>
where
    L: BlueprintLoader,
    R: Runtime,
    G: RunLogger,
{
    pub fn run_blueprint(
        &mut self,
        blueprint: &Blueprint,
        context: &mut RunContext,
    ) -> Result<RunSummary, ForgeError> {
        let is_root = context.blueprint_stack.is_empty();
        let stack_key = blueprint_stack_key(blueprint);
        if context.blueprint_stack.contains(&stack_key) {
            return Err(ForgeError::message(format!(
                "circular blueprint reference detected for {stack_key}"
            )));
        }

        if is_root && context.run_started_at.is_none() {
            context.run_started_at = Some(now_secs()?);
        }
        context.blueprint_stack.push(stack_key);
        if is_root {
            self.write_status_snapshot(blueprint, context, None, "running")?;
        }
        let result = self.run_blueprint_inner(blueprint, context);
        context.blueprint_stack.pop();
        if is_root {
            let state = if result.is_ok() {
                "succeeded"
            } else {
                "failed"
            };
            self.write_status_snapshot(blueprint, context, None, state)?;
        }
        result
    }

    fn run_blueprint_inner(
        &mut self,
        blueprint: &Blueprint,
        context: &mut RunContext,
    ) -> Result<RunSummary, ForgeError> {
        let is_root = context.blueprint_stack.len() == 1;
        let mut summary_steps = Vec::new();
        let mut index = 0;

        while index < blueprint.steps.len() {
            let step = &blueprint.steps[index];
            if !should_run(step, context)? {
                if is_root {
                    self.finish_dashboard_step(
                        index,
                        step,
                        DashboardStepStatus::Skipped,
                        None,
                        None,
                    );
                }
                let result = StepResult {
                    name: step.name.clone(),
                    step_type: step.step_type.clone(),
                    status: StepStatus::Skipped,
                    exit_code: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                    attempts: 0,
                    log_file: self.create_empty_step_log(&step.name)?,
                };
                self.record_result(context, &mut summary_steps, result)?;
                self.write_status_snapshot(blueprint, context, None, "running")?;
                index += 1;
                continue;
            }

            context
                .step_started_at
                .entry(step.name.clone())
                .or_insert(now_secs()?);
            let started_at = Instant::now();
            if is_root {
                self.start_dashboard_step(index, step);
            }
            self.write_status_snapshot(blueprint, context, Some(&step.name), "running")?;

            if step.step_type == StepType::Agentic && step.max_retries.unwrap_or(0) > 0 {
                let retry_target = self.determine_retry_target(blueprint, index, context);
                let (agent_result, retry_result, consumed_next) =
                    self.run_agentic_with_retries(step, retry_target.as_ref(), context)?;
                if is_root {
                    self.finish_dashboard_step(
                        index,
                        step,
                        dashboard_status(&agent_result.status),
                        Some(join_step_output(&agent_result)),
                        Some(started_at.elapsed().as_millis() as u64),
                    );
                }
                self.record_result(context, &mut summary_steps, agent_result)?;
                if consumed_next {
                    if let Some(target_result) = retry_result {
                        self.record_result(context, &mut summary_steps, target_result)?;
                    }
                    self.write_status_snapshot(blueprint, context, None, "running")?;
                    index += 2;
                } else {
                    self.write_status_snapshot(blueprint, context, None, "running")?;
                    index += 1;
                }
                continue;
            }

            if matches!(step.step_type, StepType::Blueprint | StepType::Gate)
                && step.blueprint.is_some()
            {
                let results = self.run_sub_blueprint(step, blueprint, context)?;
                let parent_result = results.last().cloned().ok_or_else(|| {
                    ForgeError::message(format!(
                        "sub-blueprint step `{}` produced no results",
                        step.name
                    ))
                })?;
                if is_root {
                    self.finish_dashboard_step(
                        index,
                        step,
                        dashboard_status(&parent_result.status),
                        Some(join_step_output(&parent_result)),
                        Some(started_at.elapsed().as_millis() as u64),
                    );
                }
                let failed = parent_result.status == StepStatus::Failed;
                for result in results {
                    self.record_result(context, &mut summary_steps, result)?;
                }
                self.write_status_snapshot(blueprint, context, None, "running")?;
                if step.step_type == StepType::Gate && failed {
                    return Err(ForgeError::message(format!(
                        "gate step `{}` failed",
                        step.name
                    )));
                }
                if failed && !step.allow_failure {
                    return Err(ForgeError::message(format!("step `{}` failed", step.name)));
                }
                index += 1;
                continue;
            }

            let result = self.run_single_step(step, context)?;
            if is_root {
                self.finish_dashboard_step(
                    index,
                    step,
                    dashboard_status(&result.status),
                    Some(join_step_output(&result)),
                    Some(started_at.elapsed().as_millis() as u64),
                );
            }
            let should_abort = result.status == StepStatus::Failed && !step.allow_failure;
            let gate_failed =
                step.step_type == StepType::Gate && result.status == StepStatus::Failed;
            self.record_result(context, &mut summary_steps, result.clone())?;
            self.write_status_snapshot(blueprint, context, None, "running")?;
            if gate_failed {
                return Err(ForgeError::message(format!(
                    "gate step `{}` failed",
                    step.name
                )));
            }
            if should_abort {
                return Err(ForgeError::message(format!("step `{}` failed", step.name)));
            }
            index += 1;
        }

        Ok(RunSummary {
            steps: summary_steps,
        })
    }

    fn start_dashboard_step(&self, index: usize, step: &Step) {
        if let Some(dashboard) = &self.dashboard {
            dashboard.start_step(index, &step.name);
        }
    }

    fn finish_dashboard_step(
        &self,
        index: usize,
        step: &Step,
        status: DashboardStepStatus,
        output: Option<String>,
        duration_ms: Option<u64>,
    ) {
        if let Some(dashboard) = &self.dashboard {
            dashboard.finish_step(index, &step.name, status, output, duration_ms);
        }
    }

    fn run_single_step(
        &mut self,
        step: &Step,
        context: &mut RunContext,
    ) -> Result<StepResult, ForgeError> {
        let step_log = self.logger.create_step_log(&step.name)?;
        let env = resolve_env(step, context)?;
        let variables = vars::build_variable_scope(context);

        let execution = match step.step_type {
            StepType::Deterministic | StepType::Conditional | StepType::Gate => {
                let command = step.command.as_deref().ok_or_else(|| {
                    ForgeError::message(format!("step `{}` is missing a command", step.name))
                })?;
                let command = vars::substitute_text(command, &variables)?;
                if context.dry_run {
                    let result = StepResult {
                        name: step.name.clone(),
                        step_type: step.step_type.clone(),
                        status: StepStatus::Succeeded,
                        exit_code: 0,
                        stdout: command,
                        stderr: String::new(),
                        attempts: 1,
                        log_file: step_log.as_ref().map(|log| log.relative_path.clone()),
                    };
                    write_step_log(step_log.as_ref(), &result.stdout, &result.stderr)?;
                    apply_sets(step, &result, context);
                    return Ok(result);
                }
                self.runtime
                    .run_command(&step.name, &command, &env, step_log_path(&step_log))?
            }
            StepType::Agentic => {
                let agent = vars::substitute_text(
                    step.agent
                        .as_deref()
                        .ok_or_else(|| ForgeError::message("missing agent"))?,
                    &variables,
                )?;
                let model = vars::substitute_text(
                    step.model
                        .as_deref()
                        .ok_or_else(|| ForgeError::message("missing model"))?,
                    &variables,
                )?;
                let prompt = vars::substitute_text(
                    step.prompt
                        .as_deref()
                        .ok_or_else(|| ForgeError::message("missing prompt"))?,
                    &variables,
                )?;
                if context.dry_run {
                    let result = StepResult {
                        name: step.name.clone(),
                        step_type: step.step_type.clone(),
                        status: StepStatus::Succeeded,
                        exit_code: 0,
                        stdout: prompt,
                        stderr: format!("agent={agent} model={model}"),
                        attempts: 1,
                        log_file: step_log.as_ref().map(|log| log.relative_path.clone()),
                    };
                    write_step_log(step_log.as_ref(), &result.stdout, &result.stderr)?;
                    apply_sets(step, &result, context);
                    return Ok(result);
                }
                self.runtime
                    .run_agent(
                        &step.name,
                        &agent,
                        &model,
                        &prompt,
                        &env,
                        step_log_path(&step_log),
                    )?
            }
            StepType::Blueprint => {
                return Err(ForgeError::message(format!(
                    "blueprint step `{}` must be handled through sub-blueprint execution",
                    step.name
                )));
            }
        };

        let succeeded = if step.expect_failure {
            execution.exit_code != 0
        } else {
            execution.exit_code == 0
        };

        let result = StepResult {
            name: step.name.clone(),
            step_type: step.step_type.clone(),
            status: if succeeded {
                StepStatus::Succeeded
            } else {
                StepStatus::Failed
            },
            exit_code: execution.exit_code,
            stdout: execution.stdout,
            stderr: execution.stderr,
            attempts: 1,
            log_file: step_log.map(|log| log.relative_path),
        };

        apply_sets(step, &result, context);
        Ok(result)
    }

    fn run_sub_blueprint(
        &mut self,
        step: &Step,
        parent: &Blueprint,
        context: &mut RunContext,
    ) -> Result<Vec<StepResult>, ForgeError> {
        // Substitute variables in the blueprint reference before resolving
        // the path, so that dynamic references like {test_blueprint} work.
        let mut resolved_step = step.clone();
        if let Some(ref bp) = step.blueprint {
            let variables = vars::build_variable_scope(context);
            resolved_step.blueprint = Some(vars::substitute_text(bp, &variables)?);
        }
        let path = resolve_blueprint_path(&self.blueprint_root, parent, &resolved_step)?;
        let mut child = self.loader.load(&path).map_err(|error| {
            ForgeError::message(format!(
                "failed to load sub-blueprint `{}`: {error}",
                step.blueprint.as_deref().unwrap_or_default()
            ))
        })?;
        if child.source_path.is_none() {
            child.source_path = Some(path);
        }
        let child = apply_params(child, step, context)?;
        let child_step_names = child
            .steps
            .iter()
            .map(|child_step| child_step.name.clone())
            .collect::<Vec<_>>();
        let mut results = match self.run_blueprint(&child, context) {
            Ok(summary) => summary.steps,
            Err(error) => {
                let partial_results = child_step_names
                    .iter()
                    .filter_map(|name| context.step_results.get(name).cloned())
                    .collect::<Vec<_>>();
                if partial_results.is_empty() {
                    return Err(error);
                }
                partial_results
            }
        };

        if !results.iter().any(|result| result.name == step.name) {
            let parent_result = synthesize_parent_result(step, &results);
            apply_sets(step, &parent_result, context);
            context
                .step_results
                .insert(parent_result.name.clone(), parent_result.clone());
            results.push(parent_result);
        }

        Ok(results)
    }

    fn determine_retry_target(
        &self,
        blueprint: &Blueprint,
        index: usize,
        context: &RunContext,
    ) -> Option<Step> {
        let step = &blueprint.steps[index];
        if let Some(condition) = &step.condition {
            for token in
                condition.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '-' || ch == '.'))
            {
                if let Some(name) = token.strip_suffix(".exit_code")
                    && context.step_results.contains_key(name)
                {
                    if let Some(existing_step) = blueprint
                        .steps
                        .iter()
                        .find(|candidate| candidate.name == name)
                    {
                        return Some(existing_step.clone());
                    }
                }
            }
        }

        blueprint.steps.get(index + 1).cloned()
    }

    fn run_agentic_with_retries(
        &mut self,
        step: &Step,
        retry_target: Option<&Step>,
        context: &mut RunContext,
    ) -> Result<(StepResult, Option<StepResult>, bool), ForgeError> {
        let max_retries = step.max_retries.unwrap_or(1);
        let mut attempts = 0;
        let mut consumed_next = false;
        context
            .variables
            .entry("test_output".to_string())
            .or_default();

        loop {
            attempts += 1;
            let mut agent_result = self.run_single_step(step, context)?;
            agent_result.attempts = attempts;
            self.ensure_step_log(&mut agent_result)?;
            context
                .step_results
                .insert(step.name.clone(), agent_result.clone());

            if let Some(target) = retry_target {
                let mut test_result =
                    if matches!(target.step_type, StepType::Blueprint | StepType::Gate)
                        && target.blueprint.is_some()
                    {
                        let synthetic_parent = Blueprint {
                            blueprint: blueprint_meta("retry"),
                            steps: vec![target.clone()],
                            source_path: None,
                        };
                        let mut results =
                            self.run_sub_blueprint(target, &synthetic_parent, context)?;
                        results.pop().ok_or_else(|| {
                            ForgeError::message("retry target produced no step results")
                        })?
                    } else {
                        consumed_next = true;
                        self.run_single_step(target, context)?
                    };
                self.ensure_step_log(&mut test_result)?;

                context.variables.insert(
                    "test_output".to_string(),
                    vars::join_output(&test_result.stdout, &test_result.stderr),
                );
                context
                    .step_results
                    .insert(test_result.name.clone(), test_result.clone());

                if test_result.status == StepStatus::Succeeded {
                    return Ok((agent_result, Some(test_result), consumed_next));
                }

                if attempts >= max_retries {
                    self.logger.log_step(&agent_result)?;
                    self.logger.log_step(&test_result)?;
                    return Err(ForgeError::message(format!(
                        "step `{}` failed after {attempts} attempts",
                        step.name
                    )));
                }
                continue;
            }

            if agent_result.status == StepStatus::Succeeded {
                return Ok((agent_result, None, consumed_next));
            }
            if attempts >= max_retries {
                self.logger.log_step(&agent_result)?;
                return Err(ForgeError::message(format!(
                    "step `{}` failed after {attempts} attempts",
                    step.name
                )));
            }
        }
    }

    fn record_result(
        &mut self,
        context: &mut RunContext,
        summary: &mut Vec<StepResult>,
        mut result: StepResult,
    ) -> Result<(), ForgeError> {
        self.ensure_step_log(&mut result)?;
        context
            .step_results
            .insert(result.name.clone(), result.clone());
        self.logger.log_step(&result)?;
        summary.push(result);
        Ok(())
    }

    fn write_status_snapshot(
        &self,
        blueprint: &Blueprint,
        context: &RunContext,
        current_step: Option<&str>,
        state: &str,
    ) -> Result<(), ForgeError> {
        let Some(path) = context.status_path.as_ref() else {
            return Ok(());
        };
        let step_names = blueprint
            .steps
            .iter()
            .map(|step| step.name.clone())
            .collect::<Vec<_>>();
        run_status::write_snapshot(
            path,
            &blueprint.blueprint.name,
            &step_names,
            context,
            current_step,
            state,
        )
    }

    fn create_empty_step_log(&mut self, step_name: &str) -> Result<Option<String>, ForgeError> {
        let step_log = self.logger.create_step_log(step_name)?;
        write_step_log(step_log.as_ref(), "", "")?;
        Ok(step_log.map(|log| log.relative_path))
    }

    fn ensure_step_log(&mut self, result: &mut StepResult) -> Result<(), ForgeError> {
        if result.log_file.is_none() {
            result.log_file = self.create_empty_step_log(&result.name)?;
        }
        Ok(())
    }
}

fn step_log_path(step_log: &Option<StepLog>) -> Option<&Path> {
    step_log.as_ref().map(|log| log.path.as_path())
}

fn write_step_log(step_log: Option<&StepLog>, stdout: &str, stderr: &str) -> Result<(), ForgeError> {
    let Some(step_log) = step_log else {
        return Ok(());
    };

    let mut contents = String::with_capacity(stdout.len() + stderr.len());
    contents.push_str(stdout);
    contents.push_str(stderr);
    fs::write(&step_log.path, contents)?;
    Ok(())
}

fn join_step_output(result: &StepResult) -> String {
    vars::join_output(&result.stdout, &result.stderr)
}

fn dashboard_status(status: &StepStatus) -> DashboardStepStatus {
    match status {
        StepStatus::Pending => DashboardStepStatus::Pending,
        StepStatus::Skipped => DashboardStepStatus::Skipped,
        StepStatus::Succeeded => DashboardStepStatus::Passed,
        StepStatus::Failed => DashboardStepStatus::Failed,
    }
}

fn should_run(step: &Step, context: &RunContext) -> Result<bool, ForgeError> {
    match &step.condition {
        Some(condition_expr) => {
            condition::evaluate_condition(condition_expr, &vars::build_variable_scope(context))
        }
        None => Ok(true),
    }
}

fn resolve_env(step: &Step, context: &RunContext) -> Result<BTreeMap<String, String>, ForgeError> {
    let variables = vars::build_variable_scope(context);
    let mut env = step
        .env
        .iter()
        .map(|(key, value)| Ok((key.clone(), vars::substitute_text(value, &variables)?)))
        .collect::<Result<BTreeMap<_, _>, ForgeError>>()?;
    inject_runtime_metadata(&mut env, &variables);
    Ok(env)
}

fn resolve_blueprint_path(
    root: &Path,
    parent: &Blueprint,
    step: &Step,
) -> Result<PathBuf, ForgeError> {
    let reference = step.blueprint.as_deref().ok_or_else(|| {
        ForgeError::message(format!("step `{}` is missing blueprint path", step.name))
    })?;

    if reference.contains('/') {
        return Ok(root.join(format!("{reference}.toml")));
    }

    if let Some(parent_path) = &parent.source_path
        && let Some(parent_dir) = parent_path.parent()
    {
        let sibling = parent_dir.join(format!("{reference}.toml"));
        if sibling.exists() {
            return Ok(sibling);
        }
        let common = root.join("common").join(format!("{reference}.toml"));
        if common.exists() {
            return Ok(common);
        }
    }

    Ok(root.join(format!("{reference}.toml")))
}

fn apply_params(
    mut blueprint: Blueprint,
    step: &Step,
    context: &RunContext,
) -> Result<Blueprint, ForgeError> {
    let mut variables = vars::build_variable_scope(context);
    for (key, value) in &step.params {
        let resolved = vars::substitute_text(value, &variables)?;
        variables.insert(key.clone(), resolved);
    }

    for child_step in &mut blueprint.steps {
        child_step.command = substitute_option(child_step.command.take(), &variables)?;
        child_step.agent = substitute_option(child_step.agent.take(), &variables)?;
        child_step.model = substitute_option(child_step.model.take(), &variables)?;
        child_step.prompt = substitute_option(child_step.prompt.take(), &variables)?;
        child_step.blueprint = substitute_option(child_step.blueprint.take(), &variables)?;
        child_step.condition = substitute_option(child_step.condition.take(), &variables)?;
        child_step.params = child_step
            .params
            .iter()
            .map(|(key, value)| Ok((key.clone(), vars::substitute_text(value, &variables)?)))
            .collect::<Result<_, ForgeError>>()?;
        child_step.env = child_step
            .env
            .iter()
            .map(|(key, value)| Ok((key.clone(), vars::substitute_text(value, &variables)?)))
            .collect::<Result<_, ForgeError>>()?;
    }

    Ok(blueprint)
}

fn substitute_option(
    value: Option<String>,
    variables: &BTreeMap<String, String>,
) -> Result<Option<String>, ForgeError> {
    value
        .map(|text| vars::substitute_text(&text, variables))
        .transpose()
}

fn apply_sets(step: &Step, result: &StepResult, context: &mut RunContext) {
    if let Some(variable) = &step.sets {
        context.variables.insert(
            variable.clone(),
            if result.exit_code == 0 {
                "true".to_string()
            } else {
                "false".to_string()
            },
        );
    }
}

fn synthesize_parent_result(step: &Step, child_results: &[StepResult]) -> StepResult {
    let exit_code = child_results
        .iter()
        .map(|result| result.exit_code)
        .max()
        .unwrap_or(0);
    let status = if child_results
        .iter()
        .any(|result| result.status == StepStatus::Failed)
    {
        StepStatus::Failed
    } else {
        StepStatus::Succeeded
    };

    StepResult {
        name: step.name.clone(),
        step_type: step.step_type.clone(),
        status,
        exit_code,
        stdout: String::new(),
        stderr: String::new(),
        attempts: 1,
        log_file: None,
    }
}

fn inject_runtime_metadata(
    env: &mut BTreeMap<String, String>,
    variables: &BTreeMap<String, String>,
) {
    if let Ok(cwd) = std::env::current_dir() {
        env.entry("PWD".to_string())
            .or_insert_with(|| cwd.display().to_string());
    }
    for (key, value) in variables {
        if key.ends_with("_path") {
            let env_key = format!("FORGE_{}", key.to_ascii_uppercase());
            env.entry(env_key).or_insert_with(|| value.clone());
        }
    }
}

fn blueprint_stack_key(blueprint: &Blueprint) -> String {
    blueprint
        .source_path
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| blueprint.blueprint.name.clone())
}

fn blueprint_meta(name: &str) -> crate::model::BlueprintMeta {
    crate::model::BlueprintMeta {
        name: name.to_string(),
        description: "synthetic".to_string(),
        repos: Vec::new(),
    }
}

fn now_secs() -> Result<u64, ForgeError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|error| ForgeError::message(error.to_string()))
}
