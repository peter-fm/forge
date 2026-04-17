---
command: forge
description: Use forge development guardrails when working on a project. Check for .forge/ directory, write task instructions, run blueprints with lint and test gates.
---

# Forge — Development Guardrails

When you're asked to build a feature, fix a bug, or refactor code in a project, check for a `.forge/` directory first.

## Workflow

### 1. Check for `.forge/`

```bash
ls .forge/config.toml 2>/dev/null
```

If `.forge/` exists, use it. If not, you can create it:

```bash
forge init
```

### 2. Read the project config

```bash
cat .forge/config.toml
```

This tells you: project type, test command, lint command, build command, default agent.

### 3. List available blueprints

```bash
forge list
```

Common blueprints:
- `build` — implement a task (feature or refactor) with lint + test gates, then a docs check before PR creation
- `fix-bug` — fix a bug with regression test verification, then a docs check before PR creation
- `phase` — execute one phase of multi-phase work on a shared branch (no PR yet)
- `finalize` — run final gates and open the PR for multi-phase work
- `pr-review` — senior-engineer review of an open PR
- `code-review` — review a PR and post feedback via GitHub

### 4. Write task instructions

Read `.forge/INSTRUCTION_GUIDE.md` before writing an instruction file — it defines the eight-section problem-focused shape forge agents expect. For small ad-hoc runs a short brief is fine, but for anything architectural follow the guide.

Instruction files live in `.forge/instructions/<slug>.md`. Forge hands the file directly to the implementing agent — there is no summariser in the middle, so the file has to stand alone.

### 5. Run the blueprint

```bash
forge run build --task "Add dark mode toggle to settings"
```

Or with an instruction file you've authored:

```bash
forge run build --instruction dark-mode.md --var commit_message="feat: dark mode"
```

For multi-phase work:

```bash
forge run phase \
  --instruction phase-1-stable-ids.md \
  --var phase_branch=refactor/memory-ids \
  --var commit_message="feat(memory): phase 1 — stable IDs"

# After the last phase:
forge run finalize \
  --var phase_branch=refactor/memory-ids \
  --var commit_message="refactor: memory subsystem"
```

### 6. Check results

```bash
forge status
```

## When to Use Forge

**Use forge when:**
- `.forge/` exists in the project
- You're implementing a feature, fixing a bug, or refactoring
- The task involves code changes that should be tested

**Don't use forge when:**
- The task is just reading/analysing code
- You're making a one-line config change
- No `.forge/` exists and the project is too small to benefit

## Important

- Always write clear instructions before running a blueprint
- Don't bypass forge gates by editing code directly when a blueprint is available
- If a blueprint fails after retries, report what went wrong — don't silently give up
- Expect branching blueprints to include a `docs-check` step after verification gates and before PR creation
- Check `forge status` and report the outcome to the user

## Blueprint Authoring

Use this when you need to create or extend `.forge/blueprints/*.toml`, not just run an existing one.

### TOML Schema Reference

`[blueprint]`
- `name`: blueprint name, normally matching the filename without `.toml`
- `description`: short human-readable summary shown by `forge list`

`[[step]]`
- `type`: one of `deterministic`, `agentic`, `gate`, `conditional`, `blueprint`
- `name`: unique step identifier within the blueprint
- `command`: shell command for `deterministic`, `gate`, and `conditional` steps
- `agent`: agent name for `agentic` steps, typically `codex` or `claude-code`
- `model`: optional model override for an `agentic` step
- `prompt`: prompt text for an `agentic` step; supports `{variables}`
- `blueprint`: sub-blueprint name for `blueprint` steps
- `params`: inline table mapping variables passed into a sub-blueprint
- `condition`: boolean expression controlling whether the step runs
- `sets`: variable name populated by a `conditional` step from its exit code
- `allow_failure`: if `true`, the workflow continues even when the step fails
- `expect_failure`: if `true`, a non-zero exit code is treated as success
- `max_retries`: retry count for `agentic` steps
- `env`: inline table of extra environment variables for that step

### Available Variables

- Config commands: `{test_command}`, `{lint_command}`, `{build_command}`
- CLI flags: `{instruction_path}`, `{branch}`, `{issue}`, `{pr}`, `{round}`
- `--var` overrides: `{key}` for any `--var key=value`
- Auto-generated: `{date}`, `{forge_path}`
- Multi-repo config: `{repo_name_path}` for each configured repo
- Step results: `{step_name.exit_code}`, `{step_name.log_file}`, `{step_name_output}`
- Agent selection: `{target_agent}`, `{target_model}`

### Design Principles

1. Deterministic gates before agentic steps. Validate the current state before asking an agent to continue.
2. Agentic steps should be retryable. Set `max_retries` so the agent can recover from failed gates.
3. Use sub-blueprints for reusable sequences. Shared lint, test, or setup flows belong in child blueprints.
4. Include a docs-check step in branching workflows after verification gates and before PR creation. The step should review `README.md`, `docs/`, and `AGENTS.md`, and `allow_failure = true` so the branch can still proceed if nothing needs updating or the check fails.
5. End branching workflows with PR creation. Feature branches should finish by opening a PR, not by stopping after a commit.
6. Keep prompts specific. Tell the agent which files, commands, and outputs matter.

### Worked Example

```toml
[blueprint]
name = "security-audit"
description = "Audit recent changes for security regressions and report findings"

[[step]]
type = "deterministic"
name = "build"
command = "{build_command}"

[[step]]
type = "agentic"
name = "audit"
agent = "{target_agent}"
model = "{target_model}"
prompt = """Review the current branch for security issues.

1. Run `git diff main...HEAD` to scope the recent changes.
2. Inspect authentication, authorization, and secret handling code paths.
3. Check inputs that reach shells, SQL, templates, file paths, or network calls.
4. Look for missing validation, unsafe defaults, and logging of sensitive data.
5. Write findings to `.forge/instructions/security-audit-report.md` with severity, evidence, and recommended fixes.
"""
max_retries = 1

[[step]]
type = "deterministic"
name = "test"
command = "{test_command}"

[[step]]
type = "deterministic"
name = "lint"
command = "{lint_command}"
allow_failure = true
```
