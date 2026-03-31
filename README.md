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

This creates:

```
.forge/
├── config.toml              # project settings, detected commands
├── blueprints/
│   ├── new-feature.toml     # implement a feature with gates
│   ├── fix-bug.toml         # fix a bug with test verification
│   └── refactor.toml        # refactor with lint + test gates
└── instructions/            # task briefs go here
    └── .gitkeep
```

### 2. Run a Blueprint

```bash
# Inline task
forge run new-feature --task "Add dark mode support"

# Or write detailed instructions first
cat > .forge/instructions/dark-mode.md << 'EOF'
## Dark Mode Support

Add a dark mode toggle to the settings page.
- Use CSS custom properties for theming
- Persist preference in localStorage
- Add tests for the toggle component
EOF

forge run new-feature
```

The agent reads the instructions, implements, then hits the lint gate and the test gate. If either fails, it retries (up to the configured limit).

### 3. Check Status

```bash
forge status          # current/most recent run
forge list            # available blueprints
```

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
prompt = "Read .forge/instructions/. Implement the feature. Add tests. Commit."
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
| `conditional` | Runs only when a condition expression evaluates to true. |
| `blueprint` | Invokes another blueprint. Composable workflows. |

### Agents

Built-in support for:
- **Codex** (`agent = "codex"`) — runs in tmux, polls for completion
- **Claude Code** (`agent = "claude-code"`) — runs with `--print --permission-mode bypassPermissions`

### Variables

Blueprints support variable substitution:

```toml
command = "{test_command}"     # resolved from .forge/config.toml
prompt = "Fix issue #{issue}"  # resolved from --var issue=42
```

Variables come from: `.forge/config.toml` commands, `--var` flags, step outputs, and repo paths.

## Multi-Repo Projects

For projects spanning multiple repositories, put `.forge/` in the parent directory with a dependency DAG:

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

Blueprints reference repos and their paths are injected as `FORGE_<REPO>_PATH` environment variables. Sub-blueprints can target specific repos. Build order follows your dependency chain.

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
```

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

The skill teaches System 2 to check for `.forge/` and use it when present. Install the skill:

```bash
# Copy the forge skill into Kahneman's skills directory
cp skills/forge.md ~/.kahneman/skills/
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

Forge ships with a Pi-compatible skill definition:

```bash
pi install forge    # if published to Pi skill registry
```

Or manually copy `skills/forge.md` to your agent's skills directory. The skill teaches the agent:

1. Check for `.forge/` in the current project
2. If present, read `.forge/config.toml` for project context
3. Write task instructions to `.forge/instructions/`
4. Run the appropriate blueprint
5. Report results

## Regenerating Blueprints

If your project setup changes (new test runner, different lint tool):

```bash
forge generate              # re-detect and regenerate
forge generate --force      # overwrite even if manually edited
```

## Project Structure

```
forge/
├── src/
│   ├── main.rs             # CLI entry point
│   ├── cli.rs              # clap argument parsing
│   ├── commands/            # init, run, generate, status, list
│   ├── detect.rs           # project type detection
│   ├── runner.rs           # blueprint execution engine
│   ├── model.rs            # blueprint/step data model
│   ├── config.rs           # .forge/config.toml loading
│   ├── dispatch.rs         # agent dispatch (codex, claude-code)
│   ├── condition.rs        # conditional expression evaluation
│   ├── vars.rs             # variable substitution
│   ├── parser.rs           # TOML blueprint parsing
│   ├── run_status.rs       # execution status tracking
│   ├── logger.rs           # JSONL run logging
│   └── notify.rs           # completion notifications
├── tests/                   # 56 tests
├── skills/
│   └── forge.md            # Pi/Kahneman skill definition
├── Cargo.toml
└── README.md
```

## License

MIT
