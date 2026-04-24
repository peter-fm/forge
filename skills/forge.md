---
name: forge
description: >
  Act as a high-level orchestrator for a coding task, using forge to carry it
  from plan to merge autonomously. On invocation, enter plan mode with the
  user; once the plan is agreed, drive each phase, instruction file, PR, and
  merge through forge blueprints without stopping to ask unless a genuine
  decision is required. Triggers: "forge", "forge run", "forge init",
  ".forge/", "blueprint", "build blueprint", "fix-bug blueprint", "pr-review",
  "review-codebase".
---

# Forge — Orchestrated Development Guardrails

You are the **orchestrator**. Forge is your execution layer. The user's goal is to go from a shared plan all the way to a merged PR on main, with forge running every step that touches code. You stay in the loop to think, adjust, and hand work to forge — you do not stop to ask permission between phases.

## The orchestration contract

When this skill is invoked:

1. **Enter plan mode with the user.** Explore the problem, ask questions, agree on scope and shape. Do not start coding in plan mode.
2. **Once the plan is agreed, take full control.** Leave plan mode and run the plan to completion through forge. Do not pause for routine confirmations — the plan itself is the authorization.
3. **Use forge for every code-touching step.** Instruction authoring is your job; code changes, gates, PR creation, review, and merge are forge's job.
4. **Only stop for genuine decisions.** Ambiguity that the plan doesn't cover, a gate failing in a way that changes the plan, a destructive or irreversible action. Not "should I run the next phase?" — just run it.
5. **Ideal path: plan mode → merged to main, uninterrupted.**

### What "orchestrator" means in practice

- You write instruction files. Forge agents implement from them.
- You read forge's output between steps, compare it against the plan, and adjust the next instruction file accordingly — including revising the plan if the previous phase revealed something.
- You run `forge status` after each step and interpret the result.
- You do not hand-edit source files to "help forge along." If a phase needs a tweak, write it into the next instruction file or run another forge phase.

## Step 0 — Plan mode with the user

On invocation, switch to plan mode (`EnterPlanMode`) and work with the user to define:

- **Problem & context** — what's broken, what's missing, why it matters.
- **Desired end state** — observable outcomes, not file lists.
- **Shape of delivery** — single PR, or multi-phase with one PR at the end.
- **Load-bearing constraints** — anything that must be true that the agent cannot decide for itself.
- **Out of scope** — explicit don't-touch list.

Ask clarifying questions now. Once the plan is out of plan mode and exits successfully, you own it end-to-end.

If the project has no `.forge/`, offer to run `forge init` as part of the plan. If forge genuinely doesn't fit (pure read/analysis, one-line config tweak), say so in plan mode and fall back to direct edits — but be honest about it.

## Step 1 — Read the project's forge setup

```bash
ls .forge/config.toml 2>/dev/null
cat .forge/config.toml
forge list
```

Config tells you project type, test/lint/build commands, default agent. `forge list` shows available blueprints.

Common blueprints:
- `build` — implement a task (feature or refactor) with lint + test gates, then a docs check before PR creation
- `fix-bug` — fix a bug with regression test verification, then a docs check before PR creation
- `phase` — execute one phase of multi-phase work on a shared branch (no PR yet)
- `open-pr` — run final gates and open the PR for multi-phase work
- `pr-review` — senior-engineer review of an open PR, then merge and run post-merge lint/test
- `review-codebase` — sweep the codebase for dead code, unused exports, stray TODOs, placeholder stubs, and inconsistent patterns; writes findings to `.forge/instructions/review-codebase-<date>.md` (does not modify code)

## Step 2 — Pick the blueprint by the shape of the deliverable

- **One commit, one PR → `build`.** Single reviewable PR for features, bug fixes, refactors.
- **Multi-phase, one PR at the end → `phase` per session, then `open-pr` once.** Shared branch across runs.
- **Bug with a regression test → `fix-bug`.** Prompts for the regression test first.
- **Reviewing an existing PR → `pr-review`.** Not for new code.
- **Hygiene sweep → `review-codebase`.** Produces findings; does not modify code.

