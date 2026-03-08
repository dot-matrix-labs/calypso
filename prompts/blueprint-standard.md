# Blueprint Standard

> [!NOTE]
> This document is for **blueprint authors and editors** of the Calypso prompts repository. It is not a blueprint itself and is not distributed to end users. It defines what a Calypso blueprint is, what it must contain, and how to write one correctly.

---

## What Is a Blueprint?

A Calypso blueprint is a conceptual document — a PRD for a technical domain (e.g., data security, observability, authentication, multi-tenancy). It begins at the problem level, works through design patterns and architectures, and ends with a reference implementation in Calypso's standard TypeScript stack.

A blueprint is not:
- A tutorial for a specific library
- Documentation for a specific project
- A checklist for a specific deployment

A blueprint is:
- Reusable across organizations and tech stacks
- Opinionated about principles, not about vendors
- Loosely coupled to the Calypso TS reference implementation, which is one concrete example — not the only valid one

**The test:** Could someone use this blueprint as the foundation for a Python/Django/Postgres deployment, or a Go/Postgres/Kubernetes deployment, without rewriting the conceptual sections? If yes, the blueprint is well-structured. If the concepts and the implementation are entangled, the blueprint is broken.

---

## Required Sections

Every blueprint must contain the following sections in this order. Sections may be expanded; they may not be removed or reordered.

---

### 1. Vision

**Purpose:** Establish the *why*. What problem does this blueprint address? What does the world look like when this blueprint is correctly applied? What failure modes does it prevent?

**Rules:**
- Two to five paragraphs. Not a bullet list.
- Technology-agnostic. No library names, no specific products.
- Written in declarative present tense: "Every application defaults to…", not "You should try to…"
- Must include at least one concrete consequence of *not* applying the blueprint (the cost of ignoring it)

**Antipatterns:**
- Vision that is just a restatement of the section title ("This section covers data security")
- Vision that lists implementation choices ("We will use AES-256 and Vault")
- Vision written as aspirational marketing copy with no technical grounding

---

### 2. Threat Model / Problem Space

**Purpose:** Define the adversaries, failure modes, or problem conditions the blueprint addresses. Every design choice made later in the document must trace back to a row in this section.

**Rules:**
- Use a table with at least two columns: the scenario/adversary and what must be protected or preserved under that condition
- Be concrete and specific: "disk image exfiltrated" not "data breach"
- Minimum five rows; no maximum
- A design choice with no corresponding row in this table is decoration and does not belong in the blueprint

**For security blueprints:** threat actors and attack scenarios.
**For operational blueprints (observability, deployment):** failure modes and degraded conditions.
**For process blueprints (testing, CI):** quality failure modes and their downstream consequences.

**Antipatterns:**
- Threat model that lists only obvious, generic risks with no specificity
- Threat model that is never referenced again in the document
- Skipping this section for "non-security" blueprints — every domain has a problem space

---

### 3. Core Principles

**Purpose:** State the non-negotiable architectural principles that govern every decision in this domain. These are the axioms from which the design patterns are derived.

**Rules:**
- Three to seven principles, no more
- Each principle is a short, declarative sentence followed by one paragraph of rationale
- Principles must be falsifiable: "encrypt all data" is not a principle, "a compromise of any single layer must not yield plaintext at any other layer" is
- Principles must be tech-stack-agnostic

**Antipatterns:**
- Principles that are really implementation steps ("Use AES-256-GCM for field encryption")
- Principles that contradict each other without acknowledging the trade-off
- More than seven principles (scope creep; the real principles are being diluted by preferences)

---

### 4. Design Patterns

**Purpose:** Document the recurring solutions to recurring problems in this domain. Each pattern is a named, reusable approach — not a one-off implementation detail.

**Rules for each pattern:**
- Name it. A pattern without a name cannot be referenced, discussed, or violated with precision.
- State the problem it solves in one sentence.
- Describe the solution in language that is implementation-agnostic.
- Show a concrete example in pseudo-code or generic notation if the pattern has a non-obvious shape.
- State the trade-offs explicitly: when does this pattern become the wrong choice?

**Rules for the section:**
- Minimum three patterns; no maximum
- Patterns are ordered from most fundamental to most specialized
- Each pattern must map to at least one row in the Threat Model / Problem Space section
- Patterns that only apply to the Calypso TS stack belong in the Reference Implementation section, not here

