# Task: Workspace Management + Parallel Run Support

## Part 1: Workspace Directories

### Instruction file lifecycle

When `forge run <blueprint>` is called:

1. If `--task "description"` is provided, create an instruction file at:
   `.forge/instructions/<slug>.<timestamp>.<agent>.md`
   - slug: kebab-case from task description (first 50 chars, alphanumeric + hyphens)
   - timestamp: `YYYY-MM-DDTHHMM` (compact ISO)
   - agent: from blueprint or `--agent` flag (e.g. `codex`, `claude-code`)
   - Example: `.forge/instructions/add-websocket-support.2026-03-31T1325.codex.md`

2. The instruction file content is the task description text.

3. Blueprint prompts reference their specific instruction file:
   `Read your task instructions from .forge/instructions/<filename>`
   NOT the generic "read .forge/instructions/" — the specific file.

4. On successful completion (all gates pass):
   - Move the instruction file to `.forge/archive/<original-name>.done-<timestamp>.md`
   - Example: `.forge/archive/add-websocket-support.2026-03-31T1325.codex.done-1340.md`

5. On failure (gates fail after all retries):
   - Leave in `.forge/instructions/` for retry or human review
   - Append a `<!-- FAILED: <step-name> at <timestamp> -->` footer

### Workspace directories

These should be created by `forge init` and configurable in `config.toml`:

```toml
[workspace]
instructions = "instructions"    # active task briefs
archive = "archive"              # completed/failed work
auto_archive = true              # move to archive on success
```

`forge init` should also create a `.forge/.gitignore`:
```
instructions/*
!instructions/.gitkeep
archive/
```

### `forge clean` command

New subcommand:
- `forge clean` — move any instruction files that have a corresponding successful run in .run-status.json to archive
- `forge clean --all` — archive everything in instructions/
- `forge clean --dry-run` — show what would be archived

### Migrate existing CODEX_BRIEF files

`forge init` should detect `CODEX_BRIEF*.md`, `CODEX_INSTRUCTIONS*.md`, `RED_TEAM*.md` in the repo root and offer to move them to `.forge/archive/` (with `--force` flag to do it automatically). Print a message: "Found 3 stale instruction files in repo root. Run `forge clean --migrate` to move them to .forge/archive/."

## Part 2: Parallel Run Support

### Run IDs

Every `forge run` invocation gets a unique run ID: `<blueprint>-<short-uuid>` (e.g. `new-feature-a3f2`).

### Run-scoped state

Instead of a single `.forge/.run-status.json`, use per-run status files:
`.forge/runs/<run-id>.json`

Each contains:
```json
{
  "id": "new-feature-a3f2",
  "blueprint": "new-feature",
  "instruction_file": "add-websocket-support.2026-03-31T1325.codex.md",
  "agent": "codex",
  "started_at": "2026-03-31T13:25:00Z",
  "status": "running",
  "steps": [
    { "name": "implement", "status": "succeeded", "started_at": "...", "finished_at": "...", "attempts": 1 },
    { "name": "lint", "status": "running", "started_at": "..." }
  ],
  "finished_at": null
}
```

### `forge status` updates

- `forge status` — show ALL active runs (scan `.forge/runs/` for status=running)
- `forge status <run-id>` — show a specific run
- `forge status --all` — show all runs including completed

### Lock-free parallel execution

Multiple `forge run` processes can run simultaneously because:
- Each has its own run ID and status file
- Each has its own instruction file (unique names from timestamp + slug)
- The engine itself is single-threaded per process (steps are sequential within a run)
- No shared mutable state between runs

The only constraint: if two runs target the same repo and both modify code, they'll conflict at the git level. That's expected and correct — git handles the merge conflict, not forge.

### Blueprint prompt gets run context

The agentic step prompt should include:
```
Your run ID is {run_id}.
Your instruction file is .forge/instructions/{instruction_file}.
Read ONLY your instruction file, not other agents' instructions.
```

This prevents parallel agents from reading each other's tasks.

## Implementation Notes

### New/modified files:
- `src/workspace.rs` — instruction file creation, archiving, cleanup
- `src/run_id.rs` — run ID generation
- `src/run_status.rs` — update to use per-run files in `.forge/runs/`
- `src/commands/clean.rs` — new clean command
- `src/commands/run.rs` — create instruction file, pass to blueprint, archive on success
- `src/commands/init.rs` — create workspace directories, .gitignore
- `src/commands/status.rs` — scan .forge/runs/ for active/all runs
- `src/cli.rs` — add clean subcommand, status --all flag
- `src/runner.rs` — inject run_id and instruction_file as variables

### New variables available in blueprints:
- `{run_id}` — unique run identifier
- `{instruction_file}` — filename of the instruction file
- `{instruction_path}` — full relative path to instruction file

### Tests:
- Instruction file creation with correct naming
- Archive on success
- Failed run leaves instruction in place
- Parallel run IDs are unique
- `forge clean` moves completed instructions
- `forge status` with multiple concurrent runs
