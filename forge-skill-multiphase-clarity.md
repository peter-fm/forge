# Forge skill: multi-phase blueprint guidance is unclear (and contains a wrong name)

## Summary

The forge skill (`~/.claude/skills/forge/SKILL.md`) does not give the agent a clear decision rule for picking between `build` (single PR) and `phase` + `open-pr` (shared branch, one PR at the end). The single mention of multi-phase work is buried in step 5 as one of several example invocations. As a result, an agent reading the skill from top to bottom will default to `build` even when planning an explicit multi-phase refactor.

It also names a blueprint that does not exist (`finalize` instead of `open-pr`), which would cause the run to fail if followed literally.

## Concrete failure I just hit

I wrote a four-phase architectural refactor plan that explicitly identified the work as phased, then ran `forge run build` for Phase 1. The build blueprint includes `push-branch` + `write-pr` + `create-pr`, so it tried to open a PR after one phase of a four-phase chain — the opposite of what the plan called for. The user had to catch this and redirect.

This was not a misreading on my part; it was the path of least resistance from the skill's structure:

- Step 3 lists `build` first in the "Common blueprints" bullets with a one-line description that matches "refactor".
- Step 5's primary example is `forge run build`. The phase-chain example follows it without a "use this instead when…" framing.
- Step 4 says to read `.forge/INSTRUCTION_GUIDE.md` "for anything architectural" — soft guidance, easy to skip when the skill itself shows a complete-looking workflow.

## Specific issues in `~/.claude/skills/forge/SKILL.md`

### 1. Wrong blueprint name (line 78)

```bash
forge run finalize \
  --var phase_branch=refactor/memory-ids \
  --var commit_message="refactor: memory subsystem"
```

`finalize` is not a blueprint. The actual file is `.forge/blueprints/open-pr.toml`. `INSTRUCTION_GUIDE.md` correctly uses `open-pr`. SKILL.md should match.

### 2. No decision rule for `build` vs `phase` (step 3 / step 5)

The "Common blueprints" list (lines 43-49) lumps `build`, `phase`, and `finalize` together with one-line descriptions. There is no "if you are doing X, choose Y" sentence. INSTRUCTION_GUIDE.md *does* have one ("Most branching work is `build`. Multi-phase work is `phase` + `open-pr`."), but the SKILL.md never quotes or paraphrases it.

### 3. INSTRUCTION_GUIDE.md is positioned as optional (line 53)

> Read `.forge/INSTRUCTION_GUIDE.md` before writing an instruction file — it defines the eight-section problem-focused shape forge agents expect. For small ad-hoc runs a short brief is fine, but for anything architectural follow the guide.

The escape hatch ("for small ad-hoc runs a short brief is fine") makes the GUIDE feel optional. In practice the GUIDE is the only place the `phase` workflow is described in any detail — making it conditional makes it easy for the agent to skip it and default to the SKILL's `build` example.

## Suggested changes

1. **Fix the name**: replace `forge run finalize` with `forge run open-pr` on line 78. Verify against the actual blueprint files (`.forge/blueprints/open-pr.toml` exists; no `finalize.toml`).

2. **Add a one-paragraph decision rule** before or inside step 3, paraphrased from INSTRUCTION_GUIDE.md:

   > Choose the blueprint by the shape of the deliverable, not the kind of work:
   > - **One commit, one PR** → `build` (features, bug fixes, single-PR refactors).
   > - **Multiple commits across multiple sessions, one PR at the end** → `phase` per session + `open-pr` once. Use this when a refactor is too large to land in one reviewable PR but you still want a single PR for the reviewer.
   > - **Bug with a regression test** → `fix-bug`.

3. **Strengthen step 4**: drop the "small ad-hoc runs a short brief is fine" escape hatch when an architectural decision (build vs phase, multiple files affected) is involved. INSTRUCTION_GUIDE.md should be required reading whenever the work spans more than a single file or commit.

4. **Reorder step 5**: show the `build` and `phase`+`open-pr` invocations as parallel options under the decision rule from change #2, not as a primary example with an aside. This makes the choice visible rather than implicit.

## Why this matters

The skill is what an agent reads when invoked via `forge` — it's the load-bearing reference. INSTRUCTION_GUIDE.md is one level down (project-local, only read if the skill points the agent at it). If the skill leads the agent to the wrong blueprint, the eight-section guide never gets a chance to correct course.
