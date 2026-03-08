# Git Discipline

## Metadata Schema

Every agent commit ends with `GIT_BRAIN_METADATA` as HTML comment:

```typescript
interface CommitMetadata {
  retroactive_prompt: string;  // Instruction to reproduce this diff (NOT what you were told — what you'd tell a fresh agent now). Min 50 chars. Required.
  outcome: string;             // Observable result as test-like assertion. Required.
  context: string;             // Architectural context not visible from diff. Required.
  agent: string;               // Model ID (e.g. "claude-sonnet-4-6"). Required.
  session: string;             // Session ID for grouping commits. Required.
  hints?: string[];            // Ordered implementation notes to skip false starts.
}
```

Format:
```
feat(auth): implement jwt validation

<!--
GIT_BRAIN_METADATA:
{ "retroactive_prompt": "...", "outcome": "...", "context": "...", "agent": "...", "session": "...", "hints": [...] }
-->
```

## Hook Summary

| Stage | Check | Result |
|---|---|---|
| pre-commit | Planning docs not staged | **BLOCKS** |
| pre-commit | >10 files (excl. planning) | Warns → next-prompt.md |
| pre-commit | Lint/format | Auto-fixes; remainder → next-prompt.md |
| commit-msg | GIT_BRAIN_METADATA missing/invalid | **BLOCKS** |
| post-commit | >=10 files changed vs main | Warns → next-prompt.md |
| pre-push | >20 files vs main | **BLOCKS** |
| pre-push | Lint/format/type errors | **BLOCKS** |
| pre-push | Test failures | Allows push → next-prompt.md |

---

## `scripts/hooks/pre-commit`

```bash
#!/usr/bin/env bash
STAGED=$(git diff --cached --name-only)
PLAN_ERRORS=()

if ! echo "$STAGED" | grep -q "^docs/plans/implementation-plan\.md$"; then
  PLAN_ERRORS+=("docs/plans/implementation-plan.md")
fi

if ! echo "$STAGED" | grep -q "^docs/plans/next-prompt\.md$"; then
  PLAN_ERRORS+=("docs/plans/next-prompt.md")
fi

if [ ${#PLAN_ERRORS[@]} -gt 0 ]; then
  echo "" >&2
  echo "COMMIT BLOCKED: The following planning files were not staged:" >&2
  for f in "${PLAN_ERRORS[@]}"; do
    echo "  - $f" >&2
  done
  echo "" >&2
  echo "At every commit:" >&2
  echo "  implementation-plan.md — check off completed tasks; add or reorder discovered tasks." >&2
  echo "  next-prompt.md         — overwrite with the complete prompt for the next commit." >&2
  echo "                           A commit is the unit of work. This is how the agent" >&2
  echo "                           advances from one task to the next." >&2
  echo "" >&2
  exit 1
fi

PLANNING_DOCS="docs/plans/implementation-plan.md docs/plans/next-prompt.md"
STAGED_COUNT=$(echo "$STAGED" | grep -v "^$" | grep -vF "$PLANNING_DOCS" | wc -l | tr -d ' ')
COMMIT_FILE_LIMIT=10

if [ "$STAGED_COUNT" -gt "$COMMIT_FILE_LIMIT" ]; then
  echo "" >&2
  echo "COMMIT SIZE WARNING: This commit touches ${STAGED_COUNT} files (limit: ${COMMIT_FILE_LIMIT})." >&2
  echo "Consider splitting this work into multiple commits." >&2
  echo "" >&2

  cat >> docs/plans/next-prompt.md <<EOF

---

## Commit Size Warning

The previous commit touched ${STAGED_COUNT} files, exceeding the recommended limit of ${COMMIT_FILE_LIMIT}.
Commits should be small and focused — one logical change, committed frequently.
If the next task involves many files, split it into smaller commits before pushing.
EOF
fi

bun run eslint . --fix 2>&1 || true
bun run prettier --write . 2>&1 || true

UNFIXED_ESLINT=$(bun run eslint . --max-warnings=0 2>&1) && ESLINT_CLEAN=1 || ESLINT_CLEAN=0
UNFIXED_PRETTIER=$(bun run prettier --check . 2>&1) && PRETTIER_CLEAN=1 || PRETTIER_CLEAN=0

if [ $ESLINT_CLEAN -eq 0 ] || [ $PRETTIER_CLEAN -eq 0 ]; then
  echo "" >&2
  echo "LINT/FORMAT: auto-fix applied. The following could not be fixed automatically:" >&2
  [ $ESLINT_CLEAN -eq 0 ] && echo "$UNFIXED_ESLINT" >&2
  [ $PRETTIER_CLEAN -eq 0 ] && echo "$UNFIXED_PRETTIER" >&2
  echo "Commit is allowed. These issues WILL block your next push." >&2
  echo "" >&2

  cat >> docs/plans/next-prompt.md <<EOF

---

## Unfixed Lint/Format Issues — Must resolve before next push

$([ $ESLINT_CLEAN -eq 0 ] && echo "### ESLint\n\`\`\`\n${UNFIXED_ESLINT}\n\`\`\`")
$([ $PRETTIER_CLEAN -eq 0 ] && echo "### Prettier\n\`\`\`\n${UNFIXED_PRETTIER}\n\`\`\`")

Fix these manually, stage the changes, and include them in the next commit.
EOF
fi

exit 0
```

