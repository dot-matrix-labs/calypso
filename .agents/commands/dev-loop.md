# Dev Loop

Run a continuous sequential development loop until there is no remaining planned work.

Use the deterministic repo scripts under `.agents/scripts/dev-loop/` before reasoning about GitHub state.
These scripts should be treated as the source of truth for:

- whether a PR is open or merged
- whether required checks are green
- whether a linked issue checklist is complete
- which plan issue should be selected next
- whether prep is complete for branch, worktree, remote, and PR

This command is intentionally conservative:

- only one development task may be active at a time
- never fan out work in parallel
- do not stop at the first ambiguity if a low-risk next step is available
- work selection follows the Plan ordering, even when lower-priority PRs are already open

## Selection policy

Repeat this loop:

1. Read the Plan tracking issue.
2. Select the highest-priority eligible issue from the Plan.
3. If that issue already has an open PR, continue that issue.
4. If that issue does not have an open PR, prepare it deterministically and then start development.

Preferred entrypoint:

```bash
.agents/scripts/dev-loop/select-next-work.sh
```

Priority rules:

- Plan order is authoritative.
- Ignore lower-priority open PRs if the Plan now puts another issue first.
- Only one issue may be active at a time.
- The selected issue must be completed through merge before the next issue starts.

## Deterministic preparation

Before development research or implementation begins, prepare the issue with shared scripts:

- `.agents/scripts/dev-loop/ensure-issue-worktree.sh`
- `.agents/scripts/dev-loop/verify-issue-prep.sh`

Preparation must prove all of the following:

- the issue has a dedicated branch whose name matches the issue semantics
- the branch has a dedicated worktree
- the branch exists on the remote
- the branch tracks the remote
- a PR exists for the issue
- new issue branches are created from the latest `origin/main`

If preparation verification fails, fix preparation first. Do not begin implementation until prep is valid.

## How to advance the selected issue

For the selected issue or its PR:

1. Verify deterministic prep.
2. Read the linked issue and current PR state.
3. Inspect CI, mergeability, outstanding checklist items, and recent comments.
   Use:
   - `.agents/scripts/dev-loop/pr-status.sh`
   - `.agents/scripts/dev-loop/issue-status.sh`
   - `.agents/scripts/dev-loop/remote-branch-status.sh`
   - `.agents/scripts/dev-loop/merge-ready.sh`
4. Use the `develop` skill to execute the selected issue in its dedicated worktree.
5. Keep the development thread on that issue until:
   - all issue features are implemented
   - all issue checklist items are checked
   - CI is green
   - the PR is marked ready
   - the PR is merged
6. Take the smallest valid next step that moves it forward:
   - fix failing tests or CI
   - complete remaining acceptance criteria
   - update issue checklist and stage when work is complete
   - rebase or resolve conflicts if needed
   - mark the PR ready when repository gates allow it
   - merge when repository gates allow it
7. Re-check status after each increment.
8. Stay on the selected issue until it is merged or blocked by something external that cannot be resolved from repo, plan, CI, or blueprint context.

## Decision policy

When the next step is straightforward and low risk, proceed without asking clarifying questions.

If confidence is not high enough:

1. Re-situate the current issue against the Plan ordering.
2. Use the next planned work and dependencies to narrow the likely correct action.
3. If still uncertain, read the relevant parts of `calypso-blueprint/`.
4. Only ask the human if the decision is still materially ambiguous after those steps.

## Stop condition

Keep looping until all planned issues are complete.

Do not stop merely because one pass finished. Stop only when:

- there is no remaining eligible open issue in the Plan
- or progress is blocked by an external constraint that cannot be resolved from the repo, GitHub context, plan, or blueprint

## Progress rules

- Sequential execution is mandatory.
- Always prefer the smallest unblocker over speculative refactoring.
- If a selected issue has no branch/worktree/PR yet, create them deterministically before research begins.
- If a PR is ready to merge, merge it before starting the next issue.
- Development always starts from the latest `main` when a new issue branch is created.
- If a selected issue is blocked by dependencies, choose the next eligible issue from the Plan order.
- Keep issue checklists, PR bodies, and stage fields consistent with repository rules as you go.
