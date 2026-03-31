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
│   └── code-review.toml     # structural review
├── instructions/            # task briefs go here (gitignored)
│   └── .gitkeep
├── archive/                 # completed instructions (gitignored)
└── runs/                    # JSONL run logs + status snapshots (gitignored)
```

Forge also adds gitignore rules for instructions, archive, and runs, and creates a starter `AGENTS.md` if one doesn't exist.

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
```

When you use `--task`, forge creates a uniquely named instruction file (e.g. `add-dark-mode.2026-03-31T1325.codex.md`) in `.forge/instructions/`, passes the path to the blueprint as `{instruction_path}`, then runs the gates. If a gate fails, the agent retries.

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
forge run <blueprint> [options]
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
  <blueprint>           Blueprint name (new-feature, fix-bug, refactor, etc.)

Options:
  --task <text>         Task description — creates an instruction file automatically
  --instruction <file>  Use an existing instruction file (in .forge/instructions/ or a path)
  --repo <name>         Target repo (for multi-repo projects)
  --issue <id>          GitHub issue number
  --round <number>      Round number (e.g. for red-team)
  --pr <number>         PR number (e.g. for merge)
  --agent <name>        Override agent (codex | claude-code)
  --model <name>        Override model
  --branch <name>       Git branch name (auto-generated if omitted)
  --var key=value       Override any variable (repeatable)
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

## Using with AI Agent Frameworks

### With Kahneman (Pi SDK)

Forge works as a tool for Kahneman's System 2. Instead of raw `codex exec` calls, System 2 runs structured workflows:

```bash
# System 2 writes the task brief
cat > .forge/instructions/current.md << 'EOF'
Add browser access to the agent via CDP.
EOF

# Then runs the blueprint
forge run new-feature
```

### With OpenClaw

Any OpenClaw agent can use forge via bash:

```bash
cd /path/to/project
forge run fix-bug --task "Fix the memory leak in the connection pool"
```

### With Claude Code / Codex Directly

Add to your project's `AGENTS.md`:

```markdown
## Development Workflow

This project uses forge for development guardrails.
Before making changes, check `.forge/` for available blueprints.
Run `forge run <blueprint> --task "your task"` instead of making changes directly.
```

Any agent that reads `AGENTS.md` will discover the workflow.

### As a Pi Skill

Forge ships with a Pi-compatible skill definition (`skills/forge.md`). Install it directly:

```bash
cp skills/forge.md ~/.pi/agent/skills/
```

Pi auto-discovers skills in `~/.pi/agent/skills/`. The skill teaches the agent to check for `.forge/`, read the config, write task instructions, and run the appropriate blueprint.

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
│   └── forge.md            # Pi/Kahneman skill definition
├── Cargo.toml
└── README.md
```

## License

MIT
