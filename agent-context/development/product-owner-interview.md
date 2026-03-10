# Product Owner Interview Template

<!-- last-edited: 2026-03-10 -->

CONTEXT MAP
  this ──extends──────────▶ agent-communication.md §Workflow: Product Requirements Collection (steps 1–7)
  this ◀──referenced by──── init/scaffold-task.md §Step 10

> **Scope:** This document is the interview template supplement for the Product Requirements Collection workflow defined in `agent-communication.md §Workflow: Product Requirements Collection`. Follow the steps defined there; use this document for the questionnaire content at step 2. Do NOT duplicate those workflow steps here.

---

## Preconditions

```
PRECONDITIONS:
- [ ] The human Product Owner is available and has provided a high-level description of the application
- [ ] No `docs/prd.md` exists, or the human has explicitly requested a requirements revision
- [ ] Agent has read `agent-communication.md §Workflow: Product Requirements Collection` before this document

If no high-level description has been provided: ask the Product Owner for a 2–3 sentence description of the application before generating questions.
```

---

## Steps

The full Product Requirements Collection workflow is defined in `agent-communication.md §Workflow: Product Requirements Collection`. The steps specific to conducting the interview (step 2 of that workflow) are:

1. Read the Product Owner's high-level description.
2. Generate structured questions using the **Questionnaire Categories** below.
3. **Pre-fill your best inferred answers** for every question based on the description. Do not leave blank text boxes.
4. If you cannot confidently infer an answer, provide multiple-choice options (including "Other — please specify").
5. Present the pre-filled questionnaire to the Product Owner in a single message. Do not ask one question at a time.
6. Wait for the Product Owner to confirm, correct, or expand on the pre-filled answers.
7. Synthesize confirmed answers into `docs/prd.md` per `agent-communication.md §Workflow: Product Requirements Collection` step 4.

---

## Output Specification

```
OUTPUTS:
- Pre-filled questionnaire presented to the Product Owner in a single message
- `docs/prd.md` written after Product Owner confirms answers (see agent-communication.md for format)
- External API test credentials collected and stored in `.env.test` (not committed to version control)
```

---

## Failure Handling

```
IF the Product Owner's description is too vague to infer answers:
  1. Ask for clarification on the core user goal only — one question.
  2. Do NOT proceed to the full questionnaire until the core goal is clear.

IF the Product Owner rejects the pre-filled answers extensively:
  1. Acknowledge the corrections, update the pre-filled answers in place.
  2. Re-present the revised questionnaire for final confirmation.
  3. Do NOT write docs/prd.md until the Product Owner has confirmed.

IF external API credentials are not provided at interview time:
  1. Note the missing credentials in docs/prd.md under "Constraints".
  2. Write a task in docs/plans/implementation-plan.md: "Collect test credentials for [service]".
  3. Continue — do not block PRD creation on missing credentials.
```

---

## Questionnaire Categories

Generate and pre-fill high-impact questions under each of the following critical categories. Present as a single markdown-formatted questionnaire addressed to the Product Owner. Pre-fill every answer based on the Product Owner's description; use multiple-choice options where you cannot confidently infer. Focus on product features, user stories, user roles, and workflows only — do not ask technical architecture questions.

### 1. Product Vision & Value Proposition
* What is the core problem this application solves for the user?
* How does the user currently solve this problem, and how is this application better?
* What does a successful outcome look like for the primary user?

### 2. Core Workflows & User Stories
* Describe the "Happy Path" workflow from the moment a user signs up to the moment they achieve their primary goal.
* What are the most common edge cases or alternative workflows a user might take?
* Are there complex state machines for entities (e.g., an order moving from Draft -> Paid -> Shipped -> Delivered)? 
* *Agent instruction: Formulate specific questions to extract exactly what entities exist and how they interact, and pre-fill them.*

### 3. User Roles, Permissions, and Access
* What distinct types/roles of users exist in the system (e.g., Administrator, Free User, Premium Customer)?
* What specific features and data can each role access? What are they restricted from seeing or doing?
* Does authorization depend on complex conditions (e.g., "A manager can only approve requests from their own department")?

### 4. External Integrations (Business Context)
* What external services (e.g., Payment Gateways like Stripe, CRMs like Salesforce, Email Providers like SendGrid) must the system integrate with to support the user workflow?
* What specific business actions trigger a call to these external services (e.g., "When a user upgrades their plan, charge their card")?

### 5. Test Credentials and Setup
* *Note: All external API interactions are considered critical to test. The AI must execute real network requests.*
* Please provide the necessary Sandbox/Test API keys and connection credentials for all external services so I can build the automated fixture generator. (Ensure these test credentials will not cause destructive side-effects in your production environment).

**Output Format:**
Output the pre-filled questionnaire as a single markdown document addressed to the Product Owner. Use blockquotes or clear formatting to show your inferred answers or your multiple-choice lists, asking them to simply edit, confirm, or select the correct options.
