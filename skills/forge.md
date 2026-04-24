---
name: forge
description: >
  Use forge development guardrails when working on a project. Check for a
  .forge/ directory, read its config, pick a blueprint, write an instruction
  file, then run the blueprint so lint and test gates apply. Triggers: "forge",
  "forge run", "forge init", ".forge/", "blueprint", "build blueprint",
  "fix-bug blueprint", "pr-review", "review-codebase".
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
- `open-pr` — run final gates and open the PR for multi-phase work
- `pr-review` — senior-engineer review of an open PR, then merge and run post-merge lint/test
- `review-codebase` — sweep the codebase for dead code, unused exports, stray TODOs, placeholder stubs, and inconsistent patterns; writes findings to `.forge/instructions/review-codebase-<date>.md` for later triage (does not modify code)

### 4. Pick a blueprint by the shape of the deliverable

Choose based on what should land, not the kind of work:

- **One commit, one PR → `build`.** Features, bug fixes, and refactors that fit in a single reviewable PR.
- **Multiple commits across multiple sessions, one PR at the end → `phase` per session, then `open-pr` once.** Use this when the work is too large to land in one reviewable PR but you still want a single PR for the reviewer. Each `phase` run commits to a shared branch without opening a PR; `open-pr` runs the final gates and opens the PR.
- **Bug with a regression test → `fix-bug`.** Like `build` but prompts for the regression test first.
- **Reviewing an existing PR → `pr-review`.** Not for writing new code.
- **Sweeping for hygiene issues → `review-codebase`.** Produces an instruction file of findings; does not modify code.

If you have an explicit multi-phase plan, do NOT default to `build` — it will open a PR after phase 1. Use `phase` + `open-pr`.

### 5. Write task instructions

Read `.forge/INSTRUCTION_GUIDE.md` before writing an instruction file — it defines the eight-section problem-focused shape forge agents expect. Required reading for anything spanning more than a single file or commit; a short brief is only fine for truly trivial ad-hoc runs.

Instruction files live in `.forge/instructions/<slug>.md`. Forge hands the file directly to the implementing agent — there is no summariser in the middle, so the file has to stand alone.

### 6. Run the blueprint

**Single PR (most work):**

```bash
forge run build --instruction dark-mode.md --var commit_message="feat: dark mode"
```

Or for a quick ad-hoc run without an instruction file:

```bash
forge run build --task "Add dark mode toggle to settings"
```

**Bug fix with regression test:**

```bash
forge run fix-bug --instruction missing-null-check.md --var commit_message="fix: null check on user.email"
```

**Multi-phase refactor (shared branch, one PR at the end):**

```bash
# One run per phase — same phase_branch across all of them:
forge run phase \
  --instruction phase-1-stable-ids.md \
  --var phase_branch=refactor/memory-ids \
  --var commit_message="feat(memory): phase 1 — stable IDs"

forge run phase \
  --instruction phase-2-storage.md \
  --var phase_branch=refactor/memory-ids \
  --var commit_message="feat(memory): phase 2 — storage layer"

# After the final phase, open the PR:
forge run open-pr \
  --var phase_branch=refactor/memory-ids \
  --var commit_message="refactor: memory subsystem"
```

**Review an open PR:**

```bash
forge run pr-review --pr 123
```

### 7. Check results

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
