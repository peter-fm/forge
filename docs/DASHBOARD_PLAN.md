# Forge Dashboard — Build Plan

## Overview

Add a live HTML dashboard to `forge run`. Each run spins up an ephemeral HTTP server that shows progress, agent plans, step output, and git diffs. Includes a new `plan` step type where the agent explores the codebase and proposes a plan, then the user approves/amends/rejects via the dashboard before coding begins.

## Design Principle: One Dashboard, All Blueprints

The dashboard is blueprint-agnostic. It does not know what a "PR review" or "feature implementation" is — it renders steps, output, diffs, and plans from whatever the blueprint defines. No special templates, no blueprint-specific layouts. Every blueprint gets the same dashboard. The content varies because the blueprints vary, not because the dashboard changes.

## Architecture

### HTTP Server Lifecycle
- `forge run` spawns an Axum server on a background thread
- Port: start at 8400, increment if taken (parallel runs get 8401, 8402, etc.)
- Prints `Dashboard: http://localhost:8401` at launch
- Server dies when the run completes (or is killed)
- Shared state: `Arc<Mutex<DashboardState>>` between runner thread and server thread

### Dashboard State
```rust
struct DashboardState {
    blueprint_name: String,
    started_at: Instant,
    steps: Vec<StepState>,        // name, status (pending/running/passed/failed), output, duration
    current_step: Option<usize>,
    plan: Option<PlanState>,      // agent's plan text, approval status
    git_diff: Option<String>,     // current cumulative diff (refreshed after each step)
    variables: BTreeMap<String, String>,
    finished: bool,
}

struct PlanState {
    text: String,                 // the agent's plan (markdown)
    status: PlanStatus,           // pending_approval / approved / amended / rejected
    user_feedback: Option<String>,// amendment text from user
}
```

### Routes
```
GET  /              — dashboard page (single self-contained HTML with inline CSS/JS)
GET  /api/state     — JSON snapshot of DashboardState (JS polls or uses SSE)
GET  /events        — SSE stream, pushes state updates after each step transition
POST /api/approve   — approve plan (optional body: { feedback: "..." })
POST /api/reject    — reject plan, abort run
GET  /api/diff      — current git diff as unified text
GET  /api/diff/:file — diff for a single file
```

### SSE Events
```
event: step_start    data: { "step": "lint", "index": 2 }
event: step_end      data: { "step": "lint", "status": "passed", "duration_ms": 4200 }
event: plan_ready    data: { "plan": "..." }
event: plan_approved data: { "feedback": "..." }
event: run_complete  data: { "status": "success" }
```

### Plan Step Type
New step type alongside deterministic/agentic/gate/conditional/blueprint:

```toml
[[step]]
type = "plan"
name = "explore-and-plan"
agent = "claude-code"
model = "claude-sonnet-4-20250514"
prompt = """
Explore the codebase. Understand the architecture. Then produce a numbered plan
for implementing this task: {task}

Your plan must list:
1. Every file you will create or modify
2. What changes you will make to each file and why
3. Any risks or unknowns
4. Suggested test approach

Output your plan in markdown.
"""
```

**Execution flow:**
1. Agent runs in read-only exploration mode (no writes)
2. Agent's output (the plan) is captured
3. Plan renders in dashboard
4. Runner blocks on `tokio::sync::oneshot` channel
5. User reviews plan in dashboard — three options:
   - **Approve** — runner continues to next step
   - **Amend** — user types feedback, plan + feedback injected into next agentic step's prompt as context
   - **Reject** — runner aborts with message
6. If no dashboard (e.g. `--no-dashboard` flag), plan prints to terminal and runner prompts on stdin

### Git Diff Integration
- After each step completes, run `git diff HEAD` in the project root
- Parse into per-file hunks
- Dashboard shows file list with change counts (+/- lines)
- Click a file to see side-by-side diff (rendered with inline CSS, no external deps)
- Diff rendering: parse unified diff, produce two-column HTML with line numbers and colour coding

