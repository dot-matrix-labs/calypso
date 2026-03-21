# Calypso Develop

Use this command when one selected Plan issue needs to be carried all the way from
deterministic preparation through merge.

## Command role

This command owns orchestration. It does not ask the model to rediscover obvious
GitHub or git state.

### Must do

- Select only from the Plan tracking issue.
- Prepare the issue deterministically before implementation begins.
- Use the verified worktree, branch, remote branch, and PR only.
- Allow prep to create a bootstrap empty commit if that is required to open the PR.
- Invoke the internal `develop-issue` skill only after prep is valid.
- Keep the issue thread active until the PR is merged or externally blocked.

### Must not do

- Do not start from an issue outside the Plan.
- Do not start coding before branch/worktree/remote/PR verification passes.
- Do not start a second issue while one issue is active.
- Do not stop at “ready for review”.

## Deterministic flow

1. Select the issue:

```bash
.agents/scripts/auto/select-next-work.sh
```

2. Prepare and verify the issue:

```bash
.agents/scripts/auto/ensure-issue-worktree.sh {issue-number}
.agents/scripts/auto/verify-issue-prep.sh {issue-number}
```

3. Hand the verified issue to the internal `develop-issue` skill for implementation and CI recovery.
4. Use deterministic merge helpers to mark ready and merge when the gates allow it.
