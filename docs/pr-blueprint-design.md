# Forge PR Blueprint Design — Draft for Review

## 1. Docs Check + PR Description Steps (added to all branching blueprints)

Every blueprint that creates a branch (new-feature, fix-bug, refactor) should end with these steps after tests pass:

```toml
[[step]]
type = "agentic"
name = "docs-check"
agent = "{target_agent}"
model = "{target_model}"
prompt = """
Review the changes you just made and check if the project documentation needs updating.

1. Run `git diff main...HEAD --name-only` to see what files changed.
2. Read README.md (if it exists) and check if any of these are now outdated:
   - Feature descriptions that no longer match the code
   - CLI usage examples that have changed
   - Installation instructions that need updating
   - Configuration options that were added or removed
   - Project structure sections that don't reflect new/moved files
3. Check docs/ directory (if it exists) for any files affected by your changes.
4. Check AGENTS.md (if it exists) for outdated workflow instructions.
5. If anything needs updating, make the changes and commit with message "docs: update documentation to reflect recent changes".
6. If everything is already accurate, do nothing — don't make changes for the sake of it.

Only update documentation that is genuinely affected by the code changes. Do not rewrite docs that are still correct.
"""
allow_failure = true
```

Then continue with PR creation:

```toml
[[step]]
type = "agentic"
name = "write-pr"
agent = "{target_agent}"
model = "{target_model}"
prompt = """
You have just completed work on this branch. Now write up a pull request.

1. Run `git diff main...HEAD` to see everything you changed.
2. Read the original task instructions at {instruction_path}.
3. Write a PR description covering:
   - **What problem this solves** (from the task brief)
   - **How you solved it** (architectural decisions, key changes)
   - **What changed** (files modified, new files, removed files)
   - **How to verify** (what tests cover this, how to manually check)
4. Open the PR:
   ```
   gh pr create --base main --title "<concise title>" --body "<your description>"
   ```
"""

[[step]]
type = "deterministic"
name = "verify-pr"
command = "gh pr view --json number,title,url --jq '.url'"
```

This replaces the current end-of-blueprint where the agent just commits and stops. The docs step is intentionally non-blocking so an unnecessary or failed docs review does not stop PR creation.

## 2. PR Review Blueprint (`pr-review.toml`)

A separate blueprint for reviewing and merging an open PR. Run as:

```bash
forge run pr-review --pr 42
```

Or triggered automatically after a feature blueprint completes.

```toml
[blueprint]
name = "pr-review"
description = "Senior engineer review of a PR — review implementation, merge to main, run final tests"

# Step 1: Understand what the PR is about
[[step]]
type = "agentic"
name = "review"
agent = "codex"
model = "gpt-5.4"
prompt = """
You are a senior engineer reviewing PR #{pr}.

1. Read the PR description:
   ```
   gh pr view {pr} --json title,body,additions,deletions,changedFiles
   ```

2. Check out the branch and read the full diff:
   ```
   gh pr checkout {pr}
   git diff main...HEAD
   ```

3. Review the implementation from the standpoint of the system as a whole:
   - Does the design make sense in the context of the broader codebase?
   - Are there architectural concerns, coupling issues, or missed edge cases?
   - Is the code consistent with existing patterns and conventions?
   - Are tests adequate — do they cover the new behaviour and edge cases?
   - Is there anything the implementing agent missed or got wrong?

4. If you find issues:
   - Leave review comments via `gh pr review {pr} --comment --body "..."`
   - Be specific: reference files, lines, and explain why it matters
   - Distinguish blocking issues from suggestions

5. If the code is good (or after addressing issues):
   - Approve: `gh pr review {pr} --approve --body "LGTM — <brief summary of what looks good>"`
"""
max_retries = 1

# Step 2: Merge to main (handles conflicts from parallel agents)
[[step]]
type = "agentic"
name = "merge"
agent = "codex"
model = "gpt-5.4"
prompt = """
Merge PR #{pr} to main.

1. First, try a clean merge:
   ```
   gh pr merge {pr} --squash --auto
   ```

2. If there are merge conflicts:
   - Check out the PR branch
   - Merge main into it: `git merge main`
   - Resolve conflicts carefully — understand both sides before choosing
   - Preserve the intent of both the PR and the conflicting changes
   - Commit the resolution and push
   - Then merge the PR

3. If conflicts are too complex to resolve safely, do NOT force merge.
   Instead, report what conflicts exist and stop.
"""
max_retries = 1

# Step 3: Post-merge verification
[[step]]
type = "deterministic"
name = "checkout-main"
command = "git checkout main && git pull"

[[step]]
type = "deterministic"
name = "post-merge-test"
command = "{test_command}"

[[step]]
type = "deterministic"
name = "post-merge-lint"
command = "{lint_command}"
allow_failure = true
```

