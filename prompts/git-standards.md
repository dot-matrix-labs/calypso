# Commit Messages

In traditional software development, the "why" behind a code change is often lost. Git commit messages provide a summary of _what_ changed, but rarely capture the full reasoning, the alternative approaches considered, or the specific prompt that led to the solution.

For autonomous agents, this context loss is critical. An agent entering an existing project needs to understand not just the current state of the code, but the _trajectory_ of decisions that led there.

**Git-Brain** transforms the version control system from a simple state tracker into a **Reasoning Ledger**. By embedding structured metadata into every commit, we create a searchable, replayable history of the agent's thought process.

### Goals

- **Context Preservation**: Enable agents to "remember" why a decision was made months ago.
- **Replayability**: Allow an agent to reconstruct a coding session by re-executing the stored prompts.
- **Auditability**: Provide a transparent log of autonomous decision-making.
- **Knowledge Transfer**: specific "Diff Reconstruction Hints" help new agents understand the architectural constraints without reading every line of code.

## 2. Specification

### 2.1 The Metadata Schema

Every agent commit must include a structured metadata block embedded as an HTML comment at the end of the commit message. The block is invisible to casual human inspection but machine-readable by agents inspecting the log.

```typescript
interface CommitMetadata {
  /**
   * The retroactive prompt.
   *
   * This is NOT the instruction you were originally given. It is the instruction
   * you would write NOW — after completing the task — if you had to send a fresh
   * agent to reproduce this exact change from scratch, with full knowledge of what
   * it actually required. It must be specific enough that another agent could
   * follow it without asking clarifying questions.
   *
   * This field is the causal link in the reasoning ledger. Reading the retroactive
   * prompts of recent commits tells an incoming agent not just what changed, but
   * the sequence of informed decisions that produced the current state.
   *
   * Required. Must be non-empty.
   */
  retroactive_prompt: string;

  /**
   * The verifiable outcome.
   *
   * What this commit actually achieves, stated as observable facts. Prefer
   * test-like assertions: "POST /auth/token returns 401 when JWT is expired."
   * Not "added middleware" — that describes the diff, not the outcome.
   *
   * Required. Must be non-empty.
   */
  outcome: string;

  /**
   * Architectural and domain context.
   *
   * What another agent needs to know about the codebase, constraints, or
   * decisions that shaped this change — context that is not visible from the
   * diff alone. Reference specific files, interfaces, or invariants where
   * relevant.
   *
   * Required. Must be non-empty.
   */
  context: string;

  /**
   * The agent identity — model or tool that produced this commit.
   * e.g. "claude-sonnet-4-6", "gemini-2.5-pro", "codex-cli"
   *
   * Required. Must be non-empty.
   */
  agent: string;

  /**
   * Session identifier. Groups commits that belong to the same working session.
   * Use any stable string: timestamp, tmux session name, task ID, etc.
   *
   * Required. Must be non-empty.
   */
  session: string;

  /**
   * Ordered implementation hints.
   *
   * Specific steps, gotchas, ordering constraints, or non-obvious decisions
   * discovered while doing the work. Written so that a future agent can skip
   * the false starts and go straight to the correct approach.
   *
   * Optional but strongly recommended for non-trivial changes.
   */
  hints?: string[];
}
```

### 2.2 Storage Format

The metadata is serialized as JSON inside an HTML comment block, placed at the very end of the commit message after a blank line.

**Example Commit Message:**

