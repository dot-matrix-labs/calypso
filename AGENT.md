# Calypso

<!-- last-edited: 2026-03-10 -->

You are an autonomous agent. Complete the assigned task in a single pass with minimal human intervention. Follow the curriculum below in order. Load only what the current phase requires.

---

## Phase 1: Orient

1. Read `agent-context/index.md`. This is the full document graph and keyword index.
2. Check the GitHub issues tracker for the current task. The Plan tracking issue lists work in batch order. Pick the next unstarted issue in the earliest open batch.
3. IF no task is assigned: ask the human what to build. This is the ONE acceptable reason to ask.

---

## Phase 2: Select a Workflow

Based on the task, pick exactly one development workflow from `agent-context/development/`:

| Task type | Workflow document |
|---|---|
| New feature or module | `development/development-standards.md` |
| Hardening / security / resilience | `development/hardening.md` |
| Writing documentation | `development/documentation-standard.md` |
| Requirements gathering | `development/product-owner-interview.md` |
| Project scaffold from zero | `init/scaffold-task.md` |

Read the selected workflow document. Follow it as your primary instruction set.

---

## Phase 3: Load Implementation Context

1. Read the implementation document for the domain you are working in (see the Task Routing table in `agent-context/index.md`).
2. The implementation document contains: stack spec, package inventory, module structure, interfaces, patterns, and checklists.
3. This is sufficient to write correct code. **Stop here and begin work.**

---

## Phase 4: Deepen Context (only when needed)

If at any point during implementation you encounter uncertainty — a design decision you cannot resolve from the implementation document alone — do NOT ask the human. Instead, escalate your context:

```
CONFIDENCE CHECK
  Can I resolve this from the implementation document?
    YES → continue working.
    NO  → proceed to step 1 below.

1. Read the keyword index in agent-context/index.md.
2. Identify the blueprint(s) whose keywords match your uncertainty.
3. Read the relevant blueprint section (not the full document — use the
   Context Map and section headers to target the specific concern).
4. Apply what you learned. Return to implementation.

Still uncertain after reading the blueprint?
5. Read agent-communication.md §Document Precedence Rules to check
   whether a newer document supersedes what you found.
6. Search the codebase for analogous existing implementations.
7. Choose the simplest solution consistent with the blueprint principles.

Still blocked?
8. Only now: ask the human. State what you tried, what you found, and
   what specific decision you need made.
```

This is a **context escalation loop**, not a one-time decision. Each pass through the loop adds more context. Most tasks complete at Phase 3. Blueprints are reference material, not required reading.

---

## Commit Standards

Read `agent-context/development/git-standards.md` before your first commit. Key rules:

- Conventional commit format: `type: imperative summary`
- Valid types: `feat`, `fix`, `refactor`, `test`, `docs`, `chore`, `security`
- Stage files explicitly by name. NEVER `git add .`
- NEVER use `--no-verify`
- All tests MUST pass before committing

---

## Rules

- **Autonomy first.** Do not ask the human for help unless you have exhausted the context escalation loop above.
- **Minimal context loading.** Do not read documents speculatively. Load what you need for the current phase, then work.
- **Implementation docs before blueprints.** Blueprints explain why. Implementation docs tell you what to build. Start with what.
- **One workflow per session.** Pick one workflow document and follow it to completion. Do not mix workflows.
- **Follow patterns exactly.** When an implementation document provides a code pattern, copy it. Do not invent alternatives.
- **Update docs you contradict.** If your implementation necessarily deviates from a documented pattern, update the document before committing. Stale docs are worse than no docs.
