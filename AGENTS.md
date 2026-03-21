# Calypso Agent Instructions

<!-- last-edited: 2026-03-21 -->

This repository uses a Plan-driven agent workflow. Shared agent assets live under
`.agents/` and are the single source of truth for both Claude and Codex.

## Shared Assets

- `.agents/commands/` contains command entrypoints and orchestration guidance.
- `.agents/skills/` contains focused LLM workflows.
- `.agents/scripts/auto/` contains deterministic helpers for selection, git,
  issue, PR, CI, and merge state.

Vendor-specific paths may symlink to those directories. Do not create divergent
vendor-only copies of shared logic.

## Operating Model

- Work only on issues listed in the open `Plan` tracking issue.
- Execute one Plan issue at a time.
- Finish the selected issue through merge before starting the next issue.
- Treat the Plan ordering as authoritative, even if lower-priority PRs already
  exist.
- Use deterministic repo scripts before reasoning about GitHub or git state.

## Default Entry Points

- Use `calypso-auto` for continuous Plan execution.
- Use `calypso-develop` to carry one selected Plan issue from verified prep through merge.
- Use `calypso-merge` only for deterministic merge actions on the current selected PR.
- Use `calypso-feature` as the command flow for new planned work.
- Use `calypso-replan` as the command for compliance auditing plus Plan rewriting.

Skills under `.agents/skills/` are internal implementation details. Enter through
commands, not skills.

## Must Do

- Select work from the Plan and nowhere else.
- Verify branch, worktree, remote branch, and PR state before coding.
- Create new issue branches from the latest `origin/main`.
- Push regularly so CI reflects the current state of work.
- Use deterministic readiness checks before marking ready or merging.
- Keep issue checklists, PR body, and issue stage aligned with repository rules.
- Keep PR bodies to a single issue-closing reference only.
- Keep ordering metadata only in the `Plan` issue.
- Use deterministic feature scripts for new issue creation and Plan updates.

## Must Not Do

- Do not start a second issue while one selected issue is active.
- Do not begin implementation before deterministic prep passes.
- Do not leave a selected issue half-finished for a human to close out.
- Do not ask the human when the next step is obvious from the Plan, repo, CI,
  issue, or blueprint context.
- Do not treat open PRs outside the selected Plan issue as the next source of
  truth.
- Do not put phase, step, or batch metadata into issue titles or issue bodies.
- Do not plan or imply parallel execution.

## Decision Policy

Proceed without asking clarifying questions when the next step is low risk and
obvious from local context.

If confidence is not high enough:

1. Re-situate the work against the current Plan ordering and dependency state.
2. Use deterministic scripts to inspect the selected issue, branch, PR, CI, and
   merge state.
3. Search the codebase for an analogous implementation.
4. Read the relevant part of `calypso-blueprint/`.
5. Ask the human only if the decision is still materially ambiguous.

The bias is toward forward progress.

## CLI Mode

Treat work as CLI-related if any of these are true:

- the current working directory is `./cli`
- the task mentions `cli/` paths
- the task is explicitly about the Calypso CLI

For CLI work:

- use TDD
- write the smallest viable increment
- push and inspect CI before calling the work complete
- load only the relevant shared skill or command, not every skill spec

## Commit Standards

- Use conventional commits: `feat`, `fix`, `refactor`, `test`, `docs`, `chore`,
  or `security`.
- Stage files explicitly by name. Never use `git add .`.
- Never use `--no-verify`.
- Run the relevant tests before committing.

## Completion

For Plan execution, stop only when one of these is true:

- there is no remaining eligible open issue in the Plan
- progress is blocked by an external constraint that cannot be resolved from the
  repo, GitHub state, Plan, or blueprint context