## `scripts/hooks/commit-msg`

```bash
#!/usr/bin/env bash
COMMIT_FILE="$1"
MSG=$(cat "$COMMIT_FILE")

if ! echo "$MSG" | grep -q "GIT_BRAIN_METADATA:"; then
  cat >&2 <<'BLOCK'

COMMIT BLOCKED: GIT_BRAIN_METADATA block is missing from the commit message.

Required format (append to end of commit message):

<!--
GIT_BRAIN_METADATA:
{
  "retroactive_prompt": "Specific, self-contained instruction to reproduce this change.",
  "outcome": "Observable, verifiable result of this commit.",
  "context": "Architectural or domain context not visible from the diff.",
  "agent": "model-name",
  "session": "session-id",
  "hints": ["ordered", "implementation", "notes"]
}
-->

BLOCK
  exit 1
fi

JSON=$(echo "$MSG" | awk '/GIT_BRAIN_METADATA:/{found=1; next} found && /-->/{exit} found{print}')

echo "$JSON" | bun run scripts/hooks/validate-commit-metadata.mjs >&2
if [ $? -ne 0 ]; then
  exit 1
fi
```

## `scripts/hooks/validate-commit-metadata.mjs`

```javascript
const REQUIRED = ["retroactive_prompt", "outcome", "context", "agent", "session"];

const chunks = [];
for await (const chunk of process.stdin) chunks.push(chunk);
const raw = chunks.join("").trim();

if (!raw) {
  process.stderr.write("GIT_BRAIN_METADATA: JSON block is empty.\n");
  process.exit(1);
}

let metadata;
try {
  metadata = JSON.parse(raw);
} catch (e) {
  process.stderr.write(`GIT_BRAIN_METADATA: JSON parse error — ${e.message}\n`);
  process.stderr.write("Ensure the block is valid JSON with no trailing commas.\n");
  process.exit(1);
}

const missing = REQUIRED.filter(f => !metadata[f] || String(metadata[f]).trim() === "");
if (missing.length > 0) {
  process.stderr.write(`GIT_BRAIN_METADATA: Missing or empty required fields: ${missing.join(", ")}\n`);
  process.stderr.write("All of the following must be present and non-empty:\n");
  REQUIRED.forEach(f => process.stderr.write(`  - ${f}\n`));
  process.exit(1);
}

const rp = metadata.retroactive_prompt.trim();
if (rp.length < 50) {
  process.stderr.write("GIT_BRAIN_METADATA: retroactive_prompt is too short (minimum 50 characters).\n");
  process.stderr.write("It must be specific enough for another agent to reproduce this change.\n");
  process.exit(1);
}
```

## `scripts/hooks/post-commit`

