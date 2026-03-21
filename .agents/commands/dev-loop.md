# Dev Loop

Run a continuous sequential development loop until there is no remaining planned work.

Use the deterministic repo scripts under `.agents/scripts/dev-loop/` before reasoning about GitHub state.
These scripts should be treated as the source of truth for:

- whether a PR is open or merged
- whether required checks are green
- whether a linked issue checklist is complete
- which PR or issue should be selected next

This command is intentionally conservative:

- only one development task may be active at a time
- prefer advancing existing PRs before starting new issues
- never fan out work in parallel
- do not stop at the first ambiguity if a low-risk next step is available

## Selection policy

Repeat this loop:

1. Inspect open pull requests in the repository.
2. If any open PR exists, pick the highest-priority PR to advance.
3. Only if there are no open PRs, select the next eligible issue from the Plan tracking issue.

Preferred entrypoint:

```bash
.agents/scripts/dev-loop/select-next-work.sh
```

Priority rules:

- Open PRs always come before unopened work.
- Among open PRs, prefer the earliest item in the Plan that already has a PR.
- If multiple PRs map to the same plan batch, pick the one with the clearest unblocker:
  - failing CI
  - unchecked issue tasks
  - merge conflict or stale branch
  - incomplete implementation
- Never start a second issue while another issue still has an open PR that needs work.

## How to advance an open PR

For the selected PR:

1. Read the linked issue and current PR state.
2. Inspect CI, mergeability, outstanding checklist items, and recent comments.
   Use:
   - `.agents/scripts/dev-loop/pr-status.sh`
   - `.agents/scripts/dev-loop/issue-status.sh`
3. Take the smallest valid next step that moves it forward:
   - fix failing tests or CI
   - complete remaining acceptance criteria
   - update issue checklist and stage when work is complete
   - rebase or resolve conflicts if needed
   - merge if all gates are green and repository policy allows it
4. Re-check status.
5. Stay on that PR until it is merged or genuinely blocked by something external.

Do not abandon an open PR to start a fresh issue just because the fresh issue looks easier.

## How to start new work

If there are no open PRs:

1. Read the Plan tracking issue.
2. Select the next unblocked issue from the earliest open batch.
   Use:
   - `.agents/scripts/dev-loop/plan-next-issue.sh`
3. Use the `develop` skill to execute that issue.
4. Remain sequential. Do not invoke `develop` more than once at a time.

## Decision policy

When the next step is straightforward and low risk, proceed without asking clarifying questions.

If confidence is not high enough:

1. Re-situate the current PR or issue against the Plan ordering.
2. Use the next planned work and dependencies to narrow the likely correct action.
3. If still uncertain, read the relevant parts of `calypso-blueprint/`.
4. Only ask the human if the decision is still materially ambiguous after those steps.

## Stop condition

Keep looping until all planned issues are complete and there are no remaining open PRs.

Do not stop merely because one pass finished. Stop only when:

- there are no open PRs
- there is no remaining eligible open issue in the Plan
- or progress is blocked by an external constraint that cannot be resolved from the repo, GitHub context, plan, or blueprint

## Progress rules

- Sequential execution is mandatory.
- Always prefer the smallest unblocker over speculative refactoring.
- If a PR is ready to merge, merge it before starting the next issue.
- If a selected issue is blocked by dependencies, choose the next unblocked planned issue only when there is no open PR taking priority.
- Keep issue checklists, PR bodies, and stage fields consistent with repository rules as you go.
