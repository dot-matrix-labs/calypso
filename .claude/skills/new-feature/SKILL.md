---
name: new-feature
description: Walk through the calypso feature specification workflow — intake, architecture evaluation, GitHub issue creation, and product tracking issue update.
user_invocable: true
model: opus
---

# Feature Specification Flow

You are executing the calypso feature specification workflow. This skill walks the
user through the full `calypso-feature-request` state machine: intake, architecture
evaluation, GitHub issue creation, and product tracking issue update.

## Inputs

The user provides: $ARGUMENTS

If $ARGUMENTS is empty, ask the user for:

1. **Feature name** — short identifier (e.g. "merge-queue-workflow")
2. **Motivation** — why this feature is needed
3. **Intended experience** — what the user/operator can do once it ships
4. **Known constraints** — scope limits, approach restrictions, things explicitly out of scope

Do NOT proceed until all four are provided.

---

## Setup

Before running any `gh` issue commands, detect the tasks repository:

```bash
TASKS_REPO=$(gh repo view --json nameWithOwner -q '(.owner.login) + "/" + (.name) + "-tasks"')
```

Use `$TASKS_REPO` (or `{tasks-repo}` in command templates below) wherever `--repo` is needed for issue operations.

---

## Phase 1: Intake validation

Confirm the request has:

- [ ] A clear motivation (not just "it would be nice")
- [ ] A concrete intended experience (observable behaviour, not implementation detail)
- [ ] At least one constraint or explicit scope boundary

If any are missing, ask the user to clarify. Do NOT invent missing information.

---

## Phase 2: Context gathering and architecture evaluation

Before proposing anything, read ALL of the following sources to build a complete
picture. The feature must be consistent with all three: the PRD, the current plan,
and the architecture.

### Step 1: Read the PRD

Read `docs/prd.md` — the product requirements document. Understand the product
vision, target users, and priorities. The proposed feature must align with the PRD.
If it contradicts or falls outside the PRD's scope, flag this to the user.

### Step 2: Read the Plan tracking issue

Fetch the current "Plan" tracking issue to understand what has been built, what is
in progress, and what is planned next:

```bash
gh issue list --repo {tasks-repo} --search "Plan" --state open --json number,title
```

Then read its body:

```bash
gh issue view {plan-issue-number} --repo {tasks-repo} --json body -q .body
```

Understand the current phases, completed features, and pending work. The new feature
must fit coherently into this plan — it should not duplicate, conflict with, or
undermine existing or planned features.

### Step 3: Read architecture files

Read the following files to evaluate against current architecture:

- `calypso-blueprint/blueprints/calypso-blueprint.md` — canonical architecture
- `calypso-blueprint/rules/blueprints/arch.yaml` — architecture rules
- `calypso-blueprint/rules/blueprints/process.yaml` — process rules
- `calypso-blueprint/examples/workflows/calypso-feature-request.yaml` — this workflow definition

### Step 4: Evaluate

With full context from the PRD, the Plan, and the architecture, evaluate:

1. **PRD alignment** — does this feature serve the product vision and stated priorities?
2. **Plan coherence** — does it fit logically into the current implementation plan? Does it depend on unfinished work? Does it conflict with planned features?
3. **Feasibility** — does it preserve existing invariants? Does it conflict with any architecture rule?
4. **Clarity** — is the request specific enough to implement without guessing?
5. **Risk** — does it touch cross-cutting concerns (auth, data model, deployment)?

Report your evaluation to the user with one of three outcomes:

- **Clear** — proceed to issue creation
- **Clarification needed** — list exactly what is missing and ask the user
- **Architecture conflict** — explain the conflict and ask the user how to resolve it

---

## Phase 3: Create GitHub issue

### What you MUST do

Create a GitHub issue on `{tasks-repo}` using `gh issue create` with the
following structure. Every section is required.

```
## Motivation

{Why this feature is needed. Written from the user/operator perspective, not
implementation perspective. Reference the problem being solved.}

## Behaviour

{Concrete, observable behaviour. What happens when the feature works. Include:
- Entry conditions / triggers
- Step-by-step expected behaviour
- Edge cases and error paths
- Any state machine states if applicable}

## Dependencies

{List any issues that must be completed before this one can start. Use `#{number} — {title}` format. If none, write "None."}

## Scope

{What is IN scope and what is explicitly OUT of scope. Be specific.}

## Acceptance criteria

- [ ] {Testable criterion 1}
- [ ] {Testable criterion 2}
- [ ] {Each criterion must be independently verifiable}

## Test plan

- [ ] {Specific test scenario 1}
- [ ] {Specific test scenario 2}

## Stage

**Current:** Specified
```

### Template correctness checks

Before creating the issue, verify the template is correct:

- [ ] **Motivation** is not empty and describes the "why", not the "how"
- [ ] **Behaviour** is concrete — no vague language like "should work well" or "handle errors gracefully"
- [ ] **Dependencies** lists blocking issues (with `#{number} — {title}`) or explicitly states "None."
- [ ] **Scope** has at least one explicit exclusion (out-of-scope item)
- [ ] **Acceptance criteria** has at least 2 checkboxes, each independently testable
- [ ] **Test plan** has at least 2 checkboxes describing specific test scenarios
- [ ] **Stage** is set to "Specified"
- [ ] No section is missing or empty
- [ ] Title is concise (under 80 chars), prefixed with scope (e.g. `feat: ...`, `fix: ...`)

