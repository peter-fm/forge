use crate::error::ForgeError;
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectType {
    Rust,
    Go,
    Python,
    Bun,
    Node,
    Make,
}

impl ProjectType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Go => "go",
            Self::Python => "python",
            Self::Bun => "bun",
            Self::Node => "node",
            Self::Make => "make",
        }
    }
}

impl std::str::FromStr for ProjectType {
    type Err = ForgeError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "rust" => Ok(Self::Rust),
            "go" => Ok(Self::Go),
            "python" => Ok(Self::Python),
            "bun" => Ok(Self::Bun),
            "node" => Ok(Self::Node),
            "make" => Ok(Self::Make),
            _ => Err(ForgeError::message(format!(
                "unsupported project type `{input}`"
            ))),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectCommands {
    pub test: Option<String>,
    pub lint: Option<String>,
    pub build: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedProject {
    pub project_type: ProjectType,
    pub name: String,
    pub commands: ProjectCommands,
    pub agents_md_present: bool,
}

pub fn detect_project(
    root: &Path,
    forced_type: Option<ProjectType>,
) -> Result<DetectedProject, ForgeError> {
    let project_type = match forced_type {
        Some(project_type) => project_type,
        None => detect_project_type(root)?,
    };
    let package = load_package_json(root).ok();
    let ci_hints = detect_ci_hints(root);
    let commands = detect_commands(root, project_type, package.as_ref(), &ci_hints);
    let name = detect_project_name(root, project_type, package.as_ref())?;

    Ok(DetectedProject {
        project_type,
        name,
        commands,
        agents_md_present: root.join("AGENTS.md").exists(),
    })
}

fn detect_project_type(root: &Path) -> Result<ProjectType, ForgeError> {
    if root.join("Cargo.toml").exists() {
        return Ok(ProjectType::Rust);
    }
    if root.join("go.mod").exists() {
        return Ok(ProjectType::Go);
    }
    if root.join("pyproject.toml").exists() || root.join("setup.py").exists() {
        return Ok(ProjectType::Python);
    }
    if root.join("package.json").exists() && root.join("bun.lock").exists() {
        return Ok(ProjectType::Bun);
    }
    if root.join("package.json").exists() && root.join("node_modules").exists() {
        return Ok(ProjectType::Node);
    }
    if root.join("Makefile").exists() {
        return Ok(ProjectType::Make);
    }

    Err(ForgeError::message(
        "unable to detect project type in current directory",
    ))
}

fn detect_project_name(
    root: &Path,
    project_type: ProjectType,
    package: Option<&JsonValue>,
) -> Result<String, ForgeError> {
    match project_type {
        ProjectType::Rust => {
            if let Ok(input) = fs::read_to_string(root.join("Cargo.toml"))
                && let Ok(value) = input.parse::<toml::Value>()
                && let Some(name) = value
                    .get("package")
                    .and_then(|package| package.get("name"))
                    .and_then(toml::Value::as_str)
            {
                return Ok(name.to_string());
            }
        }
        ProjectType::Python => {
            if let Ok(input) = fs::read_to_string(root.join("pyproject.toml"))
                && let Ok(value) = input.parse::<toml::Value>()
            {
                if let Some(name) = value
                    .get("project")
                    .and_then(|project| project.get("name"))
                    .and_then(toml::Value::as_str)
                {
                    return Ok(name.to_string());
                }
                if let Some(name) = value
                    .get("tool")
                    .and_then(|tool| tool.get("poetry"))
                    .and_then(|poetry| poetry.get("name"))
                    .and_then(toml::Value::as_str)
                {
                    return Ok(name.to_string());
                }
            }
        }
        ProjectType::Bun | ProjectType::Node => {
            if let Some(name) = package
                .and_then(|package| package.get("name"))
                .and_then(JsonValue::as_str)
            {
                return Ok(name.to_string());
            }
        }
        _ => {}
    }

    root.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_string())
        .ok_or_else(|| ForgeError::message("unable to determine project name"))
}

fn load_package_json(root: &Path) -> Result<JsonValue, ForgeError> {
    let input = fs::read_to_string(root.join("package.json"))?;
    Ok(serde_json::from_str(&input)?)
}

#[derive(Debug, Default)]
struct CiHints {
    test: Option<String>,
    lint: Option<String>,
    build: Option<String>,
}

fn detect_ci_hints(root: &Path) -> CiHints {
    let workflow_dir = root.join(".github/workflows");
    let mut hints = CiHints::default();
    let Ok(entries) = fs::read_dir(workflow_dir) else {
        return hints;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("yml" | "yaml")
        ) {
            continue;
        }
        let Ok(input) = fs::read_to_string(path) else {
            continue;
        };
        for line in input.lines() {
            let Some(command) = line.split_once("run:").map(|(_, value)| value.trim()) else {
                continue;
            };
            if hints.test.is_none() && looks_like_test_command(command) {
                hints.test = Some(command.to_string());
            }
            if hints.lint.is_none() && looks_like_lint_command(command) {
                hints.lint = Some(command.to_string());
            }
            if hints.build.is_none() && looks_like_build_command(command) {
                hints.build = Some(command.to_string());
            }
        }
    }

