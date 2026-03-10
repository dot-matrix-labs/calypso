# Agent Communication

> [!IMPORTANT]
> This document governs how every file in `agent-context/` is written. It applies to all authors — human and agent. Before creating or editing any document in `agent-context/`, read this document in full.

---

## Part 1: Effective Agent Communication

Documentation written for agents is not the same artifact as documentation written for humans. Human documentation optimizes for narrative flow, explanation of reasoning, and gradual conceptual onboarding. Agent documentation must optimize for constraint clarity, deterministic interpretation, and zero-ambiguity rule encoding. An agent does not skim, infer intent from tone, or fill gaps from organizational memory. It executes what is written. Soft, incomplete, or prose-buried rules produce soft, incomplete, or inconsistent output.

The sections below define the principles and patterns that apply to every document type in `agent-context/`. Document-type-specific rules follow in Parts 2–4.

---

### Foundational Principles

#### Use normative language

MUST, MUST NOT, REQUIRED, FORBIDDEN, ONLY, NEVER carry a precise meaning agents interpret consistently. Soft equivalents — "should", "generally", "typically", "preferred", "often" — produce inconsistent behavior. If a rule is mandatory, write MUST. If a behavior is prohibited, write FORBIDDEN. Soft language is permitted only to describe genuinely optional choices, and only when accompanied by the conditions under which each option applies.

#### Lead with the constraint

An agent follows a constraint more reliably than a rationale. State the rule first. Rationale belongs in the document but must follow, not precede, the constraint it supports. Explanation that precedes the rule produces agents that understand the reasoning but miss the instruction.

#### Use constraint blocks for non-obvious rules

Rules embedded in prose paragraphs are frequently skipped or underweighted. Encode each non-obvious constraint as a structured block:

```
RULE:     [The constraint, stated imperatively.]
RATIONALE: [One to three sentences explaining why.]
CORRECT:  [Compliant code or pseudocode.]
INCORRECT: [Violating code or pseudocode.]
CHECKLIST:
  - [ ] [Binary-verifiable condition]
```

Agents parse structured blocks more reliably than inline rules. Fixed field labels allow the agent to locate constraint, justification, and verification conditions without reading surrounding narrative.

#### Always include examples

Agents replicate patterns from code more reliably than they derive behavior from abstract rules. Every non-obvious constraint MUST include a correct and an incorrect implementation. Without examples, the agent defaults to training-data patterns — which reflect the distribution of code on the internet, not the decisions of this project.

#### One document, one concern

Documents scoped to a single domain allow the agent to load precisely the rules relevant to the task at hand. A 3,000-line monolith requires the agent to weight competing rules across unrelated domains. Scope each document tightly. When a document exceeds 300 lines, audit it: split sub-topics into separate files.

#### Start every document with a context map

Agents operate under context-window limits. Prior content may be truncated or compressed at any time. Every document in `agent-context/` MUST begin — immediately after its title and any callout banners — with a **Context Map**: a compact dependency graph showing how this document relates to other documents, rendered in ASCII:

```
CONTEXT MAP
  this ──implements──▶ blueprints/data-blueprint.md
  this ──requires────▶ blueprints/calypso-blueprint.md §2 (dependency policy)
  this ◀──referenced by── development/development-standards.md
```

The context map serves two purposes:
1. If the body of the document is truncated, the agent retains enough structure to know what other documents to load.
2. It doubles as a routing hint — an agent scanning for "where does data validation live?" can read context maps without loading full documents.

Rules:
- Use directed edges with labeled relationships: `implements`, `requires`, `extends`, `referenced by`, `supersedes`
- MUST list every document this document directly depends on or is depended on by
- Keep it under 15 lines. If it exceeds 15, the document's scope is too broad — split it.

#### Cover every boundary

Undocumented behavior is not prohibited behavior — it is unpredictable behavior. Any area of the system not covered by `agent-context/` is an area where the agent will invent conventions. The goal is complete boundary coverage: every domain the agent touches must have at least one document that governs it.

#### Provide a gap-filling heuristic

No documentation set can enumerate every decision. When an agent encounters a gap, it falls back on training data. Provide an explicit fallback hierarchy:

