# Task: Instruction File Lifecycle

## What to Build

Add workspace management to forge — instruction files that are created, tracked, and archived as part of the run lifecycle.

## Instruction File Creation

### Via `--task`

`forge run new-feature --task "Add WebSocket support"` should:

1. Create `.forge/instructions/<slug>.<timestamp>.<agent>.md`
   - slug: kebab-case from task text (first 50 chars, alphanumeric + hyphens only)
   - timestamp: `YYYY-MM-DDTHHMM` (compact, filesystem-safe)
   - agent: from blueprint or `--agent` flag (e.g. `codex`, `claude-code`)
   - Example: `add-websocket-support.2026-03-31T1325.codex.md`
2. Write the task text as the file content
3. Pass the **specific filename** to the blueprint as a variable `{instruction_file}`

### Via `--instruction`

`forge run fix-bug --instruction my-bug-report.md` should:

1. Verify the file exists in `.forge/instructions/` (or accept a relative/absolute path)
2. Pass it to the blueprint as `{instruction_file}`
3. Do NOT create a new file — use what the human wrote

### No scanning

Forge NEVER scans the instructions directory to pick up work. Always explicit: either `--task` or `--instruction`.

## Blueprint Prompt Variables

Add these variables available in blueprint TOML:

- `{instruction_file}` — filename only (e.g. `add-websocket-support.2026-03-31T1325.codex.md`)
- `{instruction_path}` — relative path from repo root (e.g. `.forge/instructions/add-websocket-support.2026-03-31T1325.codex.md`)

Update the generated blueprints (in commands/init.rs) to use `{instruction_path}` instead of the generic "Read .forge/instructions/" prompt:

```toml
prompt = """Read your task instructions from {instruction_path}. Implement the feature described there..."""
```

## Archiving

### On successful completion (all gates pass):

Move the instruction file to `.forge/archive/`:
- New name: `<original-stem>.done-<HHMM>.md`
- Example: `add-websocket-support.2026-03-31T1325.codex.done-1340.md`

### On failure (gates fail after all retries):

Leave the instruction file in `.forge/instructions/` unchanged. The human or agent can retry.

### `auto_archive` config

In `.forge/config.toml`:

```toml
[workspace]
auto_archive = true    # default: true
```

If `false`, instruction files stay in `instructions/` even on success.

## `forge clean` Command

New subcommand:

- `forge clean` — list instruction files in `.forge/instructions/` (excluding .gitkeep)
- `forge clean --archive` — move all instruction files to `.forge/archive/` with a `.archived-<HHMM>` suffix
- `forge clean --dry-run` — show what would be moved

## `forge init` Updates

`forge init` should now also:

1. Create `.forge/archive/` directory
2. Create `.forge/.gitignore` with:
   ```
   instructions/*
   !instructions/.gitkeep
   archive/
   runs/
   ```
3. Detect stale `CODEX_BRIEF*.md`, `CODEX_INSTRUCTIONS*.md` in repo root and print a message:
   "Found N instruction-like files in repo root. Consider moving them to .forge/instructions/ or .forge/archive/."
   (Don't move them automatically — just warn.)

## Implementation

### New files:
- `src/workspace.rs` — instruction file creation, archiving, stale file detection

### Modified files:
- `src/commands/run.rs` — accept `--task` and `--instruction`, create/resolve instruction file, archive on success
- `src/commands/init.rs` — create archive dir, .gitignore, stale file warning
- `src/commands/clean.rs` — new clean command (create this file)
- `src/cli.rs` — add `--task`, `--instruction` to run, add `clean` subcommand
- `src/vars.rs` or `src/config.rs` — inject `instruction_file` and `instruction_path` variables
- `src/commands/init.rs` — update generated blueprint prompts to use `{instruction_path}`

### Tests:
- Instruction file naming (slug generation, timestamp format, uniqueness)
- `--task` creates file with correct content
- `--instruction` resolves existing file
- Archive on successful run (mock the engine)
- Failed run leaves instruction in place
- `forge clean --archive` moves files correctly
- `forge init` creates archive dir and .gitignore
- Stale file detection in repo root

### Constraints:
- All 56 existing tests must still pass
- Don't change the engine (runner.rs) — only inject variables through the existing variable mechanism
- If neither `--task` nor `--instruction` is provided, `forge run` works exactly as before (no instruction file, backward compatible)