```text
feat(auth): implement jwt validation middleware

Adds middleware to verify JWT tokens on all protected routes. Requests
without a valid token receive a 401 with a structured error body.

<!--
GIT_BRAIN_METADATA:
{
  "retroactive_prompt": "Add a JWT validation middleware in apps/server/src/middleware/auth.ts. It must read the token from the Authorization: Bearer header, verify it using the HS256 secret in process.env.JWT_SECRET, attach the decoded payload to ctx.state.user, and return a structured 401 JSON error for missing or expired tokens. Wire it into the router in apps/server/src/router.ts before all /api routes.",
  "outcome": "Protected routes return 401 with {error: 'unauthorized'} when the JWT is missing or expired. Valid tokens set ctx.state.user and allow the request through.",
  "context": "The server uses Bun's native HTTP with a thin router in apps/server/src/router.ts. Auth state flows via ctx.state, not req.locals. JWT_SECRET is already defined in .env.",
  "agent": "claude-sonnet-4-6",
  "session": "sess_20260307_auth",
  "hints": [
    "Read from ctx.request.headers.get('authorization'), not req.headers",
    "Use Bun's built-in crypto.subtle for HS256 — do not add a jwt library",
    "Handle TokenExpiredError and JsonWebTokenError as distinct 401 cases",
    "Wire middleware before the route definitions, not inside them"
  ]
}
-->
```

# Pre-flight

## What blocks vs. what warns

| Stage | Check | Behaviour |
|---|---|---|
| prepare-commit-msg | — | Injects conformance checklist into every new commit message |
| pre-commit | Planning docs not staged | **BLOCKS** |
| pre-commit | Commit touches > 10 files (excl. planning docs) | **Warns** — appended to next-prompt.md |
| pre-commit | Lint / format | Auto-fixes applied; unfixable remainder **appended to next-prompt.md** |
| commit-msg | Conformance checklist has unchecked boxes | **BLOCKS** |
| commit-msg | GIT_BRAIN_METADATA missing or invalid | **BLOCKS** |
| post-commit | Branch has ≥ 10 files changed vs. main | **Warns** — PR due; appended to next-prompt.md |
| post-checkout | Branch switch | Refreshes `docs/standards/` via bootstrap script |
| pre-push | Blueprint violations (.js files, forbidden packages) | **BLOCKS** |
| pre-push | PR changes > 20 files vs. main | **BLOCKS** — split the PR |
| pre-push | Lint / format / type failures | **BLOCKS** |
| pre-push | Test suite failures | **Allows push** — appends failing tests to next-prompt.md |

---

### Conformance Checklist Injection (`prepare-commit-msg`)

The `prepare-commit-msg` hook fires before the agent writes the commit message. It appends a conformance checklist as an HTML comment — invisible in `git log` but present in the message buffer the agent edits. The agent must check every applicable box. The `commit-msg` hook then validates that no box is left unchecked, blocking the commit if any remain.

This is a deterministic self-audit: the agent cannot commit without explicitly confirming alignment with Calypso's core rules on every single commit.

**`scripts/hooks/prepare-commit-msg`:**

```bash
#!/usr/bin/env bash
# prepare-commit-msg: Injects Calypso conformance checklist into every new commit message.

COMMIT_FILE="$1"
COMMIT_SOURCE="$2"

# Only inject on fresh commits — skip merges, squashes, amends, fixups
case "$COMMIT_SOURCE" in
  merge|squash|commit|fixup) exit 0 ;;
esac

grep -q "CALYPSO_CHECKLIST" "$COMMIT_FILE" && exit 0

cat >> "$COMMIT_FILE" <<'EOF'

<!--
CALYPSO_CHECKLIST:
Check every applicable box. The commit-msg hook blocks if any box remains unchecked.

- [ ] TypeScript only — no .js or .mjs logic files added or modified
- [ ] Bun only — no npm, npx, or yarn calls introduced anywhere
- [ ] No mocks — no jest.mock, sinon, vi.mock, or msw on external dependencies
- [ ] No forbidden libs — no ORM, Docker config, external auth provider, Redux/Zustand added
- [ ] No skipped tests — no .skip(), .todo(), xit(), or commented-out test cases
- [ ] implementation-plan.md — completed tasks checked off; new discoveries added
- [ ] next-prompt.md — overwritten with the complete, self-contained prompt for the next commit
- [ ] retroactive_prompt — written as a specific reproduction instruction, not a diff summary
-->
EOF
```