## Phases

### Phase 1 — Dashboard shell + progress tracking
**Scope:** Axum server, dashboard HTML, step progress, SSE updates, `--no-dashboard` flag
**No plan step yet, no git diffs, no approval flow**
**Test:** Run `forge run common/smoke-test` and watch steps go green in the browser

Files to create/modify:
- `Cargo.toml` — add axum, tokio, serde_json, tokio-stream dependencies
- `src/dashboard/mod.rs` — DashboardState, server setup, routes
- `src/dashboard/html.rs` — HTML template rendering (single-file, inline CSS/JS)
- `src/dashboard/sse.rs` — SSE event stream
- `src/runner.rs` — spawn server thread, update shared state after each step
- `src/main.rs` — `--no-dashboard` and `--port` CLI flags

Estimated: ~600 lines new code

### Phase 2 — Git diff viewer
**Scope:** Run `git diff HEAD` after each step, parse into per-file hunks, render side-by-side in dashboard
**Test:** Run a real blueprint that changes files, click through diffs in browser

Files to create/modify:
- `src/dashboard/diff.rs` — git diff parsing + HTML rendering
- `src/dashboard/mod.rs` — new routes for `/api/diff` and `/api/diff/:file`
- `src/dashboard/html.rs` — diff viewer panel in the dashboard

Estimated: ~400 lines new code

### Phase 3 — Plan step type + approval flow
**Scope:** New `plan` step type in the engine, dashboard approval UI, `oneshot` channel blocking, amendment text injection
**Test:** Add a plan step to a blueprint, run it, approve via dashboard, verify the coding agent receives plan + feedback

Files to create/modify:
- `src/model.rs` — add `Plan` to `StepType` enum
- `src/runner.rs` — plan step execution (run agent read-only, capture output, block for approval)
- `src/dashboard/mod.rs` — POST handlers for approve/reject
- `src/dashboard/html.rs` — plan display panel with approve/amend/reject buttons + text field
- `src/dispatch.rs` — read-only agent mode for plan exploration (no file writes)

Estimated: ~500 lines new code

### Phase 4 — Polish + blueprint updates
**Scope:** Update `forge init` generated blueprints to include plan steps. Update warrant blueprints. Tailscale-friendly (bind 0.0.0.0 option). Error handling, timeouts, edge cases.

Files to modify:
- `src/commands/init.rs` — add plan step to generated new-feature, fix-bug, refactor blueprints
- `.forge/blueprints/warrant-shell/implement-feature.toml` — add plan step
- `.forge/blueprints/warrant-shell/fix-bug.toml` — add plan step
- `.forge/blueprints/warrant-shell/red-team.toml` — add plan step (exploration before attack)

## Dogfooding Strategy

**Phase 1:** Use `forge run new-feature --task "..."` with current blueprints (no dashboard yet — terminal only)
**Phase 2:** Use Phase 1's dashboard to watch Phase 2 being built (progress tracking works, no diffs yet)
**Phase 3:** Use Phase 2's dashboard (with diffs!) to watch Phase 3 being built
**Phase 4:** Use Phase 3's dashboard (with plan approval!) to approve Phase 4's plan before coding

Each phase is a separate `forge run` invocation. Each phase tests the previous phase's output.

## Dependencies to Add

```toml
[dependencies]
axum = "0.8"
tokio = { version = "1", features = ["full"] }
tokio-stream = "0.1"
serde_json = "1"  # likely already present
```

## Open Questions (Resolved)

1. ~~User feedback on plans~~ → Accept/Amend/Reject with text field. Feedback injected into next agentic prompt.
2. ~~Diff source~~ → `git diff HEAD` after each step, plain git output.
3. ~~Plan step opt-in~~ → Explicit step type in blueprints, added to generated defaults.
4. ~~HTTP framework~~ → Axum.
5. ~~Parallel runs~~ → Auto-increment port from 8400.
