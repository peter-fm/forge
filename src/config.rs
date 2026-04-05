use crate::cli::Commands;
use crate::error::ForgeError;
use crate::summarize::{TaskSummary, summarize_task};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::hash::Hasher;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ForgeConfig {
    pub project: ProjectConfig,
    pub commands: CommandConfig,
    pub instructions: InstructionsConfig,
    pub workspace: WorkspaceConfig,
    pub defaults: Defaults,
    pub repos: BTreeMap<String, RepoConfig>,
    pub routing: Vec<RoutingRule>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct ProjectConfig {
    #[serde(rename = "type")]
    pub project_type: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct CommandConfig {
    pub test: Option<String>,
    pub lint: Option<String>,
    pub build: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct InstructionsConfig {
    pub directory: Option<String>,
    pub gitignore: Option<bool>,
    pub agents_md: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct WorkspaceConfig {
    pub instructions: Option<String>,
    pub archive: Option<String>,
    pub auto_archive: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Defaults {
    pub agent: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepoConfig {
    pub path: String,
    pub crate_name: Option<String>,
    pub test_blueprint: Option<String>,
    pub agent: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RoutingRule {
    pub pattern: String,
    pub preferred_agent: String,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
struct RawForgeConfig {
    #[serde(default)]
    project: ProjectConfig,
    #[serde(default)]
    commands: CommandConfig,
    #[serde(default)]
    instructions: InstructionsConfig,
    #[serde(default)]
    workspace: WorkspaceConfig,
    #[serde(default)]
    agent: AgentConfig,
    #[serde(default)]
    defaults: Defaults,
    #[serde(default)]
    repos: BTreeMap<String, RawRepoConfig>,
    #[serde(default)]
    routing: Vec<RoutingRule>,
}

#[derive(Debug, Default, Deserialize)]
struct AgentConfig {
    default: Option<String>,
    model: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawRepoConfig {
    path: String,
    #[serde(rename = "crate")]
    crate_name: Option<String>,
    test_blueprint: Option<String>,
    agent: Option<String>,
    model: Option<String>,
}

pub fn load_forge_config(path: impl AsRef<Path>) -> Result<ForgeConfig, ForgeError> {
    let input = fs::read_to_string(path)?;
    load_forge_config_str(&input)
}

pub fn load_forge_config_str(input: &str) -> Result<ForgeConfig, ForgeError> {
    let raw: RawForgeConfig = toml::from_str(input)?;
    let repos = raw
        .repos
        .into_iter()
        .map(|(name, repo)| {
            Ok((
                name,
                RepoConfig {
                    path: expand_home(&repo.path)?,
                    crate_name: repo.crate_name,
                    test_blueprint: repo.test_blueprint,
                    agent: repo.agent,
                    model: repo.model,
                },
            ))
        })
        .collect::<Result<BTreeMap<_, _>, ForgeError>>()?;

    Ok(ForgeConfig {
        project: raw.project,
        commands: raw.commands,
        instructions: raw.instructions,
        workspace: raw.workspace,
        defaults: Defaults {
            agent: raw.agent.default.or(raw.defaults.agent),
            model: raw.agent.model.or(raw.defaults.model),
        },
        repos,
        routing: raw.routing,
    })
}

impl ForgeConfig {
    pub fn workspace_instructions_dir(&self) -> &str {
        self.workspace
            .instructions
            .as_deref()
            .or(self.instructions.directory.as_deref())
            .unwrap_or("instructions")
    }

    pub fn workspace_archive_dir(&self) -> &str {
        self.workspace.archive.as_deref().unwrap_or("archive")
    }

    pub fn workspace_auto_archive(&self) -> bool {
        self.workspace.auto_archive.unwrap_or(true)
    }
}

pub fn load_forge_config_if_exists(path: impl AsRef<Path>) -> Result<ForgeConfig, ForgeError> {
    let path = path.as_ref();
    if path.exists() {
        load_forge_config(path)
    } else {
        Ok(ForgeConfig::default())
    }
}

pub fn build_run_variables(
    config: &ForgeConfig,
    command: &Commands,
    forge_root: &Path,
    date: &str,
) -> Result<BTreeMap<String, String>, ForgeError> {
    build_run_variables_with_summarizer(config, command, forge_root, date, summarize_task)
}

fn build_run_variables_with_summarizer(
    config: &ForgeConfig,
    command: &Commands,
    forge_root: &Path,
    date: &str,
    summarizer: fn(&str) -> Option<TaskSummary>,
) -> Result<BTreeMap<String, String>, ForgeError> {
    match command {
        Commands::Run {
            blueprint_name,
            blueprint,
            repo,
            task,
            instruction: _,
            issue,
            round,
            pr,
            agent,
            model,
            branch,
            vars,
            ..
        } => {
            let mut values = BTreeMap::new();

            insert_if_some(&mut values, "target_agent", config.defaults.agent.clone());
            insert_if_some(&mut values, "target_model", config.defaults.model.clone());
            insert_if_some(
                &mut values,
                "project_type",
                config.project.project_type.clone(),
            );
            insert_if_some(&mut values, "project_name", config.project.name.clone());
            insert_if_some(&mut values, "test_command", config.commands.test.clone());
            insert_if_some(&mut values, "lint_command", config.commands.lint.clone());
            insert_if_some(&mut values, "build_command", config.commands.build.clone());

            for (repo_name, repo_config) in &config.repos {
                insert_repo_path_aliases(&mut values, repo_name, &repo_config.path);
            }

            if let Some(repo_name) = repo {
                let repo_config = config.repos.get(repo_name).ok_or_else(|| {
                    ForgeError::message(format!("unknown repo `{repo_name}` in forge.toml"))
                })?;
                values.insert("target_repo".to_string(), repo_name.clone());
                values.insert("target_repo_path".to_string(), repo_config.path.clone());
                values.insert("repo_path".to_string(), repo_config.path.clone());
                insert_if_some(&mut values, "crate", repo_config.crate_name.clone());
                values.insert(
                    "test_blueprint".to_string(),
                    repo_config.test_blueprint.clone().unwrap_or_default(),
                );
                insert_if_some(&mut values, "target_agent", repo_config.agent.clone());
                insert_if_some(&mut values, "target_model", repo_config.model.clone());
            }

            values.insert("date".to_string(), date.to_string());
            values.insert(
                "forge_path".to_string(),
                forge_root.to_string_lossy().into_owned(),
            );
            values.insert("instruction_file".to_string(), "current.md".to_string());
            values.insert(
                "instruction_path".to_string(),
                format!(".forge/{}/current.md", config.workspace_instructions_dir()),
            );

            insert_if_some(&mut values, "task", task.clone());
            insert_if_some(&mut values, "task_description", task.clone());
            insert_if_some(&mut values, "issue", issue.clone());
            insert_if_some(&mut values, "issue_id", issue.clone());
            insert_if_some(&mut values, "round", round.clone());
            insert_if_some(&mut values, "pr", pr.clone());
            insert_if_some(&mut values, "target_agent", agent.clone());
            insert_if_some(&mut values, "target_model", model.clone());

            let resolved_blueprint = resolve_blueprint_name(blueprint_name, blueprint);
            let task_summary = if branch.is_none() {
                task.as_deref().and_then(summarizer)
            } else {
                None
            };
            let branch_value = branch.clone().unwrap_or_else(|| {
                resolve_branch_value(resolved_blueprint, task.as_deref(), task_summary.as_ref())
            });
            values.insert("branch".to_string(), branch_value.clone());
            values.insert(
                "commit_message".to_string(),
                resolve_commit_message(task_summary.as_ref(), Some(branch_value.as_str())),
            );

            for (key, value) in vars {
                values.insert(key.clone(), value.clone());
            }

            Ok(values)
        }
        _ => Err(ForgeError::message(
            "run variables can only be built for the `run` command",
        )),
    }
}

pub fn auto_branch_name(
    blueprint: &str,
    task: Option<&str>,
    _issue: Option<&str>,
    _round: Option<&str>,
    _date: &str,
) -> String {
    let hash = task_hash7(task.unwrap_or(""));
    branch_name_from_slug(blueprint, &hash)
}

pub fn resolve_blueprint_for_run(
    blueprint_root: &Path,
    blueprint: &str,
    repo: Option<&str>,
) -> Result<PathBuf, ForgeError> {
    let mut candidates = vec![
        blueprint_root
            .join("common")
            .join(format!("{blueprint}.toml")),
    ];
    if let Some(repo) = repo {
        candidates.push(blueprint_root.join(repo).join(format!("{blueprint}.toml")));
    }
    candidates.push(blueprint_root.join(format!("{blueprint}.toml")));

    for candidate in candidates {
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    Err(ForgeError::message(format!(
        "unable to resolve blueprint `{blueprint}`"
    )))
}

fn insert_if_some(values: &mut BTreeMap<String, String>, key: &str, value: Option<String>) {
    if let Some(value) = value {
        values.insert(key.to_string(), value);
    }
}

fn resolve_blueprint_name<'a>(
    blueprint_name: &'a Option<String>,
    blueprint: &'a Option<String>,
) -> &'a str {
    blueprint_name
        .as_deref()
        .or_else(|| blueprint.as_deref())
        .unwrap_or("run")
}

fn insert_repo_path_aliases(values: &mut BTreeMap<String, String>, repo_name: &str, path: &str) {
    let normalized = repo_name.replace('-', "_");
    values.insert(format!("{normalized}_path"), path.to_string());
    if let Some(short) = normalized.strip_prefix("warrant_") {
        values.insert(format!("{short}_path"), path.to_string());
    }
}

fn expand_home(path: &str) -> Result<String, ForgeError> {
    if path == "~" || path.starts_with("~/") {
        let home = std::env::var("HOME")
            .map_err(|_| ForgeError::message("HOME must be set to expand `~` paths"))?;
        if path == "~" {
            return Ok(home);
        }
        return Ok(format!("{home}/{}", &path[2..]));
    }

    Ok(path.to_string())
}

fn resolve_branch_value(
    blueprint: &str,
    task: Option<&str>,
    summary: Option<&TaskSummary>,
) -> String {
    match summary {
        Some(summary) => branch_name_from_slug(blueprint, &summary.branch_slug),
        None => auto_branch_name(blueprint, task, None, None, ""),
    }
}

fn resolve_commit_message(summary: Option<&TaskSummary>, branch: Option<&str>) -> String {
    summary
        .map(|summary| summary.commit_message.clone())
        .or_else(|| branch.map(ToOwned::to_owned))
        .unwrap_or_else(|| "chore: update code".to_string())
}

fn branch_name_from_slug(blueprint: &str, slug: &str) -> String {
    match blueprint {
        "implement-feature" | "new-feature" => format!("feat/{slug}"),
        "fix-bug" => format!("fix/{slug}"),
        _ => format!("work/{slug}"),
    }
}

fn task_hash7(task: &str) -> String {
    let mut hasher = Fnv1a64::default();
    hasher.write(task.as_bytes());
    format!("{:016x}", hasher.finish())[..7].to_string()
}

#[derive(Default)]
struct Fnv1a64(u64);

impl Hasher for Fnv1a64 {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        if self.0 == 0 {
            self.0 = 0xcbf29ce484222325;
        }
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x100000001b3);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{auto_branch_name, build_run_variables_with_summarizer, load_forge_config_str};
    use crate::cli::Commands;
    use crate::summarize::TaskSummary;
    use std::path::Path;

    #[test]
    fn build_run_variables_uses_ai_slug_for_branch_and_commit_message() {
        fn summarizer(_: &str) -> Option<TaskSummary> {
            Some(TaskSummary {
                branch_slug: "add-admin-api".to_string(),
                commit_message: "feat: add admin api".to_string(),
            })
        }

        let config = load_forge_config_str("").expect("config should parse");
        let command = Commands::Run {
            blueprint_name: Some("new-feature".to_string()),
            blueprint: None,
            repo: None,
            task: Some("Add Admin API".to_string()),
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

        let variables = build_run_variables_with_summarizer(
            &config,
            &command,
            Path::new("/work/forge"),
            "2026-04-04",
            summarizer,
        )
        .expect("variables");

        assert_eq!(
            variables.get("branch").map(String::as_str),
            Some("feat/add-admin-api")
        );
        assert_eq!(
            variables.get("commit_message").map(String::as_str),
            Some("feat: add admin api")
        );
    }

    #[test]
    fn build_run_variables_falls_back_to_hashed_branch_and_branch_commit_message() {
        fn summarizer(_: &str) -> Option<TaskSummary> {
            None
        }

        let config = load_forge_config_str("").expect("config should parse");
        let command = Commands::Run {
            blueprint_name: Some("fix-bug".to_string()),
            blueprint: None,
            repo: None,
            task: Some("Fix websocket reconnect timing".to_string()),
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

        let variables = build_run_variables_with_summarizer(
            &config,
            &command,
            Path::new("/work/forge"),
            "2026-04-04",
            summarizer,
        )
        .expect("variables");

        assert_eq!(
            variables.get("branch").map(String::as_str),
            Some("fix/0766bdf")
        );
        assert_eq!(
            variables.get("commit_message").map(String::as_str),
            Some("fix/0766bdf")
        );
    }

    #[test]
    fn build_run_variables_uses_explicit_branch_without_summarizing() {
        fn summarizer(_: &str) -> Option<TaskSummary> {
            panic!("explicit branches should bypass the AI summarizer")
        }

        let config = load_forge_config_str("").expect("config should parse");
        let command = Commands::Run {
            blueprint_name: Some("new-feature".to_string()),
            blueprint: None,
            repo: None,
            task: Some("Add Admin API".to_string()),
            instruction: None,
            issue: None,
            round: None,
            pr: None,
            next: false,
            latest: false,
            agent: None,
            model: None,
            branch: Some("feat/custom-branch".to_string()),
            dry_run: false,
            no_dashboard: false,
            port: 8400,
            notify: Vec::new(),
            verbose: false,
            vars: Vec::new(),
        };

        let variables = build_run_variables_with_summarizer(
            &config,
            &command,
            Path::new("/work/forge"),
            "2026-04-04",
            summarizer,
        )
        .expect("variables");

        assert_eq!(
            variables.get("branch").map(String::as_str),
            Some("feat/custom-branch")
        );
        assert_eq!(
            variables.get("commit_message").map(String::as_str),
            Some("feat/custom-branch")
        );
    }

    #[test]
    fn auto_branch_name_uses_generic_hashed_prefixes() {
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
            auto_branch_name(
                "fix-bug",
                Some("Repair websocket jitter"),
                None,
                None,
                "2026-03-22"
            ),
            "fix/509ff7b"
        );
        assert_eq!(
            auto_branch_name("lint", Some("Clean warnings"), None, None, "2026-03-22"),
            "work/5a89a37"
        );
    }
}
