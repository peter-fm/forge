# forge

Development guardrails for AI coding agents.

## The Problem

AI coding agents are powerful but undisciplined. You ask Codex to add a feature and it skips the tests. You ask Claude Code to refactor a module and it breaks the build. The PR looks plausible but doesn't compile.

This happens because agents don't know your project's development workflow. They guess — and guessing produces slop.

## The Solution

Drop `.forge/` in your repo. It declares how your project builds, tests, and lints. Any agent that sees `.forge/` follows it automatically — lint after changes, test before committing, retry when gates fail.

```bash
# One-time setup
cd my-project
forge init          # auto-detects your stack, creates .forge/

# Then, whenever an agent works on the project:
forge run new-feature --task "Add WebSocket support"
```

Forge interleaves **deterministic gates** (compile, lint, test) with **agentic steps** (Codex, Claude Code). The agent implements. The gates verify. If a gate fails, the agent retries. No slop gets through.

## Why This Matters

**For your projects:** Consistent quality regardless of which agent (or human) does the work.

**For open source:** Contributors using AI agents automatically follow your development standards. `.forge/` is like `.github/workflows/` but for the agent's *development process*, not just CI after the fact. CI tells you the PR is broken. Forge stops it being submitted broken in the first place.

**For teams:** New developers (human or AI) clone the repo, see `.forge/`, and immediately know the workflow. No tribal knowledge required.

## Install

### From source (Rust)

```bash
git clone https://github.com/peter-fm/forge
cd forge
cargo install --path .
```

### Verify

```bash
forge --help
```

## Quick Start

### 1. Initialise

```bash
cd my-project
forge init
```

Forge inspects your project and auto-detects the stack:

| Detected | Project Type | Test | Lint | Build |
|---|---|---|---|---|
| `Cargo.toml` | Rust | `cargo test` | `cargo clippy -- -D warnings` | `cargo build` |
| `package.json` + `bun.lock` | Bun | `bun test` | `bun run lint` | `bun run build` |
| `package.json` | Node | `npm test` | `npm run lint` | `npm run build` |
| `pyproject.toml` / `setup.py` | Python | `pytest` | `ruff check .` | — |
| `go.mod` | Go | `go test ./...` | `go vet ./...` | `go build ./...` |
| `Makefile` | Make | `make test` | `make lint` | `make build` |

You can override: `forge init --type rust` or edit `.forge/config.toml` after.

Detection also scans `.github/workflows/` YAML files for CI hints — if your CI already runs `cargo clippy --all-targets`, forge picks that up instead of the generic default.

This creates:

```
.forge/
├── config.toml              # project settings, detected commands
├── blueprints/
│   ├── new-feature.toml     # implement a feature with gates
│   ├── fix-bug.toml         # fix a bug with test verification
│   ├── refactor.toml        # refactor with lint + test + build gates
│   ├── test.toml            # run the detected test command directly
│   └── pr-review.toml       # review, merge, and verify an open PR
├── instructions/            # task briefs go here (gitignored)
│   └── .gitkeep
├── archive/                 # completed instructions (gitignored)
└── runs/                    # JSONL run logs + status snapshots (gitignored)
```

Forge also adds gitignore rules for instructions, archive, and runs.

### 2. Run a Blueprint

Always explicit — every run has a task:

```bash
# Inline task (forge creates the instruction file automatically)
forge run new-feature --task "Add dark mode support"

# Or write a detailed brief first, then point to it
cat > .forge/instructions/dark-mode.md << 'EOF'
## Dark Mode Support

Add a dark mode toggle to the settings page.
- Use CSS custom properties for theming
- Persist preference in localStorage
- Add tests for the toggle component
EOF

forge run new-feature --instruction dark-mode.md

# Or just run the generated test blueprint directly
forge run test
```

When you use `--task`, forge creates a uniquely named instruction file (e.g. `add-dark-mode.2026-03-31T1325.codex.md`) in `.forge/instructions/`, passes the path to the blueprint as `{instruction_path}`, runs the gates, and for branching blueprints runs a non-blocking `docs-check` step before PR creation. By default `forge run` also starts a local dashboard on port `8400` and increments to the next free port when needed; use `--no-dashboard` to disable it or `--port` to choose a starting port. If a gate fails, the agent retries.

On success, the instruction file is automatically moved to `.forge/archive/`. On failure, it stays in `instructions/` for retry.

Use `--task` or `--instruction`, not both.

### 3. Check Status and Clean Up

```bash
forge status          # current/most recent run
forge status --all    # include completed runs
forge status <run-id> # specific run
forge list            # available blueprints
forge clean           # list instruction files
forge clean --archive # move completed instructions to archive
```

## CLI Reference

```
forge init [--type <type>] [--force]
forge run [--blueprint <path>] [options] [blueprint]
forge generate [--type <type>] [--force]
forge status [run-id] [--all]
forge list
forge clean [--archive] [--dry-run]
```

### `forge init`

Auto-detect project type and create `.forge/` with config and default blueprints. If `.forge/config.toml` already exists and has been manually edited, use `--force` to overwrite.

