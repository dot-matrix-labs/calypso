# Process & Workflow

## Phases
0. **Scaffold** — git init, CI, TDD env, standards bootstrapped → `reference/scaffold-task.md`
1. **Collect** — PRD at `docs/prd.md`, implementation plan created
2. **Prototype** — mock data, minimal UI, no persistence
3. **Demoware** — partial integrations, realistic UI, stable demos
4. **Alpha** — full persistence, auth, core business logic
5. **Beta** — external integrations, performance, metrics
6. **V1** — production-ready, observability, backups

## Planning Documents
Updated at **every commit** (enforced by git hooks).

| File | What | Owner |
|---|---|---|
| `docs/prd.md` | Product requirements | Human |
| `docs/plans/implementation-plan.md` | Task checklist: check off done, add discovered | Agent |
| `docs/plans/next-prompt.md` | Self-contained prompt for the next action | Agent |

`next-prompt.md` creates a **self-advancing state machine**: each commit writes the prompt for the next. Human can override by editing directly.

## Workflow: Plan → Stub → TDD

### Plan (Opus/Pro-tier agent)
Create `docs/plan/<module>_plan.md`: features, technical approach, risk-prioritized task list. Review for feasibility. Edit in place.

### Stub (Opus/Pro-tier agent)
Create all file structures, signatures, `throw new Error("Not implemented")`. Stub test files with case signatures. Commit. No logic yet.

### Implement (Sonnet/Flash-tier agent)
Per function: RED (failing test) → GREEN (minimum code) → REFACTOR. Update plan after each unit. Commit with Git-Brain metadata.

## Living Plan
Update on: task complete, blocker found, new info, plan proved wrong. Mark `[x]`, `[BLOCKED:reason]`, descope, or rewrite.
