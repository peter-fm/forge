# Forge Rebuild Plan — April 2026

> Reset to pre-April-2 state and rebuild properly with gpt-5.4.
> Commits since April 2 were built with inferior models and contain churn.
> This plan describes the intended endpoint, ordered by dependency.

## Pre-flight

- Tag old master as `pre-rebuild-april-2026` ✓
- Reset master to last clean commit (1c62723) ✓
- Working on `rebuild/april-2026` branch
- Each layer must pass `cargo test` before moving to the next

---

## Layer 1 — Foundation (no dependencies between items)

### 1.1 Repo path inference for agent dispatch
Already done in pre-existing code. ✓

### 1.2 Step logging
Already done in pre-existing code. ✓

### 1.3 Clean-tree check
Already done in pre-existing code. ✓

### 1.4 Simplify Codex dispatch — remove tmux, use subprocess
- The old run_codex() in dispatch.rs launches Codex in a tmux session and polls it
- This is unnecessary — `codex --yolo exec "prompt"` works as a plain subprocess
- Rewrite run_codex() to match run_claude(): spawn process, capture stdout/stderr, wait for exit
- Remove all tmux session management: kill_forge_sessions(), session polling loop, tmux script files
- This eliminates the entire zombie tmux session problem — no ctrlc handler needed

---

## Layer 2 — Branch naming

### 2.1 AI summarizer
- New file: src/summarize.rs
- Given a task description, produce `TaskSummary { branch_slug, commit_message }`
- Provider chain: claude -p (subscription, 30s timeout) → Codex auth (~/.codex/auth.json) → OpenAI env → Anthropic env
- Strip markdown fences from claude -p responses before JSON parsing
- Prompt asks for JSON: branch_slug (kebab-case ≤40 chars) + commit_message (conventional commit ≤72 chars)

### 2.2 Branch name as canonical identifier
- Priority chain:
  1. Explicit `--branch` flag → use as-is
  2. AI summarizer → `<prefix>/<slug>`
  3. AI fails → generic `feat/<short-hash>` / `fix/<short-hash>` based on blueprint
- **Never slugify raw task text**

### 2.3 Instruction file naming
- Priority chain for slug:
  1. Explicit `--branch` → extract part after last `/`
  2. `--instruction` file → derive from filename
  3. AI summarizer → branch_slug
  4. AI fails → generic name matching branch fallback
- **Delete `slugify_instruction` entirely**
- Delete `build_instruction_file_name` if dead code
- Format: `<slug>.<timestamp>.<agent>.md`

### 2.4 Commit message fallback
- AI fails → use branch name as commit message
- No branch either → short generic message
- **Never dump raw task text as commit message**

---

## Layer 3 — Runtime detection

### 3.1 Default branch detection
- Do not hardcode `main` or `master` anywhere
- Detect at runtime via inline shell substitution in blueprint commands
- Applies to: create-pr (--base), checkout-main, PR review checkout
- Step name `checkout-main` can stay for backwards compat

---

## Layer 4 — Default blueprints (depends on all above)

### 4.1 Blueprint generation via `forge init`
Already exists — update to use default branch detection.

### 4.2 Branching blueprint pipeline
Shared by new-feature, fix-bug, refactor:
1. clean-tree → 2. create-branch → 3. implement/fix/refactor (agentic)
4. commit-backstop → 5. lint → 6. fix-lint (agentic retry)
7. test → 8. fix-tests (agentic retry) → 9. docs-check (agentic)
10. docs-commit-backstop → 11. push-branch → 12. write-pr (agentic)
13. create-pr (using detected default branch) → 14. checkout-main (detected)

### 4.3 Code review blueprint
Takes {pr}, checks out PR branch, reviews, posts comments.

### 4.4 Refactor blueprint (multi-phase)
Plan → per-phase implement/test loops → finalize.

### 4.5 Test blueprint
Simple: run test command.

---

## Layer 5 — CLI polish

### 5.1 Real-time progress output
- Show step status as they execute
- `▶ step-name ...` → `✓ step-name (Xs)` or `✗ step-name (exit N, Xs)`

---

## Notes

- All coding must use Codex with gpt-5.4
- Each layer must be complete and tested before starting the next
- Do not interleave layers