### `forge run`

```
Arguments:
  [BLUEPRINT_NAME]      Blueprint name (new-feature, fix-bug, refactor, pr-review, etc.)

Options:
  --blueprint <path>      Run a blueprint from an explicit file path
  --task <text>         Task description — creates an instruction file automatically
  --instruction <file>  Use an existing instruction file (in .forge/instructions/ or a path)
  --repo <name>         Target repo (for multi-repo projects)
  --issue <id>          GitHub issue number
  --round <number>      Round number (e.g. for red-team)
  --pr <number>         PR number (e.g. for merge)
  --next                For `pr-review`, select the lowest-numbered open PR
  --latest              For `pr-review`, select the highest-numbered open PR
  --agent <name>        Override agent (codex | claude-code)
  --model <name>        Override model
  --branch <name>       Git branch name (auto-generated if omitted)
  --var key=value       Override any variable (repeatable)
  --no-dashboard        Disable the local run dashboard
  --port <port>         Starting port for the local dashboard (default: 8400)
  --notify <backends>   Notification backends, comma-separated (openclaw)
  --dry-run             Print steps without executing
  --verbose             Print step output as it runs
```

### `forge generate`

Re-detect project type and regenerate `.forge/` files. Same as `init` but intended for updates when your stack changes.

### `forge list`

List available blueprints from `.forge/blueprints/`.

### `forge clean`

List instruction files in `.forge/instructions/`. With `--archive`, move them to `.forge/archive/`. With `--dry-run`, show what would happen without doing it.

### Auto-Generated Branch Names

If `--branch` is not provided, the engine generates one:

| Blueprint | Branch Format |
|-----------|---------------|
| `implement-feature` | `feat/<task-slug>` |
| `fix-bug` | `fix/<issue-id>` |
| `code-review` | `refactor/code-review-<date>` |
| `red-team` | `red-team/round-<N>` |
| Other | `forge/<blueprint>-<date>` |

## Blueprints

A blueprint is a TOML file defining a sequence of steps:

```toml
[blueprint]
name = "new-feature"
description = "Implement a feature with lint and test gates"

[[step]]
type = "agentic"
name = "implement"
agent = "codex"
model = "gpt-5.4"
prompt = "Read your task instructions from {instruction_path}. Implement the feature. Add tests. Commit."
max_retries = 2

[[step]]
type = "deterministic"
name = "lint"
command = "cargo clippy -- -D warnings"

[[step]]
type = "deterministic"
name = "test"
command = "cargo test"

[[step]]
type = "agentic"
name = "docs-check"
agent = "codex"
model = "gpt-5.4"
prompt = "Review the recent code changes and update affected docs if needed."
allow_failure = true
```

### Step Types

| Type | What It Does |
|---|---|
| `deterministic` | Runs a shell command. Pass/fail on exit code. |
| `agentic` | Runs a coding agent (Codex, Claude Code). Optional retries. |
| `gate` | Like deterministic, but failure stops the entire workflow. |
| `conditional` | Runs a command, sets a variable based on exit code. |
| `blueprint` | Invokes another blueprint. Composable workflows. |

### Step Options

| Field | Types | Description |
|-------|-------|-------------|
| `command` | deterministic, conditional, gate | Shell command to run |
| `agent` | agentic | Agent to dispatch to |
| `model` | agentic | Model override |
| `prompt` | agentic | Prompt text (supports `{variables}`) |
| `blueprint` | blueprint | Sub-blueprint name |
| `params` | blueprint | Variable mapping for sub-blueprint |
| `condition` | any | Only run if condition is true (e.g. `lint.exit_code != 0`) |
| `sets` | conditional | Variable name to set based on exit code |
| `allow_failure` | any | Continue on failure instead of aborting |
| `expect_failure` | any | Invert exit code (0 = fail, non-zero = pass) |
| `max_retries` | agentic | Retry on failure up to N times |
| `env` | any | Extra environment variables for this step |

### Agents

Built-in support for:
- **Codex** (`agent = "codex"`) — runs in tmux, polls for completion
- **Claude Code** (`agent = "claude-code"`) — runs with `--print --permission-mode bypassPermissions`

### Variables

Blueprints support `{variable}` substitution:

| Source | Variables |
|--------|-----------|
| `.forge/config.toml` commands | `{test_command}`, `{lint_command}`, `{build_command}` |
| `--task` / `--instruction` | `{instruction_file}`, `{instruction_path}` |
| `--var key=value` | `{key}` |
| CLI flags | `{target_agent}`, `{target_model}`, `{branch}`, `{issue}`, etc. |
| Auto-generated | `{date}`, `{forge_path}` |
| Multi-repo config | `{<repo>_path}` for each `[repos.<name>]` entry |

## Configuration

`.forge/config.toml`:

