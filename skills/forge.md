---
command: forge
description: Use forge development guardrails when working on a project. Check for .forge/ directory, write task instructions, run blueprints with lint and test gates.
---

# Forge — Development Guardrails

When you're asked to build a feature, fix a bug, or refactor code in a project, check for a `.forge/` directory first.

## Workflow

### 1. Check for `.forge/`

```bash
ls .forge/config.toml 2>/dev/null
```

If `.forge/` exists, use it. If not, you can create it:

```bash
forge init
```

### 2. Read the project config

```bash
cat .forge/config.toml
```

This tells you: project type, test command, lint command, build command, default agent.

### 3. List available blueprints

```bash
forge list
```

Common blueprints:
- `new-feature` — implement a feature with lint + test gates
- `fix-bug` — fix a bug with regression test verification
- `refactor` — refactor with lint + test gates

### 4. Write task instructions

Write a clear brief to `.forge/instructions/current.md`:

```bash
cat > .forge/instructions/current.md << 'EOF'
## Task Title

Clear description of what needs to be done.

### Requirements
- Specific requirement 1
- Specific requirement 2

### Acceptance Criteria
- How to verify it works
EOF
```

### 5. Run the blueprint

```bash
forge run new-feature
```

Or with an inline task:

```bash
forge run fix-bug --task "Fix the null pointer in parse_config when input is empty"
```

### 6. Check results

```bash
forge status
```

## When to Use Forge

**Use forge when:**
- `.forge/` exists in the project
- You're implementing a feature, fixing a bug, or refactoring
- The task involves code changes that should be tested

**Don't use forge when:**
- The task is just reading/analysing code
- You're making a one-line config change
- No `.forge/` exists and the project is too small to benefit

## Important

- Always write clear instructions before running a blueprint
- Don't bypass forge gates by editing code directly when a blueprint is available
- If a blueprint fails after retries, report what went wrong — don't silently give up
- Check `forge status` and report the outcome to the user