```
When uncertain how to implement something:
1. Search agent-context/ for an analogous pattern.
2. Copy the closest existing implementation in the codebase.
3. Choose the simplest solution that satisfies the immediate requirement.
4. Do not introduce a new architectural pattern without explicit instruction.
```

The heuristic does not solve the gap — it constrains how the agent fills it.

---

### Common Failure Modes

- **Soft mandate.** Using "should" or "preferred" for rules that are actually required. MUST is the correct word.
- **Rule burial.** Constraints inside multi-paragraph prose are frequently skipped. Use constraint blocks.
- **Gap silence.** Leaving a domain undocumented. Training-data defaults will fill it — unpredictably.
- **Example-free rules.** Without code examples, agents invent what "compliant" looks like.
- **Contradictory cross-document rules.** Two documents defining the same concept differently produce non-deterministic behavior. One must be canonical; the other must cross-reference it. See the Document Precedence Rules below.
- **Aspirational documentation.** Describing intended architecture rather than actual current architecture. An agent building against the intended state produces code inconsistent with the real codebase.
- **Stale content.** Templates, antipatterns, and technology lists go stale. Every architectural decision that changes MUST trigger a review of all affected documents.

---

### Document Header

Every document in `agent-context/` MUST begin with a metadata block immediately after the title:

```
# [Document Title]

<!-- last-edited: YYYY-MM-DD -->

CONTEXT MAP
  this ──implements──▶ [path]
  this ──requires────▶ [path]
  ...
```

The `last-edited` date MUST be updated on every commit that modifies the document. This date is used for conflict resolution (see below) and staleness detection. An agent encountering a document with no `last-edited` date MUST treat its content as potentially stale and cross-check against the codebase before relying on it.

---

### Document Precedence Rules

When two documents make conflicting statements, resolve the conflict using these rules in order:

1. **Specificity wins.** An implementation document overrides its parent blueprint for technology-specific decisions. A development document overrides both for workflow-specific decisions.
2. **Explicit override wins.** If a document states "this supersedes [other document] for [topic]," that statement governs.
3. **Last-edited wins.** When two documents at the same specificity level conflict and neither explicitly overrides the other, the most recently edited document is authoritative. This reflects the most current project decisions.
4. **Escalate.** If precedence is still ambiguous, the agent MUST stop and ask the human rather than choose arbitrarily.

The full precedence stack, from highest to lowest authority:
```
development documents  (how to execute — most specific)
implementation documents  (what to build)
blueprints  (why and what — most general)
```

Within a tier, the document whose `last-edited` date is more recent wins.

---

### Document Discovery

All documents in `agent-context/` are indexed in `agent-context/index.md`. Before starting any task, an agent MUST:

1. Read `agent-context/index.md`.
2. Identify the documents relevant to the task by scanning the keyword index and dependency graph.
3. Load those documents — and their transitive dependencies shown in each document's Context Map.

`agent-context/index.md` MUST be updated whenever a document is added, removed, or changes its scope or dependencies.

---

## Part 2: Blueprint Documents

### Purpose

A blueprint is a conceptual document for a technical domain. It begins at the problem level, establishes why a domain requires deliberate design, works through architectural principles and patterns, and ends with one concrete reference implementation in the Calypso TypeScript stack.

Blueprints are the "why and what" layer. They are read by agents to understand the reasoning behind architectural decisions so that when an agent encounters an undocumented situation, it can reason from first principles rather than invent arbitrarily.

A blueprint is not:
- A tutorial for a specific library
- Documentation for a specific project or deployment
- A checklist for a specific task

A blueprint is:
- Reusable across organizations and tech stacks
- Opinionated about principles, not about vendors
- Loosely coupled to the Calypso TypeScript reference implementation, which is one concrete example — not the only valid one

**Decoupling test:** Remove every TypeScript, Bun, React, and PostgreSQL reference from sections 1–5. Does the document still make complete sense? If yes, sections 1–5 are correctly decoupled. If no, find the implementation specifics that leaked and move them to section 6.

---

### Required Sections

Every blueprint MUST contain the following eight sections in this order. Sections may be expanded; they MUST NOT be removed or reordered.

---

#### 1. Vision

**Purpose:** Establish the why. What problem does this blueprint address? What does the world look like when it is correctly applied? What failure modes does it prevent?

