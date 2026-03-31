use crate::error::ForgeError;
use crate::model::{Blueprint, BlueprintMeta, RepoSpec, Step, StepType};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct RawBlueprint {
    blueprint: RawBlueprintMeta,
    #[serde(rename = "step", default)]
    steps: Vec<RawStep>,
}

#[derive(Debug, Deserialize)]
struct RawBlueprintMeta {
    name: String,
    description: String,
    #[serde(default)]
    repos: Vec<RepoSpec>,
}

#[derive(Debug, Deserialize)]
struct RawStep {
    #[serde(rename = "type")]
    step_type: StepType,
    name: Option<String>,
    command: Option<String>,
    agent: Option<String>,
    model: Option<String>,
    prompt: Option<String>,
    blueprint: Option<String>,
    #[serde(default)]
    params: BTreeMap<String, String>,
    condition: Option<String>,
    sets: Option<String>,
    #[serde(default)]
    allow_failure: bool,
    max_retries: Option<u32>,
    #[serde(default)]
    expect_failure: bool,
    #[serde(default)]
    env: BTreeMap<String, String>,
}

pub fn parse_blueprint_str(input: &str) -> Result<Blueprint, ForgeError> {
    let raw: RawBlueprint = toml::from_str(input)?;
    let steps = raw
        .steps
        .into_iter()
        .enumerate()
        .map(|(index, step)| validate_step(step, index))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Blueprint {
        blueprint: BlueprintMeta {
            name: raw.blueprint.name,
            description: raw.blueprint.description,
            repos: raw.blueprint.repos,
        },
        steps,
        source_path: None,
    })
}

pub fn parse_blueprint_file(path: impl AsRef<Path>) -> Result<Blueprint, ForgeError> {
    let path = path.as_ref();
    let input = fs::read_to_string(path)?;
    let mut blueprint = parse_blueprint_str(&input)?;
    blueprint.source_path = Some(path.to_path_buf());
    Ok(blueprint)
}

fn validate_step(raw: RawStep, index: usize) -> Result<Step, ForgeError> {
    let name = raw.name.ok_or_else(|| {
        ForgeError::message(format!("missing required field `name` for step #{index}"))
    })?;

    match raw.step_type {
        StepType::Deterministic | StepType::Conditional => {
            require_field(raw.command.as_ref(), "command", &name)?;
        }
        StepType::Gate => {
            if raw.command.is_none() && raw.blueprint.is_none() {
                return Err(ForgeError::message(format!(
                    "missing required field `command` or `blueprint` for gate step `{name}`"
                )));
            }
        }
        StepType::Agentic => {
            require_field(raw.agent.as_ref(), "agent", &name)?;
            require_field(raw.model.as_ref(), "model", &name)?;
            require_field(raw.prompt.as_ref(), "prompt", &name)?;
        }
        StepType::Blueprint => {
            require_field(raw.blueprint.as_ref(), "blueprint", &name)?;
        }
    }

    Ok(Step {
        step_type: raw.step_type,
        name,
        command: raw.command,
        agent: raw.agent,
        model: raw.model,
        prompt: raw.prompt,
        blueprint: raw.blueprint,
        params: raw.params,
        condition: raw.condition,
        sets: raw.sets,
        allow_failure: raw.allow_failure,
        max_retries: raw.max_retries,
        expect_failure: raw.expect_failure,
        env: raw.env,
    })
}

fn require_field<T>(value: Option<T>, field: &str, step_name: &str) -> Result<(), ForgeError> {
    if value.is_none() {
        return Err(ForgeError::message(format!(
            "missing required field `{field}` for step `{step_name}`"
        )));
    }

    Ok(())
}
