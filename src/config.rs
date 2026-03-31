use crate::cli::Commands;
use crate::error::ForgeError;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ForgeConfig {
    pub defaults: Defaults,
    pub repos: BTreeMap<String, RepoConfig>,
    pub routing: Vec<RoutingRule>,
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
    defaults: Defaults,
    #[serde(default)]
    repos: BTreeMap<String, RawRepoConfig>,
    #[serde(default)]
    routing: Vec<RoutingRule>,
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
        defaults: raw.defaults,
        repos,
        routing: raw.routing,
    })
}

pub fn build_run_variables(
    config: &ForgeConfig,
    command: &Commands,
    forge_root: &Path,
    date: &str,
) -> Result<BTreeMap<String, String>, ForgeError> {
    match command {
        Commands::Run {
            blueprint,
            repo,
            task,
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

            insert_if_some(&mut values, "task", task.clone());
            insert_if_some(&mut values, "task_description", task.clone());
            insert_if_some(&mut values, "issue", issue.clone());
            insert_if_some(&mut values, "issue_id", issue.clone());
            insert_if_some(&mut values, "round", round.clone());
            insert_if_some(&mut values, "pr", pr.clone());
            insert_if_some(&mut values, "target_agent", agent.clone());
            insert_if_some(&mut values, "target_model", model.clone());

            let branch_value = branch.clone().unwrap_or_else(|| {
                auto_branch_name(
                    blueprint,
                    task.as_deref(),
                    issue.as_deref(),
                    round.as_deref(),
                    date,
                )
            });
            values.insert("branch".to_string(), branch_value);

            if let Some(task) = task {
                values.insert("commit_message".to_string(), task.clone());
            }

            for (key, value) in vars {
                values.insert(key.clone(), value.clone());
            }

            Ok(values)
        }
    }
}

pub fn auto_branch_name(
    blueprint: &str,
    task: Option<&str>,
    issue: Option<&str>,
    round: Option<&str>,
    date: &str,
) -> String {
    match blueprint {
        "code-review" => format!("refactor/code-review-{date}"),
        "implement-feature" => format!("feat/{}", slugify(task.unwrap_or("work"), 40)),
        "fix-bug" => format!("fix/{}", issue.unwrap_or("unknown")),
        "red-team" => format!("red-team/round-{}", round.unwrap_or("1")),
        _ => format!("forge/{blueprint}-{date}"),
    }
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

fn slugify(input: &str, max_len: usize) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash && !slug.is_empty() {
            slug.push('-');
            last_dash = true;
        }
        if slug.len() >= max_len {
            break;
        }
    }

    slug.truncate(max_len);
    while slug.ends_with('-') {
        slug.pop();
    }

    if slug.is_empty() {
        "work".to_string()
    } else {
        slug
    }
}
