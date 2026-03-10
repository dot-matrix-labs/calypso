# Process Blueprint

<!-- last-edited: 2026-03-10 -->

CONTEXT MAP
  this ◀──implemented by── implementation-ts/process-implementation.md
  this ◀──referenced by──── development/development-standards.md
  this ◀──referenced by──── index.md

> [!IMPORTANT]
> This blueprint defines the development process for AI-agent-built software: how work is planned, how progress is tracked, and how an agent advances through a product lifecycle without continuous human prompting.

---

## Vision

Software development is a state machine. Each unit of work transforms the project from one known state to a known next state. In human-driven development, the state machine runs on tribal knowledge, standup meetings, and ticket boards — none of which an AI agent can attend. When an agent starts a session, it has no memory of what happened yesterday, no sense of what is blocked, and no intuition about what matters most. Without an explicit, machine-readable process, the agent either waits for a human to tell it what to do next (defeating the purpose of autonomous development) or guesses (producing work that may be irrelevant, redundant, or out of order).

A correct process for agent-driven development makes the state machine explicit. The product requirements describe *what* to build. The implementation plan describes *how* to build it and tracks completion. The next-prompt file tells the agent exactly what to do when it wakes up. Together, these three documents form a closed loop: every commit advances the plan and writes the instructions for the next commit. The agent becomes self-advancing — a human can walk away for hours and return to find meaningful, ordered progress.

**Scope Note:** This blueprint applies exclusively to the agent's *engineering and development roles* (e.g., writing code, planning features, scaffolding projects). It does not govern any administrative, operational, or in-app roles the agent may hold on behalf of end-users within the deployed software. Those roles are governed by the respective functional blueprints including Auth, Data, and UX.

The cost of ignoring this blueprint is an agent that produces impressive-looking code in random order, skipping foundational work to build visible features, leaving gaps that compound until the project requires a human to manually re-plan and re-prioritize. The process is not overhead — it is the mechanism that converts an agent from a sophisticated autocomplete into a reliable development partner.

---

## Threat Model

| Scenario | What must be protected |
|---|---|
| Agent starts a session with no context about prior work | Continuity — the agent must resume exactly where the last session left off |
| Agent builds features before foundational infrastructure exists | Build order — scaffolding, CI, and test stubs must precede feature work |
| Product requirements change mid-development | Adaptability — the plan must accommodate changes without losing track of completed work |
| Agent works on low-priority tasks while critical work is blocked | Prioritization — the implementation plan must encode priority and ordering |
| Multiple agents work on the same project with conflicting plans | Single source of truth — one plan file, one next-prompt, no parallel plans |
| Agent completes work but does not update the plan | Plan accuracy — the plan must reflect reality at every commit |
| Human overrides the agent's next task | Human authority — the next-prompt file is human-editable and the agent respects overrides |
| Agent session crashes mid-task | Recoverability — git commits are the unit of durable progress; uncommitted work is forfeit |
| Requirements are ambiguous or incomplete | Requirement quality — the PRD interview must extract concrete, testable requirements |

---

## Core Principles

### The commit is the unit of progress

Every meaningful state change is captured in a git commit. Between commits, work is speculative and lossy. The agent's goal is to reach the next committable state as quickly as possible — not to accumulate a large batch of changes. Small, frequent commits create a fine-grained history that is easy to review, easy to revert, and easy to resume from.

### Plans are living documents, not initial artifacts

An implementation plan written once and never updated is a fiction within days. The plan is updated at every commit — new tasks are discovered and added, completed tasks are checked off, ordering is adjusted based on what the agent learned during implementation. The plan is the agent's working memory across sessions.

### The next action is always explicit

An agent should never need to decide "what should I do now?" by analyzing the entire codebase and plan from scratch. The next-prompt file contains a single, self-contained instruction for the very next action. It is written by the agent at the end of each commit, creating a self-advancing loop. A human can override it at any time by editing the file.

### Requirements are extracted, not assumed

The agent does not guess what the product should do. It generates structured interview questions for the Product Owner, collects answers, and writes a canonical Product Requirements Document. The PRD is owned by the human; the implementation plan is derived from it by the agent. This separation ensures the human controls *what* and the agent controls *how*.

### Infrastructure enforces sequencing

An agent cannot begin feature work while foundational infrastructure is incomplete. Repository, CI, test stubs, and deployment must be operational before the first feature commit. A checklist of concrete, verifiable conditions governs this — the conditions are not bureaucracy; they prevent the agent from building a beautiful facade on a foundation that does not exist.

---

## Design Patterns

### Pattern 1: Three-Document Planning Loop

**Problem:** An agent needs to know what the product should do, what work remains, and what to do right now. These are three different questions with different owners and different update frequencies.

**Solution:** Maintain three documents with distinct scopes:

