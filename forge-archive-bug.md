# Forge bug: `archive-instruction` step always fails when instructions are gitignored

## Summary

The `archive-instruction` step in the `build` and `fix-bug` blueprints exits 128, halting the pipeline before `push-branch` / `write-pr` / `create-pr`. This happens on every run of a project whose `.forge/.gitignore` excludes `instructions/*` — which is the default `forge init` configuration.

## Reproduction

1. `forge init` a Rust project (or any project — the layout is generic).
2. Confirm `.forge/.gitignore` contains:
   ```
   instructions/*
   !instructions/.gitkeep
   runs/
   ```
3. Write any instruction file under `.forge/instructions/<slug>.md` (it is untracked because of the gitignore).
4. `forge run build --instruction <slug>.md --var commit_message="..."`.
5. Observe: build → lint → test → docs-check all succeed; `archive-instruction` exits 128.

## Root cause

The `archive-instruction` step (`.forge/blueprints/build.toml:69-70` and `fix-bug.toml:69-70`) runs:

```bash
if [ -f .forge/instructions/{instruction_file} ]; then
  mkdir -p .forge/archive \
    && git mv .forge/instructions/{instruction_file} .forge/archive/{instruction_file} \
    && git commit -m 'chore: archive {instruction_file}'
fi
```

`git mv` requires the source path to be tracked. Because `.forge/.gitignore` excludes `instructions/*`, the source path is never tracked, so `git mv` always fails with:

```
fatal: not under version control, source=.forge/instructions/<slug>.md, destination=.forge/archive/<slug>.md
```

`set -e` semantics in the shell command mean the failure propagates as exit 128, the step fails, and the pipeline halts before `push-branch`. The whole forge run is reported as failed even though every substantive step succeeded.

## Why the workaround is fragile

Per the project's documented workaround (memory `project_forge_archive_instruction_bug.md`), the user must manually:

```bash
mv .forge/instructions/<slug>.md .forge/archive/<slug>.md
git add .forge/archive/<slug>.md
git commit -m "chore(forge): archive <slug>"
git push -u origin <branch>
gh pr create ...
```

This works but defeats the point of the blueprint — every `build` / `fix-bug` run requires the same six manual steps after forge "fails".

## Suggested fix

Replace `git mv` with plain `mv` + `git add` so the step works regardless of whether the source is tracked:

```bash
if [ -f .forge/instructions/{instruction_file} ]; then
  mkdir -p .forge/archive \
    && mv .forge/instructions/{instruction_file} .forge/archive/{instruction_file} \
    && git add .forge/archive/{instruction_file} \
    && git commit -m 'chore: archive {instruction_file}'
fi
```

This is semantically identical when the source is tracked (`git mv` is sugar for `mv` + `git rm` + `git add`), and works correctly when the source is gitignored (the `git rm` becomes a no-op because there's nothing to remove from the index).

Apply to both `.forge/blueprints/build.toml:70` and `.forge/blueprints/fix-bug.toml:70`. The `phase` and `open-pr` blueprints have no `archive-instruction` step, so they are unaffected.

## Note on `forge regen`

Per memory `project_forge_regen_drops_custom_settings.md`, manually editing the blueprint files in this project would be reverted by `forge regen`. The fix needs to land in forge's blueprint templates upstream so it is regen-stable.
