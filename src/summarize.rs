use serde::Deserialize;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

const CLAUDE_CLI_TIMEOUT: Duration = Duration::from_secs(30);
const OPENAI_CHAT_COMPLETIONS_URL: &str = "https://api.openai.com/v1/chat/completions";
const OPENAI_MODEL: &str = "gpt-4o-mini";
const ANTHROPIC_MESSAGES_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_MODEL: &str = "claude-3-5-haiku-latest";

type Provider = fn(&str) -> Result<TaskSummary, String>;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TaskSummary {
    pub branch_slug: String,
    pub commit_message: String,
}

pub fn summarize_task(task: &str) -> Option<TaskSummary> {
    let providers: [Provider; 4] = [
        try_claude_cli,
        try_codex_auth_file,
        try_openai_env,
        try_anthropic_env,
    ];
    summarize_task_with_providers(task, &providers, true)
}

fn summarize_task_with_providers(
    task: &str,
    providers: &[Provider],
    log_failures: bool,
) -> Option<TaskSummary> {
    for provider in providers {
        match provider(task) {
            Ok(summary) => return Some(summary),
            Err(error) => {
                if log_failures {
                    eprintln!("note: {error}");
                }
            }
        }
    }
    None
}

fn try_claude_cli(task: &str) -> Result<TaskSummary, String> {
    let prompt = build_summary_prompt(task);
    let mut child = Command::new("claude")
        .arg("-p")
        .arg(prompt)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Claude CLI summarizer failed to start: {error}"))?;

    let started_at = Instant::now();
    loop {
        if started_at.elapsed() >= CLAUDE_CLI_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "Claude CLI summarizer timed out after {}s",
                CLAUDE_CLI_TIMEOUT.as_secs()
            ));
        }

        match child.try_wait() {
            Ok(Some(_)) => {
                let output = child.wait_with_output().map_err(|error| {
                    format!("Claude CLI summarizer failed to read output: {error}")
                })?;
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                    return Err(format!(
                        "Claude CLI summarizer exited with {}{}",
                        output.status,
                        format_error_detail(&stderr)
                    ));
                }
                let stdout = String::from_utf8_lossy(&output.stdout);
                return parse_task_summary(&stdout).map_err(|error| {
                    format!("Claude CLI summarizer returned invalid JSON: {error}")
                });
            }
            Ok(None) => sleep(Duration::from_millis(100)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "Claude CLI summarizer failed while waiting: {error}"
                ));
            }
        }
    }
}

fn try_codex_auth_file(task: &str) -> Result<TaskSummary, String> {
    let path = codex_auth_path()?;
    let input = fs::read_to_string(&path).map_err(|error| {
        format!(
            "Codex auth summarizer could not read {}: {error}",
            path.display()
        )
    })?;
    let auth: CodexAuthFile = serde_json::from_str(&input).map_err(|error| {
        format!(
            "Codex auth summarizer could not parse {}: {error}",
            path.display()
        )
    })?;
    let api_key = auth.openai_api_key.ok_or_else(|| {
        format!(
            "Codex auth summarizer did not find OPENAI_API_KEY in {}",
            path.display()
        )
    })?;
    call_openai_api(task, &api_key, "Codex auth")
}

fn try_openai_env(task: &str) -> Result<TaskSummary, String> {
    let api_key = std::env::var("OPENAI_API_KEY")
        .map_err(|_| "OpenAI env summarizer missing OPENAI_API_KEY".to_string())?;
    call_openai_api(task, &api_key, "OpenAI env")
}

fn try_anthropic_env(task: &str) -> Result<TaskSummary, String> {
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .map_err(|_| "Anthropic env summarizer missing ANTHROPIC_API_KEY".to_string())?;
    call_anthropic_api(task, &api_key)
}