## 3. Expanded Forge Skill (for agents creating custom blueprints)

The forge skill should be extended with a **Blueprint Authoring** section covering:

### TOML Schema Reference

```
[blueprint]
name = "string"           # blueprint name (matches filename without .toml)
description = "string"    # human-readable description

[[step]]
type = "deterministic|agentic|gate|conditional|blueprint"
name = "string"           # unique step name within blueprint
command = "string"         # for deterministic/conditional/gate
agent = "string"           # for agentic: "codex" or "claude-code"
model = "string"           # for agentic: model override
prompt = "string"          # for agentic: prompt text with {variables}
blueprint = "string"       # for blueprint type: sub-blueprint name
params = { key = "val" }   # for blueprint type: variable mapping
condition = "string"       # run only if condition is true
sets = "string"            # for conditional: variable name to set
allow_failure = bool       # continue on failure
expect_failure = bool      # invert exit code
max_retries = int          # for agentic: retry on failure
env = { KEY = "VAL" }      # extra env vars
```

### Available Variables

| Source | Variables |
|--------|-----------|
| Config commands | `{test_command}`, `{lint_command}`, `{build_command}` |
| CLI flags | `{instruction_path}`, `{branch}`, `{issue}`, `{pr}`, `{round}` |
| `--var` overrides | `{key}` for any `--var key=value` |
| Auto-generated | `{date}`, `{forge_path}` |
| Multi-repo | `{repo_name_path}` for each configured repo |
| Conditional steps | `{step_name.exit_code}` |
| Agent | `{target_agent}`, `{target_model}` |

### Design Principles

1. **Deterministic gates before agentic steps** — compile/lint/test should validate before the next creative step
2. **Agentic steps should be retryable** — set `max_retries` so the agent can fix its own mistakes
3. **Use sub-blueprints for reusable sequences** — lint+test is a common pattern, factor it out
4. **Include a docs-check step before PR creation** — branching workflows should review `README.md`, `docs/`, and `AGENTS.md` after gates pass, with `allow_failure = true`
5. **End branching workflows with PR creation** — don't leave orphan branches
6. **Keep prompts specific** — tell the agent exactly what commands to run, don't just say "fix it"

### Example: Custom Security Audit Blueprint

```toml
[blueprint]
name = "security-audit"
description = "Run security analysis on recent changes"

[[step]]
type = "deterministic"
name = "build"
command = "{build_command}"

[[step]]
type = "agentic"
name = "audit"
agent = "claude-code"
model = "claude-sonnet-4-20250514"
prompt = """
Review the codebase for security issues:
1. Check for hardcoded secrets, credentials, API keys
2. Review input validation and sanitisation
3. Check for injection vulnerabilities (SQL, command, path traversal)
4. Review authentication and authorisation logic
5. Check dependency versions for known CVEs

Write findings to .forge/instructions/audit-results.md with severity ratings.
"""

[[step]]
type = "deterministic"
name = "test"
command = "{test_command}"
```

## Questions for Peter

1. Should PR review be auto-triggered after new-feature/fix-bug/refactor complete? Or always manually run?
2. Should the review agent be a different model/agent than the implementing one? (Different perspective = better review)
3. For parallel agent PRs, should there be a queue/ordering mechanism, or just "merge and resolve conflicts as you go"?
4. Should `forge init` generate `pr-review.toml` as a default blueprint alongside the others?
