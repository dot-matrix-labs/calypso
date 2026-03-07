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

### Pre-commit Stage

Pre-commit nags run before a commit is finalized. They are designed to:

- **Auto-fix issues** where possible (formatting, simple linting)
- **Not block** the commit on non-critical issues
- **Run quickly** to not interrupt developer flow

**Default pre-commit nags by project type:**

| Project Type | Tool Nags                            |
| ------------ | ------------------------------------ |
| Node.js/Bun  | `prettier --write`, `eslint --fix`   |

### Planning Documents Gate (blocking)

Every commit must stage updates to both planning documents. This is a hard block.

| File | What to update |
|---|---|
| `docs/plans/implementation-plan.md` | Check off completed tasks; add or reorder tasks discovered during the work |
| `docs/plans/next-prompt.md` | Overwrite with the single, self-contained prompt for the next commit to execute |

`next-prompt.md` is the state machine cursor. A commit is the unit of work in Calypso. Each commit ends by writing the prompt for the next commit. An agent session spans many commits and can execute them continuously by reading and acting on this file after each commit, without waiting for a human prompt.

Add the following to `.git/hooks/pre-commit`:

```bash
#!/usr/bin/env bash
# Planning documents gate — every commit must update both planning files.

STAGED=$(git diff --cached --name-only)
ERRORS=()

if ! echo "$STAGED" | grep -q "^docs/plans/implementation-plan\.md$"; then
  ERRORS+=("docs/plans/implementation-plan.md")
fi

if ! echo "$STAGED" | grep -q "^docs/plans/next-prompt\.md$"; then
  ERRORS+=("docs/plans/next-prompt.md")
fi

if [ ${#ERRORS[@]} -gt 0 ]; then
  echo "" >&2
  echo "COMMIT BLOCKED: The following planning files were not updated:" >&2
  for f in "${ERRORS[@]}"; do
    echo "  - $f" >&2
  done
  echo "" >&2
  echo "At every commit:" >&2
  echo "  implementation-plan.md — check off completed tasks; add or reorder discovered tasks." >&2
  echo "  next-prompt.md         — write the complete, self-contained prompt for the next" >&2
  echo "                           commit to execute. It must be specific enough to act on" >&2
  echo "                           without human input. A commit is the unit of work;" >&2
  echo "                           this is how the agent advances from one commit to the next." >&2
  echo "" >&2
  exit 1
fi
```

**Bootstrapping the hook:** The scaffold step must install this hook automatically so it is active from the first commit:

```bash
mkdir -p .git/hooks
cp scripts/hooks/pre-commit .git/hooks/pre-commit
chmod +x .git/hooks/pre-commit
```

Store the canonical hook source in `scripts/hooks/pre-commit` so it is version-controlled and re-installable by any agent or developer who clones the repository.

### Metadata Enforcement Hook (blocking, `commit-msg`)

The `commit-msg` hook fires after the commit message is written and validates that `GIT_BRAIN_METADATA` is present and schema-valid. The commit is rejected if the block is missing, the JSON is malformed, or any required field is absent or empty.

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

**Bootstrapping both hooks** — the scaffold step installs all hooks at once:

```bash
mkdir -p .git/hooks
for hook in pre-commit commit-msg; do
  cp scripts/hooks/$hook .git/hooks/$hook
  chmod +x .git/hooks/$hook
done
```

### Pre-push Stage

Pre-push nags run before code is pushed to remote. They are designed to:

- **Strictly validate** all code quality standards
- **Block push** if any check fails
- **Include slow checks** that aren't appropriate for pre-commit

**Default pre-push nags by project type:**

| Project Type | Tool Nags                                          |
| ------------ | -------------------------------------------------- |
| Node.js/Bun  | `tsc --noEmit`, `eslint`, `prettier --check`       |