- **Product Requirements Document** — what the product must do. Owned by the human. Updated when requirements change.
- **Implementation Plan** — all tasks, ordered, with completion state. Owned by the agent. Updated at every commit.
- **Next Prompt** — the single next action. Owned by the agent. Written at the end of each commit, read at the start of the next.

The three documents form a hierarchy: the PRD constrains the plan, and the plan generates the next prompt. Information flows down; overrides flow up (a human editing next-prompt overrides the agent's planned sequence).

**Trade-offs:** Three files to maintain adds overhead to every commit. But the alternative — a single plan file that serves all three purposes — becomes unwieldy and ambiguous. The overhead is seconds per commit; the clarity is worth hours of avoided confusion.

### Pattern 2: Self-Advancing State Machine

**Problem:** Agent sessions are discontinuous. Each session starts cold, with no memory of the previous session. Without explicit continuation, the agent must re-derive the project state from scratch — which is slow, error-prone, and may produce different conclusions each time.

**Solution:** Each commit writes the next-prompt file as its final action. The next session reads this file as its first action. The result is a chain: commit N writes the instructions for commit N+1, which writes the instructions for commit N+2. The agent can execute multiple commits in a single session without waiting for human input between them. A human can break the chain at any time by editing the next-prompt file.

**Trade-offs:** If the agent writes a poor next-prompt (too vague, wrong priority), the next session starts on the wrong foot. Mitigated by the implementation plan, which provides broader context. If both are wrong, the human intervenes by editing one or both.

### Pattern 3: Structured Requirements Interview

**Problem:** Product requirements communicated informally (chat messages, verbal descriptions, vague feature requests) produce vague, incomplete, and contradictory specifications. The agent builds what it thinks was meant, not what was actually needed.

**Solution:** The agent generates a structured interview document with concrete questions organized by domain: user roles, data model, workflows, integrations, constraints. The Product Owner answers in writing. The agent synthesizes the answers into a canonical PRD with testable acceptance criteria. The PRD is the contract — if it is not in the PRD, the agent does not build it.

**Trade-offs:** The interview process takes time upfront. Product Owners may resist the formality. But the alternative — building from informal requirements and iterating on misunderstandings — costs far more in rework.

### Pattern 4: Infrastructure Before Features

**Problem:** Agents optimize for visible output. Given a feature list, an agent will build the most impressive-looking features first, leaving infrastructure, testing, and error handling for "later." Later arrives and the codebase is a demo with no foundation.

**Solution:** Before implementing any feature, the repository, CI pipeline, test stubs, and deployment infrastructure must be operational. The rule is simple: if the scaffold is not complete, no feature work begins. The scaffold is not a phase — it is a prerequisite that must be satisfied immediately and maintained throughout development.

**Trade-offs:** This discipline requires resisting the pull toward visible progress. An agent that has not shipped a feature may appear unproductive. The alternative — building features on a broken foundation — produces work that cannot be tested, deployed, or extended.

---

## Plausible Architectures

### Architecture A: Solo Agent Loop (one agent, one project)

```
┌──────────────────────────────────────────────────┐
│                                                  │
│  Human writes/updates PRD                        │
│       │                                          │
│       ▼                                          │
│  Agent reads PRD + plan + next-prompt            │
│       │                                          │
│       ▼                                          │
│  Agent executes next task                        │
│       │                                          │
│       ▼                                          │
│  Agent commits:                                  │
│    1. Code changes                               │
│    2. Updated implementation plan                │
│    3. New next-prompt                            │
│       │                                          │
│       ▼                                          │
│  Loop back to "read" (same session)              │
│  ── or ──                                        │
│  Session ends; next session reads next-prompt    │
│                                                  │
└──────────────────────────────────────────────────┘
```

**When appropriate:** Single agent working on a project. Most common case for early-stage development. Human checks in periodically to review commits and adjust the PRD or next-prompt.

**Trade-offs:** No parallelism. One agent, one task at a time. Simple and predictable.

### Architecture B: Multi-Agent with Shared Plan (parallel agents, divided work)

```
┌────────────────────────────────────────────────────────┐
│                                                        │
│  Human writes PRD                                      │
│       │                                                │
│       ▼                                                │
│  Lead agent creates implementation plan                │
│  Partitions tasks by area (frontend / backend / tests) │
│       │                                                │
│       ▼                                                │
│  ┌─────────────┐  ┌─────────────┐  ┌───────────────┐  │
│  │  Agent A     │  │  Agent B     │  │  Agent C      │  │
│  │  (frontend)  │  │  (backend)   │  │  (tests)      │  │
│  │  Own branch  │  │  Own branch  │  │  Own branch   │  │
│  │  Own next-   │  │  Own next-   │  │  Own next-    │  │
│  │  prompt      │  │  prompt      │  │  prompt       │  │
│  └──────┬──────┘  └──────┬──────┘  └───────┬───────┘  │
│         └────────────────┼──────────────────┘          │
│                          ▼                             │
│  Shared implementation plan (merge via PRs)            │
│                                                        │
└────────────────────────────────────────────────────────┘
```

**When appropriate:** Larger projects where work can be cleanly partitioned by domain. Each agent works on its own branch with its own next-prompt. The implementation plan is shared and updated via pull requests.

**Trade-offs:** Coordination overhead. Agents may produce conflicting changes to shared types or APIs. Requires a merge strategy and possibly a lead agent that resolves conflicts. More throughput but more complexity.

### Architecture C: Human-in-the-Loop Gated (regulated or high-stakes)

```
┌────────────────────────────────────────────────────────┐
│                                                        │
│  Agent executes one task → commits → writes next-prompt│
│       │                                                │
│       ▼                                                │
│  GATE: Human reviews commit                            │
│       │                                                │
│       ├── Approved → Agent reads next-prompt, continues│
│       │                                                │
│       └── Rejected → Human edits next-prompt with      │
│                      corrections, agent re-executes    │
│                                                        │
└────────────────────────────────────────────────────────┘
```

**When appropriate:** High-stakes domains (finance, healthcare, compliance) where every commit must be reviewed before the agent proceeds. Slower but safer. The human is a gate, not a driver — the agent still proposes the work and the plan.

**Trade-offs:** Throughput is limited by human review speed. The agent may idle waiting for approval. Acceptable when correctness matters more than velocity.

---

## Reference Implementation — Calypso TypeScript

> The following is the Calypso TypeScript reference implementation. The principles and patterns above apply equally to other stacks; this section illustrates one concrete realization in the Calypso monorepo.

See [`agent-context/implementation-ts/process-implementation.md`](../implementation-ts/process-implementation.md) for the full stack specification: planning document locations, implementation plan format, next-prompt format, pre-commit hook enforcement, and scaffold checklist.

---

## Implementation Checklist

- [ ] `docs/prd.md` exists and contains testable acceptance criteria from a structured interview
- [ ] `docs/plans/implementation-plan.md` exists and has been updated within the last commit
- [ ] `docs/plans/next-prompt.md` exists and contains a valid, self-contained next action
- [ ] Pre-commit hook rejects commits that do not include plan and next-prompt updates
- [ ] All scaffold tasks completed: repo, CI, test stubs verified before any feature work
- [ ] At least one full loop demonstrated: commit → plan update → next-prompt → next commit resumes from prompt
- [ ] Human has reviewed and approved the PRD
- [ ] Agent has not built any features ahead of scaffold completion
- [ ] Implementation plan accurately reflects all completed and remaining work (audited by human)
- [ ] Next-prompt chain has been unbroken for at least 10 consecutive commits
- [ ] Human override of next-prompt tested and agent respected the override
- [ ] Multiple sessions demonstrated: agent resumes from next-prompt after session boundary
- [ ] Plan includes discovered tasks (tasks added during implementation, not just initial planning)
- [ ] Process documentation in `docs/` reflects the actual process used (not aspirational)
- [ ] Recovery procedure tested: agent resumes correctly after a crashed session with uncommitted work
- [ ] Human can onboard a new agent to the project using only the three planning documents

---

## Antipatterns

- **The phantom plan.** Writing an implementation plan at the start of the project and never updating it. Within days the plan diverges from reality. The agent ignores it and makes its own decisions. The plan becomes a historical artifact that misleads anyone who reads it.

- **Feature-first development.** Jumping to visible features (UI, integrations) before scaffold infrastructure is complete. The result is a demo that cannot be tested, deployed, or extended. Fixing the foundation after building the house is always more expensive than building it first.

- **Session amnesia.** Starting each agent session by asking "what should I work on?" instead of reading the next-prompt file. The agent re-derives the project state from the codebase, arrives at a different conclusion than the previous session, and produces work that conflicts with or duplicates prior efforts.

- **Monolithic commits.** Accumulating hours of work into a single massive commit. If the session crashes before the commit, all work is lost. If the commit introduces a bug, the revert is catastrophic. Small commits are cheaper in every dimension.

- **Verbal requirements.** Accepting feature requests from chat messages, verbal conversations, or vague tickets without formalizing them into the PRD. The agent builds what it understood, which is never exactly what was meant. The PRD is the contract — if it is not written there, it does not get built.

- **Plan as wishlist.** Writing implementation plan tasks as vague goals ("improve performance", "make it look better") instead of concrete, verifiable actions ("add database index on users.email", "reduce landing page bundle to under 200KB"). Vague tasks produce vague work.

- **Skipping the interview.** Assuming the agent already knows what the product should do based on its training data or a brief description. Every product has domain-specific requirements that cannot be inferred. The structured interview exists to surface them before development begins, not after.
