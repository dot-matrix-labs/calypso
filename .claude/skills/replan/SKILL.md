---
name: replan
description: Read the Plan tracking issue and all open issues, build a dependency map, reprioritize by technical risk and unblocked status, tag work into parallel concurrency groups, and update each issue and the Plan with the results.
user_invocable: true
model: opus
---

# Replan

Read the current plan and all open issues, evaluate dependencies and technical risk,
and rewrite the plan with work grouped into sequenced concurrency batches. Update
each issue's Dependencies section with accurate dependents.

## Inputs

The user provides: $ARGUMENTS

$ARGUMENTS may contain a specific lens or constraint (e.g. "focus on the auth
subsystem" or "exclude phase 3"). If empty, replan everything.

---

## Setup

Before running any `gh` issue commands, detect the tasks repository:

```bash
TASKS_REPO=$(gh repo view --json nameWithOwner -q '(.owner.login) + "/" + (.name) + "-tasks"')
```

---

## Phase 1: Gather all open issues

### Step 1: Find the Plan tracking issue

```bash
gh issue list --repo {tasks-repo} --search "Plan" --state open --json number,title
```

Identify the issue with the exact title "Plan".

### Step 2: Read the Plan

```bash
gh issue view {plan-issue-number} --repo {tasks-repo} --json body -q .body
```

Extract every issue reference (`#{number}`) from the plan body. These are the
issues under active planning.

### Step 3: Fetch all open issues in full

For each issue number found in the plan, and also:

```bash
gh issue list --repo {tasks-repo} --state open --json number,title,body --limit 100
```

Read the full body of every open issue:

```bash
gh issue view {issue-number} --repo {tasks-repo} --json number,title,body,state -q '{number,title,body,state}'
```

Build an in-memory map: `issue_number → { title, body, dependencies: [], dependents: [] }`.

---

## Phase 2: Build the dependency graph

For each issue, parse its **Dependencies** section. Extract all `#{number}` references.

- An issue that lists `#{A}` in its Dependencies section **depends on** issue A.
- Issue A therefore has issue B as a **dependent**.

After parsing all issues:

1. Populate `dependencies[]` for each issue (what it needs before it can start).
2. Populate `dependents[]` for each issue (what is blocked until it is done).
3. Identify **dependency cycles** — if any exist, report them to the user and stop.
   Do NOT proceed with a cyclic graph.

---

## Phase 3: Assess technical risk

For each open issue, score its **technical risk** from 1 (low) to 5 (high) based on:

- **Cross-cutting concerns** — touches auth, data model, deployment, shared
  infrastructure, or core abstractions → higher risk
- **Interface changes** — modifies a public API, shared type, or contract used by
  multiple other issues → higher risk
- **Novelty** — introduces a pattern not yet present in the codebase → higher risk
- **Blast radius** — if it breaks, how many other things break with it → higher risk
- **Unknowns** — vague behaviour or acceptance criteria → higher risk

Record the score and a one-sentence rationale for each issue. You MUST justify each
score from the issue content — do not invent risk.

---

## Phase 4: Assign concurrency groups (batches)

Group issues into ordered **batches**. Each batch is a set of issues that:

1. Have no dependency on any other issue in the same batch.
2. Have no dependency on any issue in a later batch.
3. Do not touch the same subsystem or shared abstraction as other issues in the
   same batch (conservative — when in doubt, put in separate batches).

### Ordering rules (applied in priority order)

1. **Dependencies first** — an issue cannot be in batch N if any of its
   dependencies are in batch N or later.
2. **Highest technical risk first** — within a valid topological ordering, prefer
   to schedule high-risk issues earlier so their blast radius is absorbed before
   dependent work begins.
3. **Unblocked issues first** — prefer issues with no dependencies (or all
   dependencies already closed) over issues that are still waiting.
4. **Conservatism on concurrency** — concurrency is the exception, not the rule.
   Two issues may be in the same batch only if you are confident they do not
   interact. If uncertain, place them in separate sequential batches.

### What counts as "touching the same subsystem"

- Same source directory or module
- Same database table, schema, or migration
- Same API endpoint or shared type
- Same configuration surface
- Any shared abstraction one issue creates and another consumes

### Output format for batches

```
Batch 1 — {one-line description of the theme}
  - #{A} — {title} [risk: 4] — {one-sentence rationale}
  - #{B} — {title} [risk: 3] — {one-sentence rationale}

Batch 2 — {one-line description of the theme}
  - #{C} — {title} [risk: 5] — {one-sentence rationale}
  ...
```

Issues in the same batch CAN be worked in parallel (they do not block each other and
do not share a subsystem). Issues in different batches MUST be done sequentially
(later batches depend on earlier ones completing).

---

## Phase 5: Update issue titles and labels

For every open issue being replanned, replan is the authoritative source of batch
assignment and scope tagging. Apply both a title prefix and a GitHub label.

### Step 1: Ensure batch labels exist

For each batch number N in the new plan, ensure the label `batch-N` exists:

```bash
gh label list --repo {tasks-repo} --json name -q '.[].name'
```

Create any missing labels:

```bash
gh label create batch-{N} --repo {tasks-repo} --color "0075ca" --description "Batch {N} in the current plan"
```

### Step 2: Update issue titles

Each issue title MUST be prefixed with its scope type if it is not already. Use the
conventional commit prefix that best describes the issue:

- `feat:` — new capability
- `fix:` — bug or incorrect behaviour
- `chore:` — non-user-facing maintenance
- `refactor:` — structural change with no behaviour change
- `docs:` — documentation only

If the title already starts with one of these prefixes, leave it as-is. If it does
not, prepend the appropriate prefix based on the issue content.

```bash
gh issue edit {issue-number} --repo {tasks-repo} --title "{new-title}"
```

Only update the title if it actually changes. Show the user what will change before
editing.

### Step 3: Apply batch label and remove stale batch labels

For each issue, remove any existing `batch-*` labels and apply the correct one for
its new batch assignment:

```bash
# Remove stale batch labels
gh issue edit {issue-number} --repo {tasks-repo} --remove-label "batch-{old}"

# Apply current batch label
gh issue edit {issue-number} --repo {tasks-repo} --add-label "batch-{N}"
```

If an issue has moved batches since the last replan, note this in the Phase 7 report.

---

## Phase 7: Update issue bodies with accurate dependents

For each issue where the **Dependents** list has changed (issues that depend on this
one), update the issue body.

Add or update a **Dependents** section immediately after the Dependencies section:

```markdown
## Dependents

_{Issues that cannot start until this one is closed}_

- #{X} — {title}
- #{Y} — {title}
```

If no issues depend on this one:

```markdown
## Dependents

None.
```

Update each issue:

```bash
gh issue edit {issue-number} --repo {tasks-repo} --body "{updated body}"
```

**IMPORTANT:**
- Preserve all existing sections — only add or update the Dependents section.
- Do NOT change the title, Stage, Acceptance criteria, Test plan, or any other section. Title changes happen in Phase 5.
- Show the user a diff of what will change for each issue before editing.
- Ask for confirmation before applying any edits.

---

## Phase 8: Rewrite the Plan tracking issue

Rewrite the Plan tracking issue body with the new batch structure. The new format:

```
Planned implementation order for all outstanding features. Batches are sequenced
by dependency and technical risk. Issues within a batch can be worked in parallel
if capacity allows, but sequential execution is the default assumption.

> Last replanned: {date}

**Batch 1 — {description}**
- #{A} — {title} [risk: 4]
- #{B} — {title} [risk: 3]

**Batch 2 — {description}**
- #{C} — {title} [risk: 5]

...
```

Rules for the rewritten plan:
- Use `**Batch N — {description}**` headings (replacing any prior Phase/Batch headings).
- Entries are plain list items — **NEVER checkboxes**. A `- [ ]` or `- [x]` MUST NOT appear anywhere in the Plan body. Completion is determined solely by whether the linked GitHub issue is open or closed.
- Include the `[risk: N]` annotation on each entry.
- Preserve any cross-cutting block-quote notes from the prior plan that are still relevant.
- Do NOT include already-closed issues.
- Before submitting the edit, scan the proposed body for `[ ]` or `[x]` — if any are found, remove them and replace with plain `- ` list items.

Show the user the full proposed new body before editing. Ask for confirmation.

```bash
gh issue edit {plan-issue-number} --repo {tasks-repo} --body "{new body}"
```

---

## Phase 9: Report

Output a structured summary:

```
## Replan summary

**Issues analysed:** {N}
**Dependency cycles detected:** {none / list them}
**Batches:** {N}

### Batch breakdown

{Repeat the batch table from Phase 4}

### Title changes

- #{A}: "old title" → "feat: new title"
- #{B}: no change

### Label / batch changes

- #{A}: batch-1 → batch-2
- #{B}: (new) → batch-1
- #{C}: no change

### Issues updated with dependents

- #{A}: added dependents #{X}, #{Y}
- #{B}: no change

### Risk highlights

Top 3 highest-risk issues (address early):
1. #{N} — {title} [risk: 5] — {rationale}
2. ...
3. ...
```

---

## Rules

- **Replan owns titles and batch labels** — this skill is the only thing that should set `batch-*` labels and scope-prefix issue titles. Other skills must not modify these.
- **Never invent dependencies** — only parse what is written in the issue body.
- **Never invent risk** — justify every score from the issue content.
- **Conservative concurrency** — when in doubt, sequential is correct.
- **User confirms before writes** — show diffs and ask before editing any issue or the Plan.
- **Dependency cycles are fatal** — stop and report; do not attempt to replan a cyclic graph.
- **Closed issues are ignored** — do not include closed issues in the new plan.
- **No checkboxes in the Plan — ever** — the Plan body must never contain `- [ ]` or `- [x]`. Task completion is read from issue state (open/closed), not from checkbox markup. If you find yourself writing a checkbox, stop and use a plain list item instead.
- **`gh` CLI only** — all GitHub operations use the gh CLI.
- **Plan is the source of truth** — `docs/plan.md` (if it exists) is NOT updated; the GitHub issue is authoritative.
