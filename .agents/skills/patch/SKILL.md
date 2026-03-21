---
name: patch
description: Extract off-topic changes from the current branch into their own issue, branch, worktree, and PR. No clarifications asked — all naming and scoping judgements are made autonomously.
user_invocable: false
---

# Patch

You are on a branch working on a feature. Some changes in the working tree (or
committed on this branch) do not belong to its theme. Extract those changes into
their own issue, branch, worktree, and PR so the current branch stays focused.

All naming and scoping decisions are made autonomously. Do NOT ask the user for
clarifications. If there is a problem, the user can close the issue and PR after
the fact.

## Inputs

The user provides: $ARGUMENTS

$ARGUMENTS may be a space-separated list of file paths to extract. If empty, the
skill identifies off-topic files autonomously (see Phase 1).

---

## Setup

```bash
REPO=$(gh repo view --json nameWithOwner -q .nameWithOwner)
CURRENT_BRANCH=$(git rev-parse --abbrev-ref HEAD)
```

---

## Phase 1: Identify the files to extract

### If $ARGUMENTS provides file paths

Use those paths exactly. Skip the analysis below.

### If $ARGUMENTS is empty

1. Read the current branch's theme from its name and recent commits:
   ```bash
   echo $CURRENT_BRANCH
   git log main..HEAD --oneline | head -10
   ```

2. List all files that differ from main (committed or not):
   ```bash
   git diff main --name-only
   git status --short
   ```

3. For each modified file, examine what changed:
   ```bash
   git diff main -- {file}
   ```

4. Identify files whose changes are unrelated to the branch's stated theme. Strong
   signals for "off-topic":
   - Path is in a completely different subsystem (e.g. `.agents/skills/` on a `feat/` branch)
   - Change is tooling, config, or documentation while branch is a product feature
   - Change is a product feature while branch is tooling or config
   - File was not mentioned in the branch's linked issue (if one exists)

5. Select those files. Do NOT ask the user to confirm.

---

## Phase 2: Save the patch

Capture the full diff of the target files versus main:

```bash
git diff main -- {files...} > /tmp/calypso-patch-extract.diff
cat /tmp/calypso-patch-extract.diff
```

Verify the patch is non-empty. If empty, stop and report — there is nothing to extract.

Also check whether any target files have committed changes on this branch:

```bash
git log main..HEAD --oneline -- {files...}
```

Record this — it affects cleanup in Phase 3.

---

## Phase 3: Remove the changes from the current branch

Restore the target files to their state on main:

```bash
git checkout main -- {files...}
```

If any of the target files had **committed** changes on this branch (detected in
Phase 2), commit a revert so the current branch history is clean:

```bash
git add {files...}
git commit -m "revert: remove off-topic changes in {files}

Extracted to their own branch by /patch."
```

If the changes were only in the working tree (not committed), no revert commit is
needed — `git checkout main -- {files}` is sufficient.

---

## Phase 4: Derive issue title, slug, and description

From the file paths and diff content, autonomously determine:

- **Title**: conventional-commit style, concise. Use `chore:`, `fix:`, `feat:`,
  `refactor:`, or `docs:` as appropriate for the nature of the change.
- **Short slug**: kebab-case, 3–5 words, suitable for a branch name.
- **Description**: 1–2 sentences describing exactly what the change does.

Do NOT ask the user. Make a judgement.

---

## Phase 5: Create the GitHub issue

```bash
gh issue create \
  --repo $REPO \
  --title "{title}" \
  --body "$(cat <<'EOF'
## Motivation

{1-2 sentences describing what the change does and why it is being isolated.
Reference the originating branch so there is an audit trail.}

## Behaviour

{What the change does — specific to the files and lines modified. Derived from the
diff. No vague language.}

## Dependencies

None.

## Scope

In scope: {list the specific files being extracted}.
Out of scope: all other changes on {current-branch}.

## Acceptance criteria

- [ ] {Primary verifiable criterion derived from the diff content}
- [ ] Changes are isolated to exactly the files listed in Scope
- [ ] {current-branch} no longer contains these changes

## Test plan

- [ ] Patch applies cleanly to main with no conflicts
- [ ] No regressions introduced on main after applying these changes

## Stage

**Current:** Specified
EOF
)"
```

Record the issue number as `{issue-number}`.

---

## Phase 6: Create branch and worktree

Derive the branch name:

```
chore/{issue-number}-{short-slug}
```

Create the branch and a linked worktree from main:

```bash
git worktree add .agents/worktrees/chore-{issue-number}-{short-slug} \
  -b chore/{issue-number}-{short-slug} \
  main
```

---

## Phase 7: Apply patch and commit

Apply the saved patch in the new worktree:

```bash
cd .agents/worktrees/chore-{issue-number}-{short-slug}
git apply /tmp/calypso-patch-extract.diff
```

If `git apply` fails:
- Try `git apply --3way /tmp/calypso-patch-extract.diff` as a fallback.
- If that also fails, stop and report the conflict to the user. Do NOT force-apply
  or create the PR with a partial patch.

Stage and commit:

```bash
git add {files...}
git commit -m "{title}

Extracted from {current-branch}.

Closes #{issue-number}"
```

---

## Phase 8: Push and create PR

```bash
git push -u origin chore/{issue-number}-{short-slug}

gh pr create \
  --repo $REPO \
  --title "{title}" \
  --body "Closes #{issue-number}"
```

---

## Phase 9: Report

Report to the user:

- Files extracted (list)
- What was done to `{current-branch}` (files restored to main state; revert commit if applicable)
- Issue URL
- PR URL

---

## Rules

- **No clarifications** — all naming and scoping decisions are autonomous
- **PR body is only `Closes #N`** — all context lives in the issue
- **Patch must apply cleanly** — stop and report if `git apply` fails; do NOT force
- **One issue per concern** — if the off-topic changes span unrelated concerns,
  create a separate issue and PR per distinct concern, each with its own worktree
- **Current branch must be clean after extraction** — `git checkout main -- {files}`
  always runs, and a revert commit is added if needed
- **Worktrees live in `.agents/worktrees/`**
- **`gh` CLI only** for all GitHub operations
- **1:1:1:1:1 invariant** — one issue, one branch, one PR, one worktree per extraction
- **Low-risk autonomy first** — choose the straightforward path without asking when the extraction boundary is obvious from the diff