```toml
[project]
type = "rust"
name = "my-project"

[commands]
test = "cargo test"
lint = "cargo clippy -- -D warnings"
build = "cargo build"

[agent]
default = "codex"               # default agent for agentic steps
model = "gpt-5.4"               # default model

[instructions]
directory = "instructions"       # relative to .forge/
gitignore = true                 # add instructions to .gitignore
agents_md = "AGENTS.md"         # path to AGENTS.md (if detected)

[workspace]
instructions = "instructions"    # instruction file directory
archive = "archive"              # completed instructions go here
auto_archive = true              # archive on successful run
```

## Multi-Repo Projects

For projects spanning multiple repositories, put `.forge/` in the parent directory:

```
my-platform/
├── .forge/
│   ├── config.toml           # declares repos and dependency order
│   └── blueprints/
│       └── cross-repo.toml   # builds in dependency order
├── core-lib/
│   └── .forge/               # single-repo blueprints
├── api-server/
│   └── .forge/
└── frontend/
    └── .forge/
```

Config declares the repos:

```toml
[repos.core-lib]
path = "core-lib"

[repos.api-server]
path = "api-server"

[repos.frontend]
path = "frontend"
```

Repo paths are injected as variables: `core-lib` → `{core_lib_path}`. Sub-blueprints can target specific repos. Build order follows your dependency chain.

## Using with Coding Agents

### Claude Code

```bash
# Use Claude Code as the implementing agent
forge run new-feature \
  --task "Add rate limiting to the API" \
  --agent claude-code \
  --model claude-sonnet-4-20250514

# Review and merge an open PR
forge run pr-review --pr 12 --agent claude-code

# Or auto-select from open PRs
forge run pr-review --next
forge run pr-review --latest
```

### Codex

```bash
# Use Codex as the implementing agent
forge run fix-bug \
  --task "Fix the null pointer in parse_config" \
  --agent codex \
  --model gpt-5.4

# Refactor with Codex
forge run refactor --task "Extract the auth middleware into its own module"
```

### Adding Forge to Your Repo

Add this to your project's `AGENTS.md` so any agent that reads it discovers the workflow:

```markdown
## Development Workflow

This project uses forge for development guardrails.
Run `forge list` to see available blueprints.
Run `forge run <blueprint> --task "your task"` instead of making changes directly.
```

### Parallel Agents

Multiple agents can work on different features simultaneously. Each `forge run` creates its own branch. When you're ready to merge:

```bash
# Review and merge oldest open PR first
forge run pr-review --next
```

The pr-review blueprint handles merge conflicts from earlier merges automatically.

### Agent Skill

Forge ships with a skill definition (`skills/forge.md`) compatible with agent frameworks that support skill discovery. It teaches the agent the full blueprint TOML schema so it can both use existing blueprints and author new ones.

## Notifications

`--notify openclaw` sends a system event when the run completes:

- **Success:** `✅ new-feature completed: 3/3 steps passed`
- **Failure:** `❌ new-feature failed at step 'test': 2/3 steps completed`

## Regenerating Blueprints

If your project setup changes (new test runner, different lint tool):

```bash
forge generate              # re-detect and regenerate
forge generate --force      # overwrite even if manually edited
```

## Build

```bash
cargo build
cargo test
```

## Project Structure

```
forge/
├── src/
│   ├── lib.rs               # library entry point shared by the binary and tests
│   ├── main.rs             # CLI entry point
│   ├── cli.rs              # clap argument parsing (init, run, generate, status, list, clean)
│   ├── commands/
│   │   ├── init.rs         # forge init — project detection + .forge/ scaffolding
│   │   ├── run.rs          # forge run — blueprint execution with instruction lifecycle
│   │   ├── generate.rs     # forge generate — re-detect and regenerate
│   │   ├── status.rs       # forge status — display run snapshots
│   │   ├── list.rs         # forge list — enumerate available blueprints
│   │   └── clean.rs        # forge clean — list/archive instruction files
│   ├── detect.rs           # project type detection (Rust, Go, Python, Bun, Node, Make)
│   ├── runner.rs           # blueprint execution engine (Engine<L,R,G>)
│   ├── model.rs            # Blueprint, Step, StepType, RunContext, RunSummary
│   ├── config.rs           # .forge/config.toml loading + variable resolution
│   ├── dashboard/
│   │   └── mod.rs          # local run dashboard server and status API
│   ├── workspace.rs        # instruction file creation, archival, and cleanup
│   ├── dispatch.rs         # agent dispatch (Codex via tmux, Claude Code via --print)
│   ├── condition.rs        # conditional expression evaluation (==, !=, &&, ||)
│   ├── vars.rs             # {variable} substitution
│   ├── parser.rs           # TOML blueprint parsing + validation
│   ├── run_id.rs           # unique run ID generation
│   ├── run_status.rs       # JSON run status snapshots (.forge/runs/)
│   ├── logger.rs           # JSONL run logging
│   ├── notify.rs           # completion notifications (OpenClaw backend)
│   └── error.rs            # ForgeError
├── tests/
├── skills/
│   └── forge.md            # agent skill definition
├── Cargo.toml
└── README.md
```

## License

MIT