**Rules:**
- Two to five paragraphs. MUST NOT be a bullet list.
- Technology-agnostic. No library names, no specific products.
- Written in declarative present tense: "Every application defaults to…", not "You should try to…"
- MUST include at least one concrete consequence of not applying the blueprint.

**Antipatterns:**
- Vision that merely restates the section title
- Vision that lists implementation choices ("We will use AES-256 and Vault")
- Vision written as aspirational marketing copy with no technical grounding

---

#### 2. Threat Model

**Purpose:** Define the adversaries, failure modes, or problem conditions the blueprint addresses. Every design choice made later in the document MUST trace back to a row in this section.

**Rules:**
- Use a table with at least two columns: scenario/adversary and what must be protected under that condition
- Be concrete and specific: "disk image exfiltrated" not "data breach"
- Minimum five rows; no maximum
- A design choice with no corresponding row in this table does not belong in the blueprint

**For security blueprints:** threat actors and attack scenarios.
**For operational blueprints:** failure modes and degraded conditions.
**For process blueprints:** quality failure modes and their downstream consequences.

**Antipatterns:**
- Threat model listing only obvious, generic risks with no specificity
- Threat model never referenced again in the document
- Skipping this section for "non-security" blueprints — every domain has a problem space

---

#### 3. Core Principles

**Purpose:** State the non-negotiable architectural principles that govern every decision in this domain. These are the axioms from which the design patterns are derived.

**Rules:**
- Three to seven principles, no more
- Each principle is a short, declarative sentence followed by one paragraph of rationale
- Principles MUST be falsifiable: "encrypt all data" is not a principle; "a compromise of any single layer must not yield plaintext at any other layer" is
- Principles MUST be tech-stack-agnostic

**Antipatterns:**
- Principles that are really implementation steps ("Use AES-256-GCM for field encryption")
- Principles that contradict each other without acknowledging the trade-off
- More than seven principles (real principles are being diluted by preferences)

---

#### 4. Design Patterns

**Purpose:** Document the recurring solutions to recurring problems in this domain.

**Rules for each pattern:**
- Name it. A pattern without a name cannot be referenced, discussed, or violated with precision.
- State the problem it solves in one sentence.
- Describe the solution in implementation-agnostic language.
- Show a concrete example in pseudocode or generic notation if the pattern has a non-obvious shape.
- State the trade-offs explicitly: when does this pattern become the wrong choice?

**Rules for the section:**
- Minimum three patterns; no maximum
- Patterns are ordered from most fundamental to most specialized
- Each pattern MUST map to at least one row in the Threat Model
- Patterns that only apply to the Calypso TypeScript stack belong in the Reference Implementation section, not here

**Antipatterns:**
- Patterns that are library API walkthroughs ("call `kms.encrypt()` with the key ID")
- Patterns with no named trade-off
- Implementation-specific code in this section (TypeScript, SQL, YAML belong in section 6)

---

#### 5. Plausible Architectures

**Purpose:** Show two to four concrete system designs that apply the principles and patterns correctly, at different scales or under different constraints.

**Rules:**
- Each architecture has a name and a brief characterization of when it is appropriate
- Use ASCII block diagrams for system topology — not prose descriptions
- Explicitly note what each architecture trades off against the others
- Architectures MUST use role-based component names — not vendor names — in their labels
- A section at the bottom of each architecture may optionally list concrete product options

**Naming convention:**

| Role-based (correct) | Product-specific (wrong) |
|---|---|
| Key Store | HashiCorp Vault |
| Relational Database | PostgreSQL |
| Object Storage | S3 |
| Message Queue | Kafka |
| Container Orchestrator | Kubernetes |

Section 6 may and should use concrete product names.

**Antipatterns:**
- Only one architecture (blueprints are about design space, not single answers)
- Architectures that are identical except for scale (missing a genuine trade-off)
- Architectures drawn around specific vendor products rather than component roles

---

#### 6. Reference Implementation — Calypso TypeScript

**Purpose:** Translate the principles, patterns, and architectures into a concrete implementation using the Calypso standard stack: TypeScript, Bun, React, and PostgreSQL.