**Hard vs. soft split:** The `pre-commit` hook scans the staged diff deterministically for all automatable violations (JS files, npm/npx usage, mocks, skipped tests, forbidden packages). These fire before the message is written and require no agent input. The checklist injected by `prepare-commit-msg` covers only the three things the agent must self-verify: planning doc quality and metadata intent. This keeps the checklist minimal and meaningful — every item on it genuinely requires human or agent judgement.

The checklist lives inside an HTML comment so it does not appear in `git log --oneline` or PR diffs, but is fully visible to the agent writing the message.

### Pre-commit Stage

The pre-commit hook has one hard block and one advisory check.

**BLOCKS — Planning documents not staged:**

Every commit must stage both planning files. This is the only reason a commit is rejected.

| File | What to update |
|---|---|
| `docs/plans/implementation-plan.md` | Check off completed tasks; add or reorder discovered tasks |
| `docs/plans/next-prompt.md` | Overwrite with the self-contained prompt for the next commit |

**WARNS — Lint and format (auto-fix first, then flag remainder):**

After the planning gate passes, the hook runs auto-fixers (`eslint --fix`, `prettier --write`). Most issues are corrected silently. Anything the auto-fixers could not resolve is captured and explicitly appended to `next-prompt.md` so the agent addresses it in the next commit. The commit is **not blocked**.

**`scripts/hooks/pre-commit`:**

```bash
#!/usr/bin/env bash
# pre-commit: Planning documents gate (blocking) + lint/format advisory (non-blocking).

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

# Commit size advisory — warn if too many files changed in one commit
PLANNING_DOCS="docs/plans/implementation-plan.md docs/plans/next-prompt.md"
STAGED_COUNT=$(echo "$STAGED" | grep -v "^$" | grep -vF "$PLANNING_DOCS" | wc -l | tr -d ' ')
COMMIT_FILE_LIMIT=10

if [ "$STAGED_COUNT" -gt "$COMMIT_FILE_LIMIT" ]; then
  echo "" >&2
  echo "COMMIT SIZE WARNING: This commit touches ${STAGED_COUNT} files (limit: ${COMMIT_FILE_LIMIT})." >&2
  echo "Calypso commits should be small and focused — one logical change per commit." >&2
  echo "Consider splitting this work into multiple commits." >&2
  echo "This warning has been appended to next-prompt.md." >&2
  echo "" >&2

  cat >> docs/plans/next-prompt.md <<EOF

---

## Commit Size Warning

The previous commit touched ${STAGED_COUNT} files, exceeding the recommended limit of ${COMMIT_FILE_LIMIT}.
Commits should be small and focused — one logical change, committed frequently.
If the next task involves many files, split it into smaller commits before pushing.
EOF
fi

# Lint and format: auto-fix first, then capture what could not be fixed
bun run eslint . --fix 2>&1 || true
bun run prettier --write . 2>&1 || true

# Check for issues that survived auto-fixing
UNFIXED_ESLINT=$(bun run eslint . --max-warnings=0 2>&1) && ESLINT_CLEAN=1 || ESLINT_CLEAN=0
UNFIXED_PRETTIER=$(bun run prettier --check . 2>&1) && PRETTIER_CLEAN=1 || PRETTIER_CLEAN=0

if [ $ESLINT_CLEAN -eq 0 ] || [ $PRETTIER_CLEAN -eq 0 ]; then
  echo "" >&2
  echo "LINT/FORMAT: auto-fix applied. The following could not be fixed automatically:" >&2
  [ $ESLINT_CLEAN -eq 0 ] && echo "$UNFIXED_ESLINT" >&2
  [ $PRETTIER_CLEAN -eq 0 ] && echo "$UNFIXED_PRETTIER" >&2
  echo "" >&2
  echo "Commit is allowed. These issues WILL block your next push." >&2
  echo "They have been appended to next-prompt.md." >&2
  echo "" >&2

  cat >> docs/plans/next-prompt.md <<EOF

---

## Unfixed Lint/Format Issues — Must resolve before next push

The following issues were not auto-fixable at the last commit.
They will block the next push if not resolved.

$([ $ESLINT_CLEAN -eq 0 ] && echo "### ESLint\n\`\`\`\n${UNFIXED_ESLINT}\n\`\`\`")
$([ $PRETTIER_CLEAN -eq 0 ] && echo "### Prettier\n\`\`\`\n${UNFIXED_PRETTIER}\n\`\`\`")

Fix these manually, stage the changes, and include them in the next commit.
EOF
fi

exit 0
```

