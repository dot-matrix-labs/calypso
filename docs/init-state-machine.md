# Init State Machine

The init state machine governs the `calypso init` setup workflow. It defines
a linear sequence of checkpoints that must be completed before a repository
is considered fully initialized.

## States

| Step                     | Description                                      |
|--------------------------|--------------------------------------------------|
| `prompt-directory`       | Choose or create the workspace directory         |
| `create-git-repo`        | Initialize `.git` if not present                 |
| `create-upstream`        | Create or configure the GitHub remote repository |
| `scaffold-github-actions`| Write workflow files to `.github/workflows/`     |
| `configure-local`        | Write `.calypso/` config (state, templates, hooks)|
| `verify-setup`           | Run doctor checks to validate the setup          |
| `complete`               | Terminal state — setup is finished                |

## Transition rules

Transitions are strictly linear: each step can only advance to the immediate
next step. Skipping steps is not permitted. Re-running a previously completed
step is allowed and resets progress to that point.

```
prompt-directory -> create-git-repo -> create-upstream -> scaffold-github-actions
    -> configure-local -> verify-setup -> complete
```

## Persistence

Progress is persisted to `.calypso/init-state.json` after each step
completes. If the process is interrupted, `calypso init` resumes from the
last persisted state.

The persisted record includes:
- `current_step` — the step to execute next
- `repo_path` — absolute path to the repository
- `github_org` / `github_repo` — optional upstream identifiers
- `completed_steps` — list of steps that finished successfully

## CLI commands

| Command                          | Description                              |
|----------------------------------|------------------------------------------|
| `calypso init`                   | Run or resume the full init flow         |
| `calypso init --reinit`          | Re-run init from scratch                 |
| `calypso init --status`          | Show current init state machine progress |
| `calypso init --step <step>`     | Manually trigger a specific step         |
| `calypso init --state`           | Print raw `init-state.json` contents     |

## Doctor integration

The `verify-setup` step runs `calypso doctor` checks against the repository.
Failing checks are advisory — they do not block init completion — but they
are surfaced so the operator can address them before starting feature work.