**Rules:**
- The introductory paragraph MUST include: "The following is the Calypso TypeScript reference implementation. The principles and patterns above apply equally to other stacks; this section illustrates one concrete realization."
- Package structure: show where domain code lives within the Calypso monorepo layout
- Interfaces: define the key TypeScript interfaces and types
- Buy vs. DIY table: for every external dependency introduced, justify whether it is Buy or DIY per the Calypso dependency policy (`calypso-blueprint.md §2`)
- Code examples MUST be minimal and illustrative — no full module implementations, no inline tests

**Antipatterns:**
- Reference implementation so detailed it overshadows the conceptual sections
- TypeScript-specific concepts in sections 1–5
- Listing packages without Buy/DIY justification

---

#### 7. Implementation Checklist

**Purpose:** Provide a verifiable, staged checklist that confirms correct application of the blueprint.

**Rules:**
- Three stages minimum: Alpha (functional baseline), Beta (production-hardened), V1 (fully operational)
- Each item MUST be a concrete, binary-verifiable action — not a description of a goal
  - Correct: `[ ] JWT expiry enforced and tested; algorithm pinned to ES256`
  - Wrong: `[ ] Implement secure authentication`
- Items are written in imperative or past-tense-passive
- The Alpha checklist MUST be the minimum gate before any real data enters the system

**Antipatterns:**
- Checklist items that cannot be verified by a third party
- Items that restate principles rather than test them
- A single flat checklist with no staging

---

#### 8. Antipatterns

**Purpose:** Name the specific wrong approaches that look plausible but violate the blueprint's principles.

**Rules:**
- Minimum five antipatterns; no maximum
- Each antipattern has a short name (bold), followed by one to three sentences explaining what it is and why it is wrong
- Antipatterns MUST be concrete and tempting — if no one would do it, it is not worth documenting
- Each antipattern implicitly maps to a principle or pattern it violates

**Antipatterns:**
- Antipatterns that are just "don't be lazy" advice
- Antipatterns already covered by the Calypso blueprint without additive specificity
- Antipatterns that prescribe a specific vendor as the wrong choice (keep it structural)

---

### Section Length Guidelines

| Section | Minimum | Maximum |
|---|---|---|
| Vision | 2 paragraphs | 5 paragraphs |
| Threat Model | 5 table rows | Unlimited |
| Core Principles | 3 items | 7 items |
| Design Patterns | 3 patterns | Unlimited |
| Plausible Architectures | 2 architectures | 4 architectures |
| Reference Implementation | Package structure + interfaces + Buy/DIY table | No code block longer than 30 lines |
| Implementation Checklist | 10 items (Alpha minimum) | Unlimited |
| Antipatterns | 5 items | Unlimited |

---

### Cross-Blueprint Consistency

Blueprints reference each other. When a blueprint depends on a concept defined in another blueprint, cite it explicitly:

> "Agent authentication is defined in the Data Security Blueprint. This blueprint assumes that mechanism is in place."

NEVER duplicate concepts across blueprints. If a concept appears in two blueprints, one is canonical and the other references it.

---

### Blueprint Review Checklist

Before merging a new or revised blueprint:

- [ ] `<!-- last-edited: YYYY-MM-DD -->` is present and current
- [ ] Context Map is present after the title
- [ ] All eight required sections are present and in order
- [ ] Vision contains no library or product names
- [ ] Every design pattern maps to at least one threat model row
- [ ] Sections 1–5 pass the decoupling test (no TypeScript-specific concepts)
- [ ] Architectures use role-based component names
- [ ] Reference Implementation section is explicitly labeled as one realization, not the definitive one
- [ ] Buy/DIY table is present in the Reference Implementation section
- [ ] All checklist items are binary-verifiable
- [ ] Checklist has at least three maturity stages
- [ ] Antipatterns are concrete and tempting (not obvious advice)
- [ ] No section exceeds the maximum length guidelines
- [ ] No concepts are duplicated from another blueprint without an explicit cross-reference

---

## Part 3: Implementation Documents

### Purpose

An implementation document is the technology layer. Where a blueprint answers "what is the right design and why," an implementation document answers "exactly what code to write, what packages to use, and what patterns to follow in this stack." An agent reading an implementation document should finish with zero ambiguity about what to build.

Implementation documents are not aspirational. They describe the current, actual, sanctioned implementation — not the intended future state. Every specification in an implementation document is a constraint, not a suggestion.

---

### Required Sections

Every implementation document MUST contain the following sections. Sections may be expanded; they MUST NOT be removed.