If the plan is explicitly multi-phase, do NOT default to `build` — it opens a PR after phase 1. Use `phase` + `open-pr`.

## Step 3 — Write the instruction file

**Required reading before writing:** `.forge/INSTRUCTION_GUIDE.md` in the project. It defines the eight-section problem-focused shape forge agents expect. Read it every time — don't assume you remember it.

Instruction files live in `.forge/instructions/<slug>.md`. Forge hands the file directly to the implementing agent with no summariser in the middle, so the file must stand alone.

Key discipline:
- Describe WHAT is true when done, not HOW to get there.
- No invented file paths, function signatures, or SQL DDL.
- Load-bearing constraints only — things the agent cannot decide for itself.
- Runnable acceptance criteria against the repo state at close.

## Step 4 — Run forge, autonomously

### Single-PR work (`build` / `fix-bug`)

```bash
forge run build --instruction <slug>.md --var commit_message="<message>"
# or
forge run fix-bug --instruction <slug>.md --var commit_message="<message>"
```

When it finishes:
1. Run `forge status`.
2. Read the output against the plan.
3. If the PR is open, proceed to Step 5 (review and merge) — do not stop to ask.
4. If a gate failed, diagnose and either re-run, write a follow-up instruction, or stop for a user decision only if the failure changes the plan.

### Multi-phase work (`phase` + `open-pr`) — the orchestration loop

For each phase, in order:

1. **Write the phase instruction file** (reading INSTRUCTION_GUIDE.md first). Scope it to just this phase.
2. **Kick off the phase:**

   ```bash
   forge run phase \
     --instruction <phase-slug>.md \
     --var phase_branch=<shared-branch-name> \
     --var commit_message="<phase commit message>"
   ```

3. **Check the result.** `forge status` plus a look at what landed on the branch (`git log`, `git diff`).
4. **Compare against the plan.** Did this phase achieve what it was meant to? Did it surface anything that changes the next phase — a missing assumption, a cleaner shape, a constraint you hadn't seen?
5. **Adjust the plan if needed**, then **write the next phase instruction file** reflecting any revisions. The next phase builds on the actual state of the branch, not on your stale mental model.
6. **Repeat** until all planned phases are done.

All phases share the same `phase_branch` value. Pick a branch name once and reuse it.

After the last phase, finalise with a single PR:

```bash
forge run open-pr \
  --var phase_branch=<shared-branch-name> \
  --var commit_message="<PR title>"
```

## Step 5 — Review the PR and merge to main

Once a PR is open (from `build`, `fix-bug`, or `open-pr`), don't stop. Run forge's PR review, which also merges on success:

```bash
forge run pr-review --pr <number>
```

Report the final outcome — PR URL, merged SHA, any follow-up items — to the user.

## When to stop and ask the user

Stop only for genuine decisions:

- A gate fails in a way that reveals the plan is wrong, not just a bug in the current phase.
- Forge surfaces a question that the plan doesn't answer (e.g. a schema choice that has lasting consequences).
- A destructive or irreversible action the plan did not authorise (force-push to main, deleting shared branches, dropping data).
- `pr-review` flags a blocking issue that requires a product or architectural call.

Do NOT stop for:
- "Phase 1 done, should I start phase 2?" — just start it.
- "The PR is open, should I review it?" — run `pr-review`.
- "Lint passed, should I continue?" — yes.

## Checking results

```bash
forge status
```

After every forge step. Read it; don't skim.

## When to use forge

**Use forge when:**
- `.forge/` exists in the project
- You're implementing a feature, fixing a bug, or refactoring
- The task involves code changes that should be tested

**Don't use forge when:**
- The task is just reading/analysing code
- You're making a one-line config change
- No `.forge/` exists and the project is too small to benefit

## Important

- Always write clear instructions before running a blueprint — re-read INSTRUCTION_GUIDE.md each time.
- Don't bypass forge gates by editing code directly when a blueprint is available.
- If a blueprint fails after retries, diagnose and fix — don't silently give up and don't hand-patch around it.
- Expect branching blueprints to include a `docs-check` step after verification gates and before PR creation.
- Report the outcome (merged SHA, PR URL, follow-ups) to the user at the end.

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