**Antipatterns:**
- Patterns that are just library API walkthroughs ("call `kms.encrypt()` with the key ID")
- Patterns with no named trade-off
- Implementation-specific code in this section (TypeScript, SQL, YAML belong in the Reference Implementation)

---

### 5. Plausible Architectures

**Purpose:** Show two to four concrete system designs that apply the principles and patterns correctly, at different scales or under different constraints. These are starting points, not prescriptions.

**Rules:**
- Each architecture has a name and a brief characterization of when it is appropriate (scale, team size, deployment model, regulatory context)
- Use ASCII block diagrams for system topology — not prose descriptions
- Explicitly note what each architecture trades off against the others
- Architectures must be implementation-agnostic in their labels (use generic component names: "Key Store", "Event Bus", "Aggregation Worker" — not "Vault", "Kafka", "Lambda")
- A section at the bottom of each architecture may optionally list concrete product options for each component — but the architecture itself must not depend on those choices

**Antipatterns:**
- Only one architecture (blueprints are about design space, not single answers)
- Architectures that are identical except for scale (missing a genuine trade-off)
- Architectures drawn around specific vendor products rather than component roles

---

### 6. Reference Implementation — Calypso TypeScript

**Purpose:** Translate the principles, patterns, and architectures into a concrete implementation using the Calypso standard stack: TypeScript, Bun, React, SQLite/PostgreSQL.

**Rules:**
- This section is explicitly marked as one implementation of the blueprint, not the definitive implementation
- The introductory paragraph must include a statement to this effect: "The following is the Calypso TypeScript reference implementation. The principles and patterns above apply equally to other stacks; this section illustrates one concrete realization."
- Package structure: show where security/domain code lives within the Calypso monorepo layout (`/packages/`, `/apps/server/`, etc.)
- Interfaces: define the key TypeScript interfaces and types
- Buy vs DIY table: for every external dependency introduced, justify whether it is a Buy or a DIY per the Calypso dependency policy
- Code examples must be minimal and illustrative — no full module implementations, no inline tests
- Reference the Calypso dependency policy (`calypso-blueprint.md §2`) when making Buy/DIY decisions

**Antipatterns:**
- Reference implementation that is so detailed it overshadows the conceptual sections
- TypeScript-specific concepts smuggled into the Design Patterns section above
- Listing packages without Buy/DIY justification
- Copying code from real libraries without attribution

---

### 7. Implementation Checklist

**Purpose:** Provide a verifiable, staged checklist that an implementer can use to confirm correct application of the blueprint. Staged by product maturity milestones.

**Rules:**
- Three stages minimum: Alpha (functional baseline), Beta (production-hardened), V1 (fully operational)
- Each item is a concrete, binary verifiable action — not a description of a goal
  - **Correct:** `[ ] JWT expiry enforced and tested; algorithm pinned to ES256`
  - **Wrong:** `[ ] Implement secure authentication`
- Items are written in past-tense-passive or imperative: "Verified", "Configured", "Tested"
- Each item implicitly or explicitly maps to a design pattern or architecture decision in the blueprint
- The Alpha checklist must not be a subset of basic functionality — it must be the minimum gate before any real data enters the system

**Antipatterns:**
- Checklist items that cannot be verified by a third party without running the system
- Checklist items that restate principles rather than test them
- A single flat checklist with no staging (makes it impossible to know what is blocking vs. what is future work)

---

### 8. Antipatterns

**Purpose:** Name the specific wrong approaches that look plausible but violate the blueprint's principles. This section makes the blueprint defensible in code review.

**Rules:**
- Minimum five antipatterns; no maximum
- Each antipattern has a short name (bold), followed by one to three sentences explaining what it is and why it is wrong
- Antipatterns must be concrete, not generic ("Don't do security wrong" is not an antipattern)
- Antipatterns must be things that real implementers are tempted to do — if no one would ever do it, it is not worth documenting
- Each antipattern implicitly maps to a principle or pattern it violates

**Antipatterns:**
- Antipatterns that are just "don't be lazy" advice
- Antipatterns that are already covered by the Calypso blueprint or hardening standard (duplication without additive specificity)
- Antipatterns that prescribe a specific vendor or library as the wrong choice (keep it structural)

---

## Section Length Guidelines