---

#### Blueprint Reference

The first line of every implementation document MUST name its corresponding blueprint:

```
> Implements: [Blueprint Name] (`agent-context/blueprints/[name]-blueprint.md`)
```

An implementation document without a blueprint reference is a free-floating specification with no principled foundation.

---

#### Stack Specification

State the exact technologies used in this domain. No alternatives, no "you could also use." This section eliminates all discretion about tooling:

```
REQUIRED stack for [domain]:
- Language: TypeScript (strict mode)
- Runtime: Bun
- Database client: [exact package and version constraint]
- Test framework: Vitest
- [etc.]
```

If a technology choice is contested or has a known alternative, note it once with the reason the alternative was rejected, then never mention it again.

---

#### Package Inventory

List every external package used in this domain with its justification. MUST use the Buy/DIY format from the Calypso dependency policy:

| Package | Version constraint | Classification | Justification |
|---|---|---|---|
| `[name]` | `[semver]` | Buy / DIY | [Why this is not written in-house] |

FORBIDDEN: adding a package not listed in this table. If a needed package is absent, add it here with justification before using it.

---

#### Module Structure

Show the exact file and directory layout for this domain within the Calypso monorepo:

```
packages/[domain]/
  src/
    [module-a]/
      index.ts       — public exports only
      [module-a].ts  — implementation
      [module-a].test.ts
    [module-b]/
      ...
  package.json
```

Every new file created in this domain MUST fit this structure. Files that do not fit signal a structural decision that requires a document update before proceeding.

---

#### Core Interfaces

Define the TypeScript interfaces and types that form the public contract of this domain. These are constraints, not suggestions. Agent-written code MUST conform to these signatures:

```typescript
// [Brief description of what this interface represents]
interface [Name] {
  [field]: [type]
}
```

If an interface changes, this section MUST be updated before any code changes are made.

---

#### Implementation Patterns

For each major operation in this domain, provide a complete, minimal, correct implementation the agent can copy. Each pattern MUST follow the constraint block format:

```
RULE:     [What this pattern requires.]
CORRECT:  [Complete minimal implementation in TypeScript.]
INCORRECT: [The common wrong approach.]
```

Patterns in this section are authoritative. An agent that encounters an undocumented case MUST copy the nearest pattern and adapt it — not invent a new approach.

---

#### Correctness Checklist

A binary-verifiable checklist the agent runs after completing any work in this domain. Every item MUST be checkable by reading code or running a command — no subjective items:

```
Before committing work in [domain]:
- [ ] [Specific verifiable condition]
- [ ] [Specific verifiable condition]
- [ ] All new code covered by at least one test that would fail if the behavior were absent
- [ ] No package used that is not in the Package Inventory
- [ ] Module structure matches the layout defined above
```

---

#### Antipattern Checklist

A list of forbidden implementations specific to this domain. Each entry names the pattern, states why it is forbidden, and names the correct alternative:

```
FORBIDDEN: [Pattern name]
Reason:    [Why this is wrong in this domain.]
Instead:   [What to do.]
```

Minimum five entries. Entries MUST be specific to this domain — general Calypso antipatterns belong in the blueprint, not here.

---

### Implementation Document Rules

- Implementation documents MUST be kept current. When a technology decision changes, the document MUST be updated before the codebase is changed.
- Implementation documents MUST NOT duplicate content from their blueprint. If a principle is documented in the blueprint, the implementation document references it rather than restating it.
- Code examples MUST compile. Pseudocode is not acceptable in implementation documents — agents will copy it verbatim.
- The implementation document is the last word on any question about how to write code in its domain. If a question is not answered here, the answer is: follow the blueprint principles, apply the nearest pattern in this document, and add the missing rule to this document before the next session.

---

## Part 4: Development Documents

### Purpose

A development document governs a recurring process. Where blueprints answer "why" and implementation documents answer "what to build," development documents answer "how to execute a workflow to a deterministic correct outcome."

Development documents are optimized for one-shot agent execution, not for human readability. They are step sequences, decision trees, and checklists — not explanatory prose. An agent that follows a development document from top to bottom MUST arrive at the correct outcome without human intervention.

Each development document covers one workflow. The canonical workflows are:

