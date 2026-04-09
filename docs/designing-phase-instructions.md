# Designing phase instructions

A guide to writing instruction files for forge-driven coding agents, with a focus on the problem-focused brief pattern used for architectural / foundational phases.

Audience: anyone using forge to run coding agents against phase instruction files. This document lives alongside the tool and is versioned with the blueprint code so the pattern and the mechanics stay in sync.

## What a phase instruction file is

A phase instruction file is a single Markdown document describing one bounded unit of work for a coding agent to implement. It lives in the workspace at:

```
<workspace>/.forge/instructions/<phase-slug>.md
```

Naming conventions:

- kebab-case.
- Slug matches the content of the phase, not a timestamp. `memory-phase-1-stable-ids-and-ingest.md` is good; `2026-04-09-notes.md` is not.
- No leading date prefix required. Forge does not care about ordering via filename — it only resolves by name when you pass `--instruction <name>`.
- `.md` extension.

The instruction file is the direct input to the implementing agent. There is no re-summarisation, no hand-off layer, no orchestrator translating the file into a different prompt. The agent reads the file directly and acts on it. That means the file has to stand alone and be intelligible without extra context — if it reads as "the agent will already know X", it won't, and the phase will drift.

Instruction files are typically gitignored (the default forge init sets `.forge/instructions/` as ignored) and treated as working files, not tracked artefacts. The durable record of what a phase said is the commit it produced plus the auto-archived copy in `.forge/archive/`, not the file on the working tree.

## The refactor-phase blueprint

For architectural / foundational phases, forge ships a blueprint called `refactor-phase` at `.forge/blueprints/refactor-phase.toml`. It has five steps, in order:

1. **`checkout-or-create-branch`** (deterministic)
   `git checkout {refactor_branch} 2>/dev/null || git checkout -b {refactor_branch}`
   Lands the working tree on the right branch. Idempotent — if the branch already exists, it checks it out; otherwise it creates it.

2. **`implement-phase`** (agentic)
   Dispatches the target agent against `{instruction_path}` with a prompt that says "read your task instructions from this file and implement this refactor phase without changing intended behavior outside the scoped phase." This is the step that does the work. The agent owns the solution.

3. **`commit-backstop`** (deterministic)
   `git add -A && git diff --cached --quiet || git commit -m "{commit_message}"`
   Safety net. If the agent implemented changes but forgot to commit, this backstop commits them. If the diff is empty (agent already committed), it's a no-op. You never end a phase with uncommitted work sitting in the index.