fn call_openai_api(task: &str, api_key: &str, source: &str) -> Result<TaskSummary, String> {
    let response = ureq::post(OPENAI_CHAT_COMPLETIONS_URL)
        .set("Authorization", &format!("Bearer {api_key}"))
        .set("Content-Type", "application/json")
        .send_json(serde_json::json!({
            "model": OPENAI_MODEL,
            "temperature": 0,
            "messages": [
                {
                    "role": "system",
                    "content": "Return only JSON with branch_slug and commit_message."
                },
                {
                    "role": "user",
                    "content": build_summary_prompt(task)
                }
            ]
        }))
        .map_err(|error| format!("{source} summarizer request failed: {error}"))?;

    let payload: OpenAIResponse = response
        .into_json()
        .map_err(|error| format!("{source} summarizer response parse failed: {error}"))?;
    let content = payload
        .choices
        .into_iter()
        .next()
        .map(|choice| choice.message.content)
        .ok_or_else(|| format!("{source} summarizer returned no choices"))?;
    parse_task_summary(&content)
        .map_err(|error| format!("{source} summarizer returned invalid JSON: {error}"))
}

fn call_anthropic_api(task: &str, api_key: &str) -> Result<TaskSummary, String> {
    let response = ureq::post(ANTHROPIC_MESSAGES_URL)
        .set("x-api-key", api_key)
        .set("anthropic-version", "2023-06-01")
        .set("Content-Type", "application/json")
        .send_json(serde_json::json!({
            "model": ANTHROPIC_MODEL,
            "max_tokens": 200,
            "temperature": 0,
            "messages": [
                {
                    "role": "user",
                    "content": build_summary_prompt(task)
                }
            ]
        }))
        .map_err(|error| format!("Anthropic env summarizer request failed: {error}"))?;

    let payload: AnthropicResponse = response
        .into_json()
        .map_err(|error| format!("Anthropic env summarizer response parse failed: {error}"))?;
    let content = payload
        .content
        .into_iter()
        .find(|block| block.kind == "text")
        .map(|block| block.text)
        .ok_or_else(|| "Anthropic env summarizer returned no text block".to_string())?;
    parse_task_summary(&content)
        .map_err(|error| format!("Anthropic env summarizer returned invalid JSON: {error}"))
}

fn parse_task_summary(input: &str) -> Result<TaskSummary, String> {
    let stripped = strip_markdown_fences(input);
    let mut summary: TaskSummary =
        serde_json::from_str(&stripped).map_err(|error| error.to_string())?;
    summary.branch_slug = normalize_branch_slug(&summary.branch_slug);
    summary.commit_message = summary.commit_message.trim().to_string();
    if summary.commit_message.is_empty() {
        return Err("commit_message must not be empty".to_string());
    }
    Ok(summary)
}

pub(crate) fn normalize_branch_slug(input: &str) -> String {
    let leaf = input.rsplit('/').next().unwrap_or(input);
    let mut slug = String::new();
    let mut last_hyphen = false;

    for ch in leaf.chars() {
        if ch.is_ascii_alphanumeric() {
            if slug.len() == 40 {
                break;
            }
            slug.push(ch.to_ascii_lowercase());
            last_hyphen = false;
        } else if !last_hyphen && !slug.is_empty() {
            if slug.len() == 40 {
                break;
            }
            slug.push('-');
            last_hyphen = true;
        }
    }

    while slug.ends_with('-') {
        slug.pop();
    }

    if slug.is_empty() {
        "work".to_string()
    } else {
        slug
    }
}

fn strip_markdown_fences(input: &str) -> String {
    let trimmed = input.trim();
    if !trimmed.starts_with("```") {
        return trimmed.to_string();
    }

    let mut lines = trimmed.lines();
    let Some(first_line) = lines.next() else {
        return String::new();
    };
    if !first_line.trim_start().starts_with("```") {
        return trimmed.to_string();
    }

    let mut body = lines.collect::<Vec<_>>();
    if body
        .last()
        .is_some_and(|line| line.trim_start().starts_with("```"))
    {
        body.pop();
    }

    body.join("\n").trim().to_string()
}

fn build_summary_prompt(task: &str) -> String {
    format!(
        "Generate a git branch slug and commit message for this coding task.\n\
Return JSON only with this exact shape:\n\
{{\"branch_slug\":\"kebab-case-max-40-chars\",\"commit_message\":\"conventional commit max 72 chars\"}}\n\
Rules:\n\
- branch_slug must be lowercase kebab-case, at most 40 characters, with no prefix like feat/ or fix/\n\
- commit_message must be a single conventional commit line, at most 72 characters\n\
- do not include markdown fences or commentary\n\
Task:\n{task}"
    )
}

