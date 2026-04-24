# Writing Forge Instruction Files

Guide for writing `.forge/instructions/<slug>.md` files — the briefs forge hands to implementing agents.

First read `.forge/blueprints/*` and decide which blueprint fits the coding session. Most branching work is `build` (single branch, single PR). Multi-phase work is `phase` + `open-pr` (shared branch across runs, one PR at the end).

## The principle

Describe the problem. Explain the larger context. State the load-bearing constraints. Leave the HOW to the implementing agent.

The instruction file is a contract on WHAT is true when the task is done, not a walkthrough of HOW to get there. If you find yourself writing function signatures, you are the one doing the design work — stop and let the agent do it.

## Shape

Eight sections, in order. Target 80–150 lines. Red flag over 300.

1. **Context** — where this work fits in the larger project. Cite specific design docs or ADRs. Name what to read; name what to ignore. Explicitly ban reading other in-flight instruction files.
2. **Problem** — what's true today. Reference actual files, actual code. End with a one-sentence gap statement.
3. **Goal** — observable outcomes, not file lists. "You can do X and Y happens." Close with: "These are outcomes, not file lists — how you lay out modules, types, functions, and tests is your call."
4. **Constraints (non-negotiable)** — load-bearing rules from the spec or from already-landed code. Only things the agent cannot decide for itself. Number them.
5. **Why [X] is the load-bearing decision** — the ONE decision per task that cannot be cheaply revised later. Walk through a concrete worked example of the failure mode. Give a self-verification property.
6. **Out of scope** — explicit "don't touch / don't stub / don't prepare for" list. End with: "If you find yourself reaching for any of the above, stop."
7. **Philosophy (non-negotiable)** — greenfield-foundation stance if applicable: no shortcuts, no fallbacks, no backward compat, delete old code wholesale. Omit this section when the code has existing users or downstream consumers; in that case, state the compatibility rules in Constraints instead.
8. **Acceptance** — self-verifiable criteria the agent can check without a human. Each item checkable with a shell command against the repo state at close. Not against some future state.

## Anti-patterns

- Inventing file paths, module layouts, function signatures, or type names.
- Inlining SQL DDL or code that already lives in a design doc. Reference the section instead.
- Step-by-step implementation sequences ("first do X, then do Y").
- Pinned literal numbers that are today-snapshots ("test count floor of 309"). Use relational acceptance ("no worse than baseline") instead.
- "Style and conventions" sections — the agent knows the language's idioms.
- Acceptance criteria that depend on a human doing something between runs.

## Verification before saving

- Line count 80–150? (Yellow at 200, red at 300.)
- Does the Goal section use outcome language, not file-list language?
- Does each constraint have a reason you can name ("spec says", "already-landed code requires it", "cross-machine contract")?
- Exactly one load-bearing section with a worked example?
- Is every acceptance criterion runnable against the repo state at close?
- No function signatures, no SQL DDL, no file paths for new modules?

## Running

Single-shot work (one branch, one PR):

```bash
forge run build \
  --instruction <slug>.md \
  --var commit_message="<commit message>"
```

Multi-phase work (shared branch across runs, one PR opened at the end):

```bash
forge run phase \
  --instruction <phase-slug>.md \
  --var phase_branch=<branch-name> \
  --var commit_message="<phase commit message>"

# After the last phase:
forge run open-pr \
  --var phase_branch=<branch-name> \
  --var commit_message="<PR title>"
```
