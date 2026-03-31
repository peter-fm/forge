# forge

Development guardrails for AI coding agents.

Drop `.forge/` in any repo to define build, test, and lint gates that agents follow automatically. When an AI agent opens a project and sees `.forge/`, it knows exactly how to build, test, and verify its work — no guessing, no skipping tests, no sloppy PRs.

## Quick Start

```bash
cd my-project
forge init          # auto-detects project type, creates .forge/
forge run new-feature --task "Add dark mode support"
```

## What It Does

`forge init` inspects your project (Rust? Python? Node? Go?) and generates:

- **Blueprints** — TOML workflows that interleave deterministic gates (compile, lint, test) with agentic steps (Codex, Claude Code). The agent implements, the gates verify, retry on failure.
- **Config** — detected test/lint/build commands, default agent preferences.
- **Instructions directory** — where task briefs go. The agent reads these, not free-form prompts.

```
.forge/
├── config.toml
├── blueprints/
│   ├── new-feature.toml
│   ├── fix-bug.toml
│   └── refactor.toml
└── instructions/
```

## Why

AI coding agents are powerful but undisciplined. They'll implement a feature and skip the tests. They'll refactor code and break the build. They'll submit a PR that looks right but doesn't compile.

Forge makes the guardrails explicit. The repo declares its development workflow, and any agent that respects `.forge/` will follow it — lint after every change, test before committing, retry when gates fail.

For open source: contributors using AI agents will automatically follow your project's development standards. Fewer sloppy PRs, less maintainer burden.

## Blueprints

A blueprint is a TOML file defining a sequence of steps:

```toml
[blueprint]
name = "new-feature"
description = "Implement a feature with lint and test gates"

[[step]]
type = "agentic"
name = "implement"
agent = "codex"
model = "gpt-5.4"
prompt = "Read .forge/instructions/. Implement the feature. Add tests. Commit."
max_retries = 2

[[step]]
type = "deterministic"
name = "lint"
command = "cargo clippy -- -D warnings"

[[step]]
type = "deterministic"
name = "test"
command = "cargo test"
```

Step types:
- **deterministic** — run a shell command, pass/fail on exit code
- **agentic** — run a coding agent (Codex, Claude Code), with optional retries
- **gate** — like deterministic, but failure stops the entire workflow
- **conditional** — run only when a condition is met
- **blueprint** — invoke another blueprint (composable workflows)

## Multi-Repo / Monorepo

For projects spanning multiple repos, put `.forge/` in the parent directory:

```
warrant/
├── .forge/
│   └── blueprints/
│       └── cross-repo.toml    # knows dependency ordering
├── warrant-core/
│   └── .forge/                # single-repo blueprints
├── warrant-shell/
│   └── .forge/
└── registry/
    └── .forge/
```

## Status

Under active development. Engine is stable (47 tests). `init` and project detection coming next.