    hints
}

fn detect_commands(
    root: &Path,
    project_type: ProjectType,
    package: Option<&JsonValue>,
    ci_hints: &CiHints,
) -> ProjectCommands {
    match project_type {
        ProjectType::Rust => {
            ProjectCommands {
                test: Some(
                    ci_hints
                        .test
                        .clone()
                        .unwrap_or_else(|| "cargo test".to_string()),
                ),
                lint: Some(ci_hints.lint.clone().unwrap_or_else(|| {
                    "cargo fmt --check && cargo clippy -- -D warnings".to_string()
                })),
                build: Some(
                    ci_hints
                        .build
                        .clone()
                        .unwrap_or_else(|| "cargo build".to_string()),
                ),
            }
        }
        ProjectType::Go => ProjectCommands {
            test: Some(
                ci_hints
                    .test
                    .clone()
                    .unwrap_or_else(|| "go test ./...".to_string()),
            ),
            lint: Some(
                ci_hints
                    .lint
                    .clone()
                    .unwrap_or_else(|| "go vet ./...".to_string()),
            ),
            build: Some(
                ci_hints
                    .build
                    .clone()
                    .unwrap_or_else(|| "go build ./...".to_string()),
            ),
        },
        ProjectType::Python => ProjectCommands {
            test: Some(
                ci_hints
                    .test
                    .clone()
                    .unwrap_or_else(|| "pytest".to_string()),
            ),
            lint: Some(
                ci_hints
                    .lint
                    .clone()
                    .unwrap_or_else(|| "ruff check .".to_string()),
            ),
            build: ci_hints.build.clone(),
        },
        ProjectType::Bun => detect_js_commands(package, ci_hints, true),
        ProjectType::Node => detect_js_commands(package, ci_hints, false),
        ProjectType::Make => {
            let targets = load_make_targets(root);
            ProjectCommands {
                test: Some(
                    ci_hints
                        .test
                        .clone()
                        .unwrap_or_else(|| "make test".to_string()),
                ),
                lint: targets.contains(&"lint".to_string()).then(|| {
                    ci_hints
                        .lint
                        .clone()
                        .unwrap_or_else(|| "make lint".to_string())
                }),
                build: targets.contains(&"build".to_string()).then(|| {
                    ci_hints
                        .build
                        .clone()
                        .unwrap_or_else(|| "make build".to_string())
                }),
            }
        }
    }
}

fn detect_js_commands(
    package: Option<&JsonValue>,
    ci_hints: &CiHints,
    bun: bool,
) -> ProjectCommands {
    let scripts = package
        .and_then(|package| package.get("scripts"))
        .and_then(JsonValue::as_object);
    let exec = if bun { "bun" } else { "npm" };
    let default_test = if bun { "bun test" } else { "npm test" };
    let fallback_lint = script_command(exec, pick_script_name(scripts, &["lint", "check"]));
    let fallback_build = script_command(exec, pick_script_name(scripts, &["build"]));

    ProjectCommands {
        test: Some(
            ci_hints
                .test
                .clone()
                .unwrap_or_else(|| default_test.to_string()),
        ),
        lint: ci_hints.lint.clone().or(fallback_lint),
        build: ci_hints.build.clone().or(fallback_build),
    }
}

fn pick_script_name(
    scripts: Option<&serde_json::Map<String, JsonValue>>,
    candidates: &[&str],
) -> Option<String> {
    let scripts = scripts?;
    for candidate in candidates {
        if scripts.contains_key(*candidate) {
            return Some((*candidate).to_string());
        }
    }
    scripts
        .keys()
        .find(|name| candidates.iter().any(|candidate| name.contains(candidate)))
        .cloned()
}

fn script_command(exec: &str, script_name: Option<String>) -> Option<String> {
    let script_name = script_name?;
    if exec == "npm" && script_name == "test" {
        return Some("npm test".to_string());
    }
    if exec == "bun" && script_name == "test" {
        return Some("bun test".to_string());
    }
    Some(format!("{exec} run {script_name}"))
}

fn load_make_targets(root: &Path) -> Vec<String> {
    let Ok(input) = fs::read_to_string(root.join("Makefile")) else {
        return Vec::new();
    };
    input
        .lines()
        .filter_map(|line| {
            let candidate = line.split_once(':')?.0.trim();
            if candidate.is_empty()
                || candidate.starts_with('.')
                || candidate.contains(' ')
                || candidate.contains('=')
                || candidate.contains('%')
            {
                return None;
            }
            Some(candidate.to_string())
        })
        .collect()
}

fn looks_like_test_command(command: &str) -> bool {
    command.contains(" test") || command.starts_with("test ") || command.contains("pytest")
}

fn looks_like_lint_command(command: &str) -> bool {
    command.contains("lint") || command.contains("clippy") || command.contains("ruff")
}

fn looks_like_build_command(command: &str) -> bool {
    command.contains("build")
}