| Section | Minimum | Maximum |
|---|---|---|
| Vision | 2 paragraphs | 5 paragraphs |
| Threat Model / Problem Space | 5 table rows | Unlimited |
| Core Principles | 3 items | 7 items |
| Design Patterns | 3 patterns | Unlimited |
| Plausible Architectures | 2 architectures | 4 architectures |
| Reference Implementation | Package structure + interfaces + buy/DIY table | No code blocks longer than 30 lines |
| Implementation Checklist | 10 items (Alpha minimum) | Unlimited |
| Antipatterns | 5 items | Unlimited |

---

## Coupling Guidelines

### The Decoupling Test

After drafting a blueprint, apply this test to sections 1–5 (Vision through Plausible Architectures):

> Remove every TypeScript, Bun, React, SQLite, and PostgreSQL reference from sections 1–5. Does the document still make complete sense?

If yes: sections 1–5 are correctly decoupled. Section 6 carries all implementation specifics.

If no: find the TypeScript-specific concepts that leaked into the conceptual sections and move them to Section 6 or generalize them into implementation-agnostic language.

### Naming Conventions for Architectures

Use role-based names, not product names, in sections 1–5:

| Role-based (correct) | Product-specific (wrong) |
|---|---|
| Key Store | HashiCorp Vault |
| Relational Database | PostgreSQL |
| Object Storage | S3 |
| Message Queue | Kafka |
| Runtime | Bun / Node |
| Container Orchestrator | Kubernetes |

Section 6 may and should use concrete product names.

### Cross-Blueprint Consistency

Blueprints reference each other. When a blueprint depends on a concept defined in another blueprint, cite it explicitly:

> "Agent authentication is defined in the Data Security Blueprint. This blueprint assumes that mechanism is in place."

Do not duplicate concepts across blueprints. If authentication appears in two blueprints, one should be canonical and the other should reference it.

---

## Review Checklist for Blueprint Editors

Before merging a new or revised blueprint:

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

## Blueprint File Template

Copy this template when starting a new blueprint. Delete this line and the section comments before publishing.

```markdown
# [Domain] Blueprint

> [!IMPORTANT]
> [One-sentence statement of the blueprint's purpose and who it is for.]

---

## Vision

[2–5 paragraphs. Tech-stack-agnostic. Declarative present tense. Include the cost of ignoring this blueprint.]

---

## Threat Model

| Scenario | What must be protected |
|---|---|
| [Scenario 1] | [What is at risk] |
| [Scenario 2] | [What is at risk] |
| [Scenario 3] | [What is at risk] |
| [Scenario 4] | [What is at risk] |
| [Scenario 5] | [What is at risk] |

---

## Core Principles

### [Principle Name]

[One sentence statement.] [One paragraph rationale.]

### [Principle Name]

[One sentence statement.] [One paragraph rationale.]

### [Principle Name]

[One sentence statement.] [One paragraph rationale.]

---

## Design Patterns

### Pattern 1: [Name]

**Problem:** [One sentence.]

**Solution:** [Implementation-agnostic description.]

**Trade-offs:** [When is this pattern wrong?]

### Pattern 2: [Name]

...

---

## Plausible Architectures

### Architecture A: [Name] ([when appropriate])

[ASCII block diagram]

**Trade-offs vs. other architectures:** [...]

### Architecture B: [Name] ([when appropriate])

[ASCII block diagram]

**Trade-offs vs. other architectures:** [...]

---

## Reference Implementation — Calypso TypeScript

> The following is the Calypso TypeScript reference implementation. The principles and patterns above apply equally to other stacks; this section illustrates one concrete realization using TypeScript, Bun, React, and PostgreSQL.

### Package Structure

\`\`\`
/packages/[domain]
  /[module-a]
  /[module-b]
\`\`\`

### Core Interfaces

\`\`\`typescript
// [Interface definitions]
\`\`\`

### Dependency Justification

| Package | Reason to Buy | Justified |
|---|---|---|
| [package] | [reason] | Yes / No — DIY |

---

## Implementation Checklist

### Alpha Gate

- [ ] [Verifiable item]
- [ ] [Verifiable item]

### Beta Gate

- [ ] [Verifiable item]
- [ ] [Verifiable item]

### V1 Gate

- [ ] [Verifiable item]
- [ ] [Verifiable item]

---

## Antipatterns

- **[Name].** [What it is and why it violates the blueprint.]
- **[Name].** [What it is and why it violates the blueprint.]
- **[Name].** [What it is and why it violates the blueprint.]
- **[Name].** [What it is and why it violates the blueprint.]
- **[Name].** [What it is and why it violates the blueprint.]
```
