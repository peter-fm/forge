# Codex Brief: Forge — Development Guardrails for AI Agents

## Context

You are working on `forge` — a generalised development harness for AI coding agents. The codebase was forked from `warrant-forge` and already has a working blueprint engine (47 tests, ~2000 lines). Your job is to generalise it:

1. Add `forge init` — auto-detect project type, generate `.forge/` directory with sensible blueprints
2. Add `forge generate` — regenerate blueprints from current repo state
3. Add `forge status` — show running/completed steps
4. Update the CLI to work with `.forge/` convention (look for blueprints in `.forge/blueprints/`)
5. Add the `.forge/instructions/` workflow
6. Keep all 47 existing tests passing

Read ALL existing source files in `src/` before making changes. The engine is well-structured — don't break it.

## The `.forge/` Convention

When a user runs `forge init` in a project directory, it creates:

```
.forge/
├── config.toml              # project settings
├── blueprints/
│   ├── new-feature.toml     # implement a feature
│   ├── fix-bug.toml         # fix a bug with test verification
│   └── refactor.toml        # refactor with lint+test gates
└── instructions/            # task briefs go here (gitignored by default)
    └── .gitkeep
```

It also creates/appends to `.gitignore`:
```
.forge/instructions/*
!.forge/instructions/.gitkeep
```

## `forge init` — Project Detection

Detect project type by scanning the current directory:

| File Found | Project Type | Test Command | Lint Command | Build Command |
|---|---|---|---|---|
| `Cargo.toml` | rust | `cargo test` | `cargo clippy -- -D warnings` | `cargo build` |
| `package.json` + `bun.lock` | bun | `bun test` | `bun run lint` (if script exists) | `bun run build` (if script exists) |
| `package.json` + `node_modules` | node | `npm test` | `npm run lint` (if script exists) | `npm run build` (if script exists) |
| `pyproject.toml` or `setup.py` | python | `pytest` | `ruff check .` | (none) |
| `go.mod` | go | `go test ./...` | `go vet ./...` | `go build ./...` |
| `Makefile` only | make | `make test` | `make lint` (if target exists) | `make build` (if target exists) |

If multiple indicators exist, prefer: rust > go > python > bun > node > make.

Also detect:
- If `AGENTS.md` exists, note it in config. If it doesn't, create a minimal one mentioning `.forge/`.
- If `.github/workflows/` exists, try to extract test/lint commands from CI config.
- Read `package.json` scripts to find actual lint/test/build script names.

### Generated Blueprint: `new-feature.toml`

```toml
[blueprint]
name = "new-feature"
description = "Implement a new feature with lint and test gates"

[[step]]
type = "agentic"
name = "implement"
agent = "codex"
model = "gpt-5.4"
prompt = """Read the task instructions in .forge/instructions/. Implement the feature described there. \
Make sure to add tests for new functionality. Commit your changes."""
max_retries = 2

[[step]]
type = "deterministic"
name = "lint"
command = "{lint_command}"

[[step]]
type = "deterministic"
name = "test"
command = "{test_command}"
```

The `{lint_command}` and `{test_command}` are substituted with the detected commands during `init`.

### Generated Blueprint: `fix-bug.toml`

```toml
[blueprint]
name = "fix-bug"
description = "Fix a bug with test verification"

[[step]]
type = "agentic"
name = "fix"
agent = "codex"
model = "gpt-5.4"
prompt = """Read the task instructions in .forge/instructions/. Fix the bug described there. \
Add a regression test that would have caught this bug. Commit your changes."""
max_retries = 3

[[step]]
type = "deterministic"
name = "test"
command = "{test_command}"
```

### Generated `config.toml`

```toml
[project]
type = "rust"                    # auto-detected
name = "my-project"             # from directory name or package name

[commands]
test = "cargo test"
lint = "cargo clippy -- -D warnings"
build = "cargo build"

[agent]
default = "codex"
model = "gpt-5.4"

[instructions]
directory = "instructions"       # relative to .forge/
gitignore = true                # add instructions to .gitignore
```

## `forge run` — Updated Workflow

The existing CLI already has `run`. Update it to:

1. Look for blueprints in `.forge/blueprints/` by default (in addition to the existing `--blueprint` flag for arbitrary paths)
2. `forge run new-feature` → loads `.forge/blueprints/new-feature.toml`
3. `forge run new-feature --task "Add browser support"` → writes the task to `.forge/instructions/current.md`, then runs the blueprint
4. `forge run --blueprint /path/to/custom.toml` → existing behaviour, unchanged

Load `.forge/config.toml` if it exists to get default agent, model, and command settings. Blueprint variables `{test_command}`, `{lint_command}`, `{build_command}` are resolved from config.

## `forge generate` — Regenerate Blueprints

Re-run project detection and regenerate `.forge/blueprints/` and `.forge/config.toml`. Useful when:
- Project dependencies changed
- New test/lint scripts were added
- Switching build systems

Should warn before overwriting existing blueprints that have been manually edited (check modification time vs creation time, or add a `# forge-generated` header comment).

## `forge status`

Show the state of the current/most recent forge run:
- Which blueprint was executed
- Each step: name, status (pending/running/succeeded/failed/skipped), duration
- Current step if still running
- Final result

This can be a simple log file at `.forge/.run-status.json` updated by the engine during execution.

## CLI Updates (`src/cli.rs` + `src/main.rs`)

Current CLI structure (from `src/cli.rs`):

```
forge run --blueprint <path> [--dry-run] [--verbose]
```

New structure:

```
forge init [--type <project-type>] [--force]
forge run <blueprint-name> [--task "description"] [--dry-run] [--verbose]
forge run --blueprint <path> [--dry-run] [--verbose]
forge generate [--force]
forge status
forge list                    # list available blueprints
```

## New Files

- `src/commands/mod.rs` — command dispatch
- `src/commands/init.rs` — project detection + .forge/ generation
- `src/commands/generate.rs` — blueprint regeneration
- `src/commands/status.rs` — run status display
- `src/commands/list.rs` — list available blueprints
- `src/detect.rs` — project type detection logic

## Modified Files

- `src/cli.rs` — expanded CLI with new subcommands
- `src/main.rs` — dispatch to new commands
- `src/config.rs` — load `.forge/config.toml`, resolve variables from it
- `src/runner.rs` — write status updates during execution
- `Cargo.toml` — rename to `forge`, any new deps if needed

## Tests

Add tests for:
- `tests/detect_test.rs` — project type detection (create temp dirs with indicator files)
- `tests/init_test.rs` — `forge init` creates correct `.forge/` structure
- `tests/config_test.rs` — `.forge/config.toml` loading and variable resolution
- All 47 existing tests in `tests/engine.rs` MUST continue to pass unchanged

## Constraints

- **Don't break the engine.** The `runner.rs`, `model.rs`, `condition.rs`, `vars.rs`, `parser.rs` core should need minimal changes.
- **Backward compatible.** `forge run --blueprint /path/to.toml` must still work exactly as before.
- **Keep it simple.** `forge init` should be fast and deterministic — no network calls, no AI, just filesystem inspection.
- **Rust 2024 edition.**
- **The existing `dispatch.rs` agent support (claude-code, codex) stays unchanged.** Don't add new agent types.

## Commit Strategy

1. `feat: project detection and forge init` — detect.rs, commands/init.rs, .forge/ generation
2. `feat: .forge/config.toml loading and variable resolution`
3. `feat: updated CLI — init, run with .forge/, generate, status, list`
4. `feat: run status tracking and display`
5. `test: detection, init, config, status`