4. **`test`** (deterministic, `allow_failure = true`)
   `cargo test` (or whatever the workspace's detected test command is). Runs the tests. `allow_failure = true` means a test failure does not abort the run — it just sets a non-zero exit code that the next step keys off.

5. **`fix-tests`** (agentic, `condition = "test.exit_code != 0"`, `max_retries = 2`)
   Only runs if step 4 failed. Tells the agent "tests failed, run the test command to see the failures, fix them, don't change functionality". Retried up to twice. If it still fails, the run ends with a failure report.

The shape is: deterministic checkout, agentic implement, deterministic commit, deterministic test gate, agentic fix-test loop. The gates bracket the agentic work so drift is caught immediately.

## Variables

`refactor-phase` is parameterised by:

- `{refactor_branch}` — the branch name to check out or create. Passed via `--var refactor_branch=<name>`.
- `{target_agent}` — which agent to dispatch. Defaults from workspace `.forge/config.toml` `[agent] default`. Can be overridden via `--var target_agent=<name>` or the top-level `--agent` flag.
- `{target_model}` — which model to run. Defaults from workspace `.forge/config.toml` `[agent] model`. Can be overridden via `--var target_model=<name>` or `--model`.
- `{instruction_path}` — resolved from `--instruction <file>`. See below.
- `{commit_message}` — the backstop commit message, passed via `--var commit_message="..."`.

For deskjob, `.forge/config.toml` sets codex + gpt-5.4 as the default agent and model, so you typically only need to supply `refactor_branch`, `instruction`, and `commit_message` at the CLI.

### Instruction path resolution

`--instruction <input>` is resolved in this order (see `src/workspace.rs::resolve_instruction_file`):

1. Absolute path — used as-is.
2. `<workspace>/.forge/instructions/<input>` — relative to the conventional instructions directory.
3. `<workspace>/<input>` — relative to the workspace root.

Bare filenames work. `--instruction memory-phase-1-stable-ids-and-ingest.md` will find `.forge/instructions/memory-phase-1-stable-ids-and-ingest.md` automatically.

## Running a phase

Typical invocation:

```
forge run refactor-phase \
  --instruction memory-phase-1-stable-ids-and-ingest.md \
  --var refactor_branch=memory/phase-1-stable-ids \
  --var commit_message="feat(memory): phase 1 — stable IDs and ingest"
```

That starts the run, drives the agent against the instruction file, runs the test gate, retries on failure, and archives the instruction file on success.

## Run artefacts

Every run writes to `<workspace>/.forge/runs/run-<unix_ts>/`. Contents:

- `run.jsonl` — the structured run log. One JSONL event per step transition, covering start / stdout chunks / stderr chunks / exit.
- `step-N-<slug>.log` — one file per step, containing the raw output of that step as it ran. The slug is derived from the step's `name` field.

To tail live progress of the agentic implementation step on a running phase:

```
tail -f .forge/runs/run-<ts>/step-2-implement-phase.log
```

The step log streams in real time because forge dispatches agents via a raw PTY and a spawn_reader thread flushes chunks to the log as they arrive. You can watch the agent think as it happens.

### How dispatch actually works

`src/dispatch.rs` does not shell out to tmux, screen, or any session manager. It calls `nix::pty::openpty()` directly from Rust to allocate a master/slave pty pair, spawns `bash` with the slave fd wired up as stdin/stdout/stderr, and launches a `spawn_reader` thread that reads chunks from the master fd and writes them to the step log file as they arrive.

The practical implications:

- Live step logs — you see output the moment it happens, no buffering.
- No tmux binary required in the host environment.
- No session naming collisions between concurrent runs.
- Killing the forge process cleanly kills the child via the pty.

If you are reading source to understand the runtime model, start at `src/dispatch.rs` and look for `openpty` and `spawn_reader`. There is no tmux code to find.

## Auto-archive

If the workspace config has `[workspace] auto_archive = true` (forge init's default), then on a successful run the instruction file is moved from `.forge/instructions/<name>.md` to `.forge/archive/<name>.md`. On a failed run the file stays in place so you can edit it and re-run.

If you want to keep a completed instruction as a reference while still archiving it, copy it first or check the archive copy back in manually — but remember that instruction files are typically gitignored, so "checked in" means either tracking a specific file or committing a curated copy elsewhere in the repo.

## Writing the instruction file

There are two common shapes for phase instruction files:

- **Prescriptive** — for small TDD phases with clear test cases. List the types, functions, tests, and file paths. The agent executes. Covered by the `phased-build-with-codex` Hermes skill.
- **Problem-focused** — for architectural / foundational phases. Describe the problem, explain the larger context, state the load-bearing constraints, leave the HOW to the agent. Covered here and by the `designing-phase-instructions` Hermes skill.

The rest of this document is about the problem-focused shape, because that is the one that tends to go wrong and is the one the refactor-phase blueprint is designed for.

### The principle

Describe the problem. Explain the larger context. State the load-bearing constraints. Leave the HOW to the implementing agent.

The instruction file is a contract on WHAT is true when the phase is done, not a walkthrough of HOW to get there. If you find yourself writing function signatures, SQL DDL, module layouts, or step-by-step sequences, you are doing the design work instead of letting the agent do it. Stop and ask whether you actually have a decision to make or whether you're pre-committing to shapes you haven't thought through.

This matters specifically for forge-driven phases because the implementing agent is capable of making good design decisions when given a clear problem statement. A prescriptive brief either forces the agent to follow decisions you didn't vet, or leaves it to silently deviate, both of which waste the design value of the run.

### The recommended plan shape

Eight sections, in order. Target length 80–150 lines. Red flag over 300.

#### 1. Context

Where this phase fits in the larger spec or vision. Brief. Pulled from source-of-truth docs, not invented. Cite the spec file name and the specific sections the agent should read. Name explicitly anything to ignore — superseded instruction files, earlier-phase notes that describe the pre-refactor architecture, anything that could mislead.

Do not paraphrase the spec. Point to it. The agent will read it.

#### 2. Problem

What's true today, what's missing, what the user-visible gap is. Be concrete. Reference actual files, actual function names, actual current behaviour. End with a one-sentence statement of the specific gap this phase closes ("The user-visible gap is X. This phase closes that gap and only that gap.").

Do not describe hypothetical future problems. Describe what is wrong now.

#### 3. Goal

Observable outcomes, not file lists. Write in "you can do X and Y happens" style:

> When Phase 1 is done, running `deskjob ingest` walks `.deskjob/transcripts/`, populates the spool, and running it again reports zero new events.

Not:

> Create `crates/deskjob-spool/src/lib.rs` with a `Spool` struct that has an `ingest` method returning `Result<IngestStats>`.

Close the section with an explicit line like "these are outcomes, not file lists — how you lay out crates, modules, types, functions, and tests is your call." That gives the agent permission to design.

#### 4. Constraints (non-negotiable)

Load-bearing rules from the spec or from already-landed code. Only include things the implementing agent cannot or should not decide for itself. Number them. Each one should be defensible as "this is not yours to decide".

Typical categories of legitimate constraint:
- Identity / addressing schemes (event IDs, record keys, stable hashes).
- Schema contracts with downstream consumers.
- Cross-machine or cross-process compatibility rules.
- "This path is already shipped, do not regress it."
- "Dependency X is forbidden because it conflicts with principle Y in the spec."

If you can't name the reason a constraint is load-bearing, it's prescription pretending to be a constraint. Delete it.

#### 5. Why [the single most load-bearing constraint] is the load-bearing decision

This is the most important section in the plan. Pick the ONE decision in this phase that cannot be cheaply revised later and dedicate a section to explaining it.

Cover:

- **Why it matters** — who depends on it. Name them. Later phases, users, cross-machine sync, regression harnesses, the eval file, downstream storage.
- **What breaks if it's wrong** — concrete downstream failures. "Every stored ID becomes a lie. Summaries lose their anchors. The recall eval goes stale."
- **A worked example showing the failure mode** — real data, real bytes, two competing choices, the user-visible divergence. This is the thing that actually teaches the agent to take the decision seriously. Let this part breathe. Show the bad path and the good path side by side.
- **A self-verification property** — something the agent can test against without a human. "The same conversation canonicalised on two machines hashes identically, byte for byte."

This is the ONE section the plan invests real prose in. Everything else is tight. The worked example is the investment that pays back across every later phase that sits on top of this decision.

There is usually exactly one such decision per phase. Occasionally two. If you have three, the phase is too big — split it.

#### 6. Out of scope

Explicit "don't touch / don't stub / don't prepare for" list. Name the specific tables, files, functions, crates, CLI commands, features. Prevents scope creep without prescribing shape. Close with "if you find yourself reaching for any of the above, stop."

This section is unapologetically long. It's the one place where exhaustive enumeration is a virtue.

#### 7. Philosophy (non-negotiable)

Use this section when there are no existing users and this is foundation-laying work. Free the agent to delete old code wholesale, rename things, not hedge, not fall back, not leave shims.

Typical beats:

- No shortcuts. No quick fixes. Do the right thing, not the expedient thing.
- No fallbacks to old code. No backwards compatibility hedging. There are no users to keep happy.
- When this phase makes an existing path obsolete, replace it wholesale and delete the old path. No shims "just in case" — there is no case.
- No preserving behaviour for phantom consumers. Rename things that should be renamed.
- Foundation-quality. Everything added here will be depended on by later phases. If a choice feels like "I'll tidy this later", do the tidy version now.

Include the self-check line verbatim or nearly so:

> When you notice yourself thinking "I could just..." and the rest of the sentence is a compromise, that is the cue to do the uncompromised version instead.

If there ARE existing users or downstream consumers, do not include this section. Say so explicitly in the Constraints instead.

#### 8. Acceptance

Self-verifiable criteria the agent can check without a human in the loop. Numbered. Each item should be checkable with a shell command or a quick inspection.

Good:
- `cargo test --workspace` passes.
- `deskjob ingest` run twice in a row produces zero new events on the second run.
- The new canonical ID function has unit tests for stability, sensitivity, and the 8-character hex suffix property.
- `deskjob memory bench` output metrics are equal to or better than the pre-phase baseline captured at the start of the phase.

Bad:
- Test count is at least 309. (Snapshot of today that will go stale.)
- The Spool struct has a `new` method. (Prescribing shape.)
- Code is idiomatic. (Not self-verifiable.)
- All edge cases are handled. (Not self-verifiable.)

Use relational acceptance ("equal to or better than baseline") over absolute numbers whenever the metric could legitimately move.

## Pitfalls / anti-patterns

Do not:

- Invent file paths, module layouts, function signatures, or type names.
- Inline SQL DDL or code sketches that already live in the spec. Reference the spec section.
- Write step-by-step implementation sequences ("first do X, then do Y, then do Z").
- Include "tips" or "gotchas" about code you guessed at rather than read.
- Pin literal today-snapshots (test counts, line counts, exact metric values).
- Prescribe CLI flag surfaces beyond what the spec guarantees.
- Add a "Style and conventions" section. The agent knows Rust / TS / Python idioms.

If the plan has more than one "why this is load-bearing" section, you're probably overclaiming. One is the norm.

## Verification heuristics

Run these against the draft before saving it:

- **Line count.** 80–150 is the target. 200 is yellow. 300+ is red.
- **Grep for imperatives on shape.** `grep -iE "create (a )?file|function should|implement a (struct|function|trait|type|module) named|add a method|add a field"` should return near zero.
- **Count load-bearing explanations.** At least one "why X is the load-bearing decision" section with a worked example.
- **Scan the Goal section.** Every bullet should be an observable outcome, not a file-system artefact.
- **Scan the Constraints section.** Each constraint should be defensible as "not yours to decide because [spec / already-landed code / cross-machine / users]".

## A note on how the file is consumed

Forge does not transform the instruction file before handing it to the agent. The implement-phase step's prompt just says "read your task instructions from {instruction_path}". The agent opens the file and reads it directly. There is no re-summarisation, no hand-off, no orchestrator in the middle.

That means the file has to be self-contained. If the agent needs to know about a spec document, name it by path. If it needs to ignore older files, name them by path. If there's a subtlety that matters, put it in the file. "Context that will be provided at runtime" does not exist — the instruction file is the only context.

## Canonical example

A worked example of a problem-focused phase instruction file exists at `.forge/instructions/memory-phase-1-stable-ids-and-ingest.md` in the deskjob repo (not this repo — deskjob is a separate workspace that uses forge). 107 lines. All eight sections present. The "Why canonicalisation is the load-bearing decision" section is the pattern in its intended form.

That file is deskjob-specific in its subject matter (event IDs for a memory subsystem) but the shape and the section boundaries transfer directly to any other architectural phase in any other project. Read it before writing your first problem-focused brief.
