
# Process — Calypso TypeScript Implementation

> This document is the Calypso TypeScript reference implementation for the [Process Blueprint](../blueprints/process-blueprint.md). The principles, threat model, and patterns in that document apply equally to other stacks. This document covers the concrete realization in the Calypso monorepo.

---

## Planning Documents

| File | Scope | Owner |
|---|---|---|
| `docs/prd.md` | What the product must do | Human (Product Owner) |
| `docs/plans/implementation-plan.md` | All tasks, ordered, with completion state | Agent, updated each commit |
| `docs/plans/next-prompt.md` | The single next action | Agent, updated each commit |

## Requirements Interview

The agent generates a structured interview using the template in the process standards. The output is written to `docs/prd.md`. See `product-owner-interview.md` in the process prompts for the interview template.

## Implementation Plan Format

```markdown
## Phase: Scaffold
- [x] Initialize git repository
- [x] Create GitHub remote
- [x] Set up CI workflows
- [ ] Stub all test suites

## Phase: Prototype
- [ ] Create landing page with mock data
- [ ] Implement basic navigation
```

Tasks are markdown checkboxes grouped by phase. Updated at every commit with both discovery (new tasks) and completion (checked boxes).

## Next Prompt Format

```markdown
## Next Action

Read `docs/plans/implementation-plan.md` and locate the first unchecked
task under "Phase: Scaffold". The previous commit completed CI workflow
setup. The next task is stubbing the test suites.

Create empty test files for: server unit, server integration, browser
unit, browser component, browser e2e. Use Vitest for unit tests and
Playwright for browser tests. Reference the testing-blueprint for test
categories and naming conventions.

After completing, update the implementation plan and write the next
prompt for the following task.
```

Written in second person. Self-contained. Includes context about what was just completed and what comes next.

## Pre-Commit Hook Enforcement

The git pre-commit hook (defined in `git-standards.md`) verifies that both `docs/plans/implementation-plan.md` and `docs/plans/next-prompt.md` are included in the commit's staged files. If either is missing, the commit is rejected.

## Scaffold Checklist (Stage 0)

1. `git init` + `gh repo create`
2. Create `.github/workflows/` with CI jobs
3. Stub all test suites (server unit, integration, browser unit, component, e2e)
4. Verify all tests run (and fail, since no implementation exists)
5. Write initial implementation plan and next-prompt

## Maturity Stages

| Stage | Key Deliverables |
|---|---|
| Scaffold | Repo, CI, test stubs, planning docs |
| Prototype | Mock data, minimal UI, basic flows, no persistence |
| Demoware | Partial integrations, realistic UI, stable demo workflows |
| Alpha | Full persistence, authentication, core business logic |
| Beta | External integrations, performance, reliability, metrics |
| V1 | Production stability, observability, backups |

## Dependency Justification

| Package | Reason | Buy or DIY |
|---|---|---|
| None required | The process blueprint introduces no runtime dependencies | N/A |