```bash
#!/usr/bin/env bash
PR_REMINDER_THRESHOLD=10

MERGE_BASE=$(git merge-base HEAD origin/main 2>/dev/null \
  || git merge-base HEAD main 2>/dev/null \
  || echo "")

[ -z "$MERGE_BASE" ] && exit 0

BRANCH_FILE_COUNT=$(git diff --name-only "$MERGE_BASE" HEAD | wc -l | tr -d ' ')

if [ "$BRANCH_FILE_COUNT" -ge "$PR_REMINDER_THRESHOLD" ]; then
  echo "" >&2
  echo "┌─────────────────────────────────────────────────────┐" >&2
  echo "│  PR DUE: ${BRANCH_FILE_COUNT} files changed on this branch vs. main.   │" >&2
  echo "│  Open a pull request before this branch grows further.│" >&2
  echo "└─────────────────────────────────────────────────────┘" >&2
  echo "" >&2

  cat >> docs/plans/next-prompt.md <<EOF

---

## PR Due — Open Before Continuing

This branch has changed ${BRANCH_FILE_COUNT} files since main. A pull request must be opened
imminently. Do this before starting the next feature task:

1. Ensure all tests pass and lint is clean.
2. Push the branch: \`git push\`
3. Open a PR: \`gh pr create\`
4. After merge, pull main and continue on a fresh or rebased branch.

Do not accumulate further unreviewed changes on this branch.
EOF
fi

exit 0
```

## `scripts/hooks/pre-push`

```bash
#!/usr/bin/env bash
set -euo pipefail

echo "pre-push: checking lint, format, and types..." >&2

QUALITY_FAILED=0
QUALITY_OUTPUT=""

ESLINT_OUT=$(bun run eslint . --max-warnings=0 2>&1) || { QUALITY_FAILED=1; QUALITY_OUTPUT+="$ESLINT_OUT\n"; }
PRETTIER_OUT=$(bun run prettier --check . 2>&1) || { QUALITY_FAILED=1; QUALITY_OUTPUT+="$PRETTIER_OUT\n"; }
TSC_OUT=$(bun run tsc --noEmit 2>&1) || { QUALITY_FAILED=1; QUALITY_OUTPUT+="$TSC_OUT\n"; }

if [ $QUALITY_FAILED -ne 0 ]; then
  echo "" >&2
  echo "PUSH BLOCKED: Lint, format, or type errors must be resolved before pushing." >&2
  echo -e "$QUALITY_OUTPUT" >&2
  exit 1
fi

PR_FILE_LIMIT=20
MERGE_BASE=$(git merge-base HEAD origin/main 2>/dev/null || git merge-base HEAD main 2>/dev/null || echo "")

if [ -n "$MERGE_BASE" ]; then
  PR_FILE_COUNT=$(git diff --name-only "$MERGE_BASE"...HEAD | wc -l | tr -d ' ')
  if [ "$PR_FILE_COUNT" -gt "$PR_FILE_LIMIT" ]; then
    echo "" >&2
    echo "PUSH BLOCKED: This PR changes ${PR_FILE_COUNT} files (limit: ${PR_FILE_LIMIT}). Split into smaller PRs." >&2
    echo "" >&2
    exit 1
  fi
fi

echo "pre-push: running full test suite..." >&2

TEST_OUTPUT=$(bun test 2>&1) && TEST_EXIT=0 || TEST_EXIT=$?

if [ $TEST_EXIT -ne 0 ]; then
  FAILING=$(echo "$TEST_OUTPUT" | grep -E "^\s*(FAIL|✗|×|●|not ok)" | head -30 || true)

  cat >> docs/plans/next-prompt.md <<EOF

---

## FAILING TESTS — Must be addressed before next push

\`\`\`
${FAILING}
\`\`\`

For each failure: fix the test or fix the code. Never skip/disable/ignore.

EOF

  echo "WARNING: test failures detected. Push proceeding. See next-prompt.md." >&2
fi

exit 0
```

## Hook Installation
```bash
mkdir -p .git/hooks
for hook in pre-commit commit-msg post-commit pre-push; do
  cp scripts/hooks/$hook .git/hooks/$hook
  chmod +x .git/hooks/$hook
done
```