### Metadata and Checklist Enforcement Hook (blocking, `commit-msg`)

The `commit-msg` hook fires after the commit message is written and runs two validations in sequence: first the conformance checklist (any unchecked box blocks), then the `GIT_BRAIN_METADATA` schema (missing block, malformed JSON, or empty required fields block).

This hook runs at a different stage than `pre-commit` — it receives the commit message file as its first argument and inspects its content.

**`scripts/hooks/commit-msg`:**

```bash
#!/usr/bin/env bash
# commit-msg: Enforces GIT_BRAIN_METADATA schema on every agent commit.

COMMIT_FILE="$1"
MSG=$(cat "$COMMIT_FILE")

if ! echo "$MSG" | grep -q "GIT_BRAIN_METADATA:"; then
  cat >&2 <<'BLOCK'

COMMIT BLOCKED: GIT_BRAIN_METADATA block is missing from the commit message.

Every agent commit must end with a metadata block. The key field is
retroactive_prompt — not the instruction you were given, but the instruction
you would write now, with full knowledge of what this change required, so that
another agent could reproduce it without asking questions.

Required format (append to the end of your commit message):

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

# Extract the JSON block between GIT_BRAIN_METADATA: and the closing -->
JSON=$(echo "$MSG" | awk '/GIT_BRAIN_METADATA:/{found=1; next} found && /-->/{exit} found{print}')

# Validate JSON structure and required fields using bun
echo "$JSON" | bun run scripts/hooks/validate-commit-metadata.mjs >&2
if [ $? -ne 0 ]; then
  exit 1
fi
```

**`scripts/hooks/validate-commit-metadata.mjs`:**

```javascript
// Reads JSON from stdin and validates the GIT_BRAIN_METADATA schema.
// Exits 1 with a descriptive error if validation fails.

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

### PR Readiness Hook (advisory, `post-commit`)

The `post-commit` hook runs after every successful commit. It counts the total files changed on the current branch vs. `main`. When that count reaches 10, the branch is due for a PR — the hook prints a console warning and appends an instruction to `docs/plans/next-prompt.md` so the agent carries it into the next task.

This is distinct from the per-commit size advisory (which counts staged files) and the pre-push hard block (which fires at 20). The post-commit warning is the early signal: you have accumulated enough change to form a coherent, reviewable PR — open one before the branch grows further.

**`scripts/hooks/post-commit`:**

```bash
#!/usr/bin/env bash
# post-commit: Warns when the branch has accumulated >= 10 file changes vs. main.

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

**Bootstrapping all hooks** — the scaffold step installs all hooks at once:

```bash
mkdir -p .git/hooks
for hook in prepare-commit-msg pre-commit commit-msg post-commit post-checkout pre-push; do
  cp scripts/hooks/$hook .git/hooks/$hook
  chmod +x .git/hooks/$hook
done
```

---

### Standards Refresh Hook (`post-checkout`)

The `post-checkout` hook fires after every branch switch. It runs `bootstrap-standards.sh` to ensure `docs/standards/` is current with the latest Calypso templates. This prevents an agent resuming work on a stale branch from operating with outdated conventions.

**`scripts/hooks/post-checkout`:**