fn codex_auth_path() -> Result<PathBuf, String> {
    let home = std::env::var("HOME").map_err(|_| {
        "Codex auth summarizer requires HOME to locate ~/.codex/auth.json".to_string()
    })?;
    Ok(PathBuf::from(home).join(".codex/auth.json"))
}

fn format_error_detail(detail: &str) -> String {
    if detail.is_empty() {
        String::new()
    } else {
        format!(": {detail}")
    }
}

#[derive(Debug, Deserialize)]
struct CodexAuthFile {
    #[serde(rename = "OPENAI_API_KEY", alias = "openai_api_key", alias = "api_key")]
    openai_api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIResponse {
    choices: Vec<OpenAIChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAIChoice {
    message: OpenAIMessage,
}

#[derive(Debug, Deserialize)]
struct OpenAIMessage {
    content: String,
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContentBlock>,
}

#[derive(Debug, Deserialize)]
struct AnthropicContentBlock {
    #[serde(rename = "type")]
    kind: String,
    text: String,
}

#[cfg(test)]
mod tests {
    use super::{
        Provider, TaskSummary, normalize_branch_slug, parse_task_summary, strip_markdown_fences,
        summarize_task_with_providers,
    };

    #[test]
    fn strips_markdown_fences_before_json_parsing() {
        let summary = parse_task_summary(
            r#"
            ```json
            {"branch_slug":"feat/Add API Endpoint!!","commit_message":"feat: add api endpoint"}
            ```
            "#,
        )
        .expect("summary should parse");

        assert_eq!(summary.branch_slug, "add-api-endpoint");
        assert_eq!(summary.commit_message, "feat: add api endpoint");
    }

    #[test]
    fn strip_markdown_fences_leaves_plain_json_unchanged() {
        assert_eq!(
            strip_markdown_fences(r#"{"branch_slug":"demo","commit_message":"feat: demo"}"#),
            r#"{"branch_slug":"demo","commit_message":"feat: demo"}"#
        );
    }

    #[test]
    fn normalize_branch_slug_removes_prefixes_and_caps_length() {
        assert_eq!(
            normalize_branch_slug("feat/Add API Endpoint for Admin Dashboard!!!"),
            "add-api-endpoint-for-admin-dashboard"
        );
        assert_eq!(
            normalize_branch_slug("work/THIS___IS___A___VERY___LONG___BRANCH___NAME___123"),
            "this-is-a-very-long-branch-name-123"
        );
    }

    #[test]
    fn parse_task_summary_rejects_empty_commit_message() {
        let error = parse_task_summary(r#"{"branch_slug":"demo","commit_message":"   "}"#)
            .expect_err("summary should fail");
        assert!(error.contains("commit_message"));
    }

    #[test]
    fn fallback_logic_uses_first_successful_provider() {
        fn fail(_: &str) -> Result<TaskSummary, String> {
            Err("failed".to_string())
        }
        fn succeed(_: &str) -> Result<TaskSummary, String> {
            Ok(TaskSummary {
                branch_slug: "demo-branch".to_string(),
                commit_message: "feat: demo".to_string(),
            })
        }
        fn should_not_run(_: &str) -> Result<TaskSummary, String> {
            panic!("provider chain should stop after success")
        }

        let providers: [Provider; 3] = [fail, succeed, should_not_run];
        let summary =
            summarize_task_with_providers("demo task", &providers, false).expect("summary");

        assert_eq!(
            summary,
            TaskSummary {
                branch_slug: "demo-branch".to_string(),
                commit_message: "feat: demo".to_string(),
            }
        );
    }

    #[test]
    fn fallback_logic_returns_none_when_all_providers_fail() {
        fn fail_one(_: &str) -> Result<TaskSummary, String> {
            Err("failed one".to_string())
        }
        fn fail_two(_: &str) -> Result<TaskSummary, String> {
            Err("failed two".to_string())
        }

        let providers: [Provider; 2] = [fail_one, fail_two];
        assert!(summarize_task_with_providers("demo task", &providers, false).is_none());
    }
}