If any check fails, fix it before creating the issue. Show the user the full issue
body and ask for confirmation before running `gh issue create`.

### What you MUST NOT do

- Do NOT create the issue without showing the user the full body first
- Do NOT skip any section of the template
- Do NOT use vague acceptance criteria ("it works", "no errors")
- Do NOT add labels unless the user specifies them
- Do NOT assign the issue unless the user specifies an assignee
- Do NOT create duplicate issues — search existing issues first with `gh issue list --search "{feature name}"`

### Creating the issue

```bash
gh issue create \
  --repo {tasks-repo} \
  --title "{title}" \
  --body "{body}"
```

Report the issue number and URL to the user.

---

## Phase 4: Ensure product tracking issue exists and add feature

The product tracking issue is a single, unique GitHub issue titled **"Plan"** that
tracks all features organized by phase. There must be exactly one.

### Step 1: Find the tracking issue

```bash
gh issue list --repo {tasks-repo} --search "Plan" --state open --json number,title
```

Look for an issue with the exact title "Plan". If multiple matches, pick the one
whose body contains phase headings.

### Step 2: If no tracking issue exists — create one

If no "Plan" issue exists, create it using this exact template.

```
Planned implementation order for all outstanding features. Each issue builds on the ones before it.

**Phase 1 — {description}**
{feature entries go here}

**Phase 2 — {description}**
{feature entries go here}
```

Phase names and descriptions are determined by the current state of the project.
Ask the user what the first phase should be called and what it covers. Do NOT
invent phase names.

### What you MUST know about the tracking issue format

- Phase headings are bold text: `**Phase N — {description}**`
- Feature entries are plain list items (NO checkboxes): `- #{issue} — {description}`
- Completion is tracked by issue state (open/closed via PR), NOT by checkboxes
- Features within a phase are ordered by dependency (earlier entries are prerequisites)
- Dependencies are noted inline with `_(requires #{other-issue})_` suffix
- Block quotes for cross-cutting notes (e.g. coverage gates) go above the relevant phase

### Step 3: Add the new feature to the tracking issue

1. Fetch the current tracking issue body:

   ```bash
   gh issue view {plan-issue-number} --repo {tasks-repo} --json body -q .body
   ```

2. Determine the correct phase for the new feature. Consider:
   - What existing features does this depend on?
   - What phase are its dependencies in?
   - The new feature goes in the same phase as or after its latest dependency

3. Ask the user which phase the feature belongs in. Show the current phases and
   suggest a placement. Do NOT place the feature without user confirmation.

4. Add the entry as a plain list item under the chosen phase (NO checkboxes — completion is tracked by issue state):

   ```
   - #{new-issue} — {Feature title} _(requires #{dependency})_
   ```

   Omit the `_(requires ...)_` suffix if there are no dependencies.

5. Update the tracking issue:
   ```bash
   gh issue edit {plan-issue-number} --repo {tasks-repo} --body "{updated body}"
   ```

### What you MUST NOT do in this phase

- Do NOT create a second tracking issue if one already exists
- Do NOT change the title of the tracking issue
- Do NOT reorder existing features without asking the user
- Do NOT add checkboxes to the tracking issue — completion is tracked by issue state (open/closed via PR)
- Do NOT remove or rename existing phases
- Do NOT add a phase without asking the user
- Do NOT place the feature in a phase without user confirmation
- Do NOT modify `docs/plan.md` — the GitHub issue is the source of truth

---

## The 1:1:1:1:1 invariant

Every feature maintains a strict one-to-one mapping across five resources:

**1 issue : 1 branch : 1 PR : 1 subagent : 1 worktree**

- The GitHub issue created in Phase 3 is the single source of truth for the feature.
- Implementation happens on exactly one branch named after the feature.
- That branch produces exactly one PR.
- Exactly one subagent is assigned to implement the feature, running in its own isolated git worktree.
- The subagent MUST use `isolation: "worktree"` — it never works in the main checkout.

This invariant is non-negotiable. If any of these resources are shared across
features or duplicated within a feature, the process is broken. The skill does NOT
launch the subagent or create the branch/worktree — that happens downstream in the
default feature workflow. But every feature issue created here will eventually map
to exactly one of each.

---

## Completion

When done, report:

1. Feature issue number and URL
2. Tracking issue number and what phase the feature was added to
3. Suggested next step (usually: "ready for implementation via the default feature workflow")

---

## Rules this skill enforces

- GitHub Issues are the source of truth for planning
- Feature issues have structured sections: Motivation, Behaviour, Dependencies, Scope, Acceptance criteria, Test plan, Stage
- The product tracking issue links to feature issues as plain list items (no checkboxes — completion tracked by issue state)
- One feature = one issue = one branch = one PR = one subagent = one worktree
- `gh` CLI is the required GitHub surface for all GitHub operations
- Architecture evaluation must pass before issue creation
- No self-attestation — user confirms before creation
- Plans are living documents updated at every change