```bash
#!/usr/bin/env bash
# post-checkout: Refreshes Calypso standards after switching branches.

PREV_HEAD="$1"; NEW_HEAD="$2"; BRANCH_CHECKOUT="$3"

[ "$BRANCH_CHECKOUT" != "1" ] && exit 0
[ "$PREV_HEAD" = "$NEW_HEAD" ] && exit 0

echo "post-checkout: refreshing Calypso standards..." >&2

if [ -f "./scripts/bootstrap-standards.sh" ]; then
  bash ./scripts/bootstrap-standards.sh
else
  curl -sSL https://raw.githubusercontent.com/dot-matrix-labs/calypso/main/scripts/bootstrap-standards.sh | bash
fi
```

---

### Pre-push Stage

The pre-push hook runs four checks in order.

**BLOCKS — Blueprint violations (architecture audit):**

Before any quality checks, the hook scans the repository for hard architectural violations: JavaScript files in `apps/` or `packages/`, forbidden packages in `package.json`, and a missing `docs/standards/` directory. These are the antipatterns from `AGENT.md` enforced deterministically at push time — catching anything that slipped past the `pre-commit` staged-diff scan across the full working tree.

**BLOCKS — Lint, format, and type errors:**

Code with lint, formatting, or TypeScript type errors must not be pushed. These were warned about at commit time; they must be resolved before the push is allowed.

**ALLOWS but annotates — Test suite failures:**

The full test suite always runs on push. If tests fail, the push is **not blocked** — the code reaches the remote. However, the hook appends the failing test names to `docs/plans/next-prompt.md` in the working tree with a mandatory instruction to address them. The next commit will be required to stage `next-prompt.md` (pre-commit gate), which forces the agent to acknowledge the failures.

Failing tests must be **checked, fixed, or rewritten. They must never be ignored or skipped.**

**`scripts/hooks/pre-push`:**

```bash
#!/usr/bin/env bash
# pre-push: Blocks on lint/format/type failures. Runs full test suite;
#           annotates next-prompt.md on failures but does not block push.

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
  echo "These were warned about at commit time. Fix them, update next-prompt.md, and commit." >&2
  echo "" >&2
  exit 1
fi

# PR size gate — block if too many files changed vs. main
PR_FILE_LIMIT=20
MERGE_BASE=$(git merge-base HEAD origin/main 2>/dev/null || git merge-base HEAD main 2>/dev/null || echo "")

if [ -n "$MERGE_BASE" ]; then
  PR_FILE_COUNT=$(git diff --name-only "$MERGE_BASE"...HEAD | wc -l | tr -d ' ')
  if [ "$PR_FILE_COUNT" -gt "$PR_FILE_LIMIT" ]; then
    echo "" >&2
    echo "PUSH BLOCKED: This PR changes ${PR_FILE_COUNT} files (limit: ${PR_FILE_LIMIT})." >&2
    echo "" >&2
    echo "Pull requests must be small and reviewable. Split this branch into smaller PRs:" >&2
    echo "  1. Identify a logical subset of changes that can stand alone." >&2
    echo "  2. Create a new branch from main with only those changes." >&2
    echo "  3. Open a PR for that subset, get it merged." >&2
    echo "  4. Repeat for the remaining changes." >&2
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

The following tests were failing at the time of the last push.
They must be **checked, fixed, or rewritten. Never ignore or skip them.**

\`\`\`
${FAILING}
\`\`\`

For each failure: determine whether the test is wrong (fix the test to match
correct behaviour) or the implementation is wrong (fix the code). Do not
disable, comment out, or add skip/todo markers to avoid addressing failures.

EOF

  echo "" >&2
  echo "WARNING: ${TEST_EXIT} test failure(s) detected. Push is proceeding, but" >&2
  echo "docs/plans/next-prompt.md has been updated with the failing tests." >&2
  echo "They must be resolved — not ignored — in the next commit." >&2
  echo "" >&2
fi

exit 0
```