1. **Product requirements collection** — extracting and formalizing what to build
2. **New feature development** — taking a requirement from plan to merged code
3. **Hardening** — improving correctness, security, and resilience of existing code
4. **Git commit** — producing a correctly structured, passing commit
5. **Deployment** — releasing a verified build to the target environment

---

### Required Sections

Every development document MUST contain the following sections.

---

#### Preconditions

State exactly what must be true before this workflow begins. The agent MUST verify each precondition before proceeding. If a precondition is not met, the document MUST state what to do:

```
PRECONDITIONS:
- [ ] [Condition that must be true]
- [ ] [Condition that must be true]

If any precondition is not met: [exact instruction — do not proceed / run [other workflow] first / etc.]
```

---

#### Steps

Number every step. Steps are imperative commands. No prose explanation inside a step — if context is needed, place it in a RATIONALE line immediately after the step, indented:

```
1. [Do this exact thing.]
   RATIONALE: [One sentence if the step is non-obvious.]

2. IF [condition]:
     [Do this.]
   ELSE:
     [Do this.]

3. [Do this exact thing.]
```

Decision points MUST be explicit IF/ELSE branches, not left to agent judgment. A development document with steps like "make appropriate changes" has failed.

---

#### Output Specification

State what must exist or be true when the workflow is complete. This is the acceptance condition:

```
OUTPUTS:
- [Artifact that must exist, with its location]
- [State that must be true, verifiable by [command or inspection]]
```

The agent MUST verify every output before declaring the workflow complete.

---

#### Failure Handling

For each step that can fail, state what to do:

```
IF step [N] fails:
  1. [Diagnosis step]
  2. [Remediation step]
  3. IF unresolved: [escalation — stop, ask human, revert to state X]
```

NEVER leave failure handling implicit. An agent that hits an unhandled failure state will improvise.

---

### Workflow Specifications

The sections below define the step structure for each canonical workflow. These are the authoritative patterns; individual development documents in `agent-context/development/` MUST conform to them.

---

#### Workflow: Product Requirements Collection

**Goal:** Produce a `docs/prd.md` containing testable acceptance criteria signed off by the Product Owner.

**Preconditions:**
- [ ] No `docs/prd.md` exists, or the human has explicitly requested a requirements revision
- [ ] The human is available to answer interview questions

**Steps:**
1. Read `agent-context/development/product-owner-interview.md`.
2. Generate interview questions organized by domain: user roles, data model, core workflows, integrations, constraints, non-functional requirements.
3. Present all questions to the human in a single message. Do not proceed until all are answered.
4. Synthesize answers into `docs/prd.md` with the following structure:
   - Problem statement
   - User roles and their permissions
   - Core workflows with acceptance criteria written as: "Given [state], when [action], then [outcome]"
   - Data entities and their relationships
   - Integration requirements
   - Constraints (performance, compliance, deployment)
   - Out-of-scope items (explicit list of what will not be built)
5. Present `docs/prd.md` to the human for approval.
6. IF the human requests changes: incorporate them, return to step 5.
7. IF the human approves: commit `docs/prd.md` with message `docs: product requirements document`.

**Output:** `docs/prd.md` exists, is approved, and is committed.

---

#### Workflow: New Feature Development

**Goal:** Implement a feature from the implementation plan to a passing, reviewed, merged state.

**Preconditions:**
- [ ] `docs/prd.md` exists and is approved
- [ ] `docs/plans/implementation-plan.md` exists with the feature as a listed task
- [ ] `docs/plans/next-prompt.md` identifies this feature as the next task
- [ ] All tests pass on the current branch

**Steps:**
1. Read `docs/plans/next-prompt.md`. Confirm the task.
2. Read the relevant blueprint and implementation document for the feature domain.
3. Write a minimal plan in a code comment or scratch note: what files change, what interfaces are affected.
4. Implement the feature. Follow the implementation document's patterns exactly.
5. Write tests before or during implementation — NEVER after. Tests MUST fail before the feature is complete and pass after.
6. Run the full test suite. ALL tests MUST pass before proceeding.
7. Run the correctness checklist from the relevant implementation document.
8. Update `docs/plans/implementation-plan.md`: mark this task complete, add any discovered tasks.
9. Write `docs/plans/next-prompt.md` with the next task from the plan.
10. Execute the Git Commit workflow (see below).

