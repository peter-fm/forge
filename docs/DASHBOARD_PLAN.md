# Forge Dashboard — Build Plan

## Overview

This document tracks the dashboard roadmap beyond the initial implementation that has already landed.

Current status:
- Phase 1 is implemented: `forge run` starts a local dashboard server, exposes `GET /`, `GET /api/state`, and `GET /events`, and keeps it alive for 60 seconds after the run finishes.
- The current HTML page is still a placeholder and the dashboard does not yet include plan approval or git diff views.
- The remaining sections below describe the intended follow-on phases.

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

### Plan Step — Default Behaviour

**Planning is on by default.** Forge automatically injects a plan step before the first agentic step in any blueprint. The user opts out with `--skip-plan`, not in.

```bash
forge run new-feature --task "Add WebSocket support"            # default: plan → approve → build
forge run new-feature --task "Add WebSocket support" --skip-plan  # skip straight to building
```

This is a runtime injection — forge inserts the plan step automatically. Blueprints stay clean and don't need a `type = "plan"` entry. The injected plan step uses the same agent and model as the first agentic step in the blueprint.

**The injected plan prompt:**
```
Explore the codebase. Understand the architecture. Then produce a numbered plan
for implementing this task: {task}

Your plan must list:
1. Every file you will create or modify
2. What changes you will make to each file and why
3. Any risks or unknowns
4. Suggested test approach

Output your plan in markdown. Do NOT make any changes — planning only.
```

**Execution flow:**
1. Agent runs in read-only exploration mode (no writes)
2. Agent's output (the plan) is captured
3. Plan renders in dashboard (or prints to terminal if `--no-dashboard`)
4. Runner blocks on `tokio::sync::oneshot` channel
5. User reviews plan — three options:
   - **Approve** — runner continues to the first real agentic step
   - **Amend** — user types feedback, plan + feedback injected into the next agentic step's prompt as context
   - **Reject** — runner aborts with message
6. If no dashboard, plan prints to terminal and runner prompts on stdin (y/n/amend)

**Why default-on:**
- Safe by default — every task gets human review before code changes
- No blueprint changes needed — works with all existing blueprints
- Simpler for users — no need to understand blueprint TOML to get planning
- `--skip-plan` is a conscious opt-out for trusted/trivial tasks

**Config override:** Can also be set in `.forge/config.toml`:
```toml
[plan]
enabled = true          # default: true
skip_blueprints = ["pr-review"]  # blueprints that never get a plan step (review is already the plan)
```

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
- `src/dashboard/mod.rs` — DashboardState, placeholder HTML, API routes, and SSE stream
- `src/runner.rs` — update shared state as root blueprint steps start and finish
- `src/cli.rs` — `--no-dashboard` and `--port` CLI flags
- `src/commands/run.rs` — dashboard launch, completion status, and shutdown wait
- `tests/dashboard_test.rs` — API coverage for the launched dashboard

Estimated: ~600 lines new code

### Phase 2 — Git diff viewer
**Scope:** Run `git diff HEAD` after each step, parse into per-file hunks, render side-by-side in dashboard
**Test:** Run a real blueprint that changes files, click through diffs in browser

Files to create/modify:
- `src/dashboard/diff.rs` — git diff parsing + HTML rendering
- `src/dashboard/mod.rs` — new routes for `/api/diff` and `/api/diff/:file`
- `src/dashboard/html.rs` — diff viewer panel in the dashboard

Estimated: ~400 lines new code

### Phase 3 — Plan step + approval flow
**Scope:** Runtime plan injection, dashboard approval UI, `oneshot` channel blocking, amendment text injection, `--skip-plan` flag, terminal fallback (stdin y/n/amend)
**Test:** Run `forge run new-feature --task "..."`, verify plan appears in dashboard, approve it, verify agent receives plan context in the implement step. Also test `--skip-plan` skips the plan step entirely.

Files to create/modify:
- `src/runner.rs` — plan step injection (before first agentic step), read-only agent execution, capture output, block for approval via oneshot channel
- `src/cli.rs` — `--skip-plan` flag
- `src/commands/run.rs` — pass skip_plan to runner, terminal fallback for plan approval when `--no-dashboard`
- `src/dashboard/mod.rs` — POST handlers for `/api/approve` and `/api/reject`
- `src/dashboard/html.rs` — plan display panel with approve/amend/reject buttons + text field
- `src/dispatch.rs` — read-only agent mode for plan exploration (no file writes)
- `src/config.rs` — `[plan]` config section: `enabled`, `skip_blueprints`

Estimated: ~500 lines new code

### Phase 4 — Polish
**Scope:** Tailscale-friendly (bind 0.0.0.0 option). Error handling, timeouts, edge cases. Config for `skip_blueprints` (e.g. pr-review never gets a plan step).

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
3. ~~Plan step opt-in~~ → **Reversed: plan is default-on, opt-out with `--skip-plan`.** Runtime injection, not a blueprint step type. Config: `[plan] enabled = true, skip_blueprints = [...]`.
4. ~~HTTP framework~~ → Axum.
5. ~~Parallel runs~~ → Auto-increment port from 8400.
