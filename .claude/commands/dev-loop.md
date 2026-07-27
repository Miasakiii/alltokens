---
description: Run one autonomous dev iteration — next P0/P1 task with de-sloppify
argument-hint: Optional task override (defaults to SHARED_TASK_NOTES.md when present)
---

# Dev Loop — Single Iteration

Run **one** focused iteration of the AllTokens dev loop. Uses the Sequential Pipeline + De-Sloppify pattern from the autonomous-loops skill. Script equivalent: `scripts/dev-pipeline.ps1`.

## Context files (read first)

1. `README.md` — always tracked: project structure, risk areas, verify commands
2. `SHARED_TASK_NOTES.md`, `STATUS.md`, `PLAN.md` — **optional** internal planning docs, not tracked in git; read each only if it exists, skip silently otherwise

## Pick the task

If `$ARGUMENTS` is non-empty, use it as the task.

Otherwise, if `SHARED_TASK_NOTES.md` exists, select the **first unchecked** item in its Progress section, preferring P0 over P1.

If neither is available, stop and ask the user for a task — do not invent one.

## Execute (in order)

### 1. Implement

- Implement the task with tests where appropriate
- Match existing Rust/React/Tauri conventions
- Do **not** create new documentation files unless required
- Do **not** commit

### 2. De-sloppify (separate pass)

Review working-tree changes. Remove:

- Tests of language/framework behavior (not business logic)
- Redundant type checks and over-defensive error handling
- Debug noise, commented-out code

Keep real business logic tests. Run tests after cleanup.

### 3. Verify

```bash
cargo test --workspace
cargo build --release -p alltokens-cli
cd frontend && npm run build
```

Fix failures only. No new features. No commit.

## Update progress notes

When the task is done and `SHARED_TASK_NOTES.md` exists:

1. Mark the completed item `[x]` in Progress
2. Add a one-line note under Notes if you learned something (blockers, paths, fixtures)
3. Update Next Steps to point at the following unchecked item

If it does not exist, summarize the outcome in your final output instead.

## Rules

- Minimal scope — smallest correct diff
- Do not commit unless the user explicitly asks
- If blocked (missing tool install, compile timeout), document in Notes and stop cleanly