**Output:** Feature committed, plan updated, next-prompt written, all tests passing.

---

#### Workflow: Hardening

**Goal:** Improve correctness, security, or resilience of existing code without introducing regressions.

**Preconditions:**
- [ ] All tests pass on the current branch
- [ ] `agent-context/development/hardening.md` has been read

**Steps:**
1. Read `agent-context/development/hardening.md` in full.
2. Select one hardening target from the hardening document's priority list, or from the explicit instruction in `docs/plans/next-prompt.md`.
3. Scope the change to a single concern. FORBIDDEN: combining a hardening change with a feature change in the same commit.
4. Make the change. If the change requires a new test, write the test first.
5. Run the full test suite. ALL tests MUST pass.
6. Run the correctness checklist from the relevant implementation document.
7. Run the antipattern checklist from the relevant implementation document.
8. Update `docs/plans/implementation-plan.md` if this hardening task was listed.
9. Write `docs/plans/next-prompt.md`.
10. Execute the Git Commit workflow.

**Output:** Single hardening change committed, tests passing, no regressions.

---

#### Workflow: Git Commit

**Goal:** Produce a correctly structured, signed, passing commit.

**Preconditions:**
- [ ] All tests pass
- [ ] No unintended files are staged (`git status` reviewed)
- [ ] `docs/plans/implementation-plan.md` updated if this commit advances a planned task
- [ ] `docs/plans/next-prompt.md` written

**Steps:**
1. Run `git status`. Review every changed file. MUST NOT stage: `.env` files, generated secrets, files outside the scope of this task.
2. Stage files explicitly by name. FORBIDDEN: `git add .` or `git add -A` without prior review of `git status`.
3. Run the full test suite one final time. If any test fails: fix it before committing.
4. Write the commit message. Format:
   ```
   [type]: [imperative summary under 72 characters]

   [Optional body: what changed and why, not how. Wrap at 72 characters.]
   ```
   Valid types: `feat`, `fix`, `refactor`, `test`, `docs`, `chore`, `security`.
5. Commit: `git commit -m "[message]"`.
6. IF pre-commit hook fails:
   - Read the hook output.
   - Fix the issue.
   - Return to step 3.
   - NEVER use `--no-verify`.

**Output:** One commit on the current branch, hook passing, message correctly formatted.

---

#### Workflow: Deployment

**Goal:** Release a verified build to the target environment with zero downtime and a tested rollback path.

**Preconditions:**
- [ ] All tests pass on the branch being deployed
- [ ] The branch has been reviewed and merged to the deployment target (typically `main`)
- [ ] The deployment environment is reachable and healthy
- [ ] Read `agent-context/blueprints/deployment-blueprint.md` and `agent-context/implementation-ts/deployment-implementation.md`

**Steps:**
1. Confirm the exact image tag or commit SHA being deployed.
2. Run the pre-deployment checklist from `agent-context/implementation-ts/deployment-implementation.md`.
3. Apply the deployment manifest: `kubectl apply -f [manifest]` or equivalent for the current environment.
4. Monitor rollout: `kubectl rollout status deployment/[name]`. Wait for completion.
5. IF rollout fails or pods do not become Ready within the timeout:
   - Capture logs: `kubectl logs -l app=[name] --previous`
   - Execute rollback: `kubectl rollout undo deployment/[name]`
   - Report failure with logs. Do NOT attempt a second deployment without human review.
6. Run smoke tests against the deployed environment.
7. IF smoke tests fail:
   - Execute rollback immediately.
   - Report failure with test output.
8. Record the deployment in `docs/deployments/[date]-[environment].md` with: image tag, deployer (agent session), time, smoke test results.

**Output:** Deployment confirmed healthy, smoke tests passing, deployment record committed.

---

### Development Document Rules

- Steps MUST be imperative commands. No step may read "consider", "evaluate", or "as appropriate."
- Every decision point MUST have an explicit ELSE branch.
- Failure handling MUST be specified for every step that can fail non-trivially.
- Development documents are updated when a workflow changes. An outdated development document is more dangerous than no document — it produces confident incorrect execution.
- Cross-references to implementation documents are REQUIRED wherever a workflow involves domain-specific patterns (e.g., "run the correctness checklist from the auth implementation document").
