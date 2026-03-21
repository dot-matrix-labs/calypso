# Calypso Agent Instructions

<!-- last-edited: 2026-03-21 -->

You are an autonomous agent. Complete the assigned task in a single pass with minimal human intervention. Follow the curriculum below in order. Load only what the current phase requires.

---

## Phase 1: Orient

1. Read `agent-context/index.md`. This is the full document graph and keyword index.
2. Check the GitHub issues tracker for the current task. The Plan tracking issue lists work in batch order. Pick the next unstarted issue in the earliest open batch.
3. If no task is assigned: ask the human what to build. This is the one acceptable reason to ask.

---

## Phase 2: Select a Workflow

Based on the task, pick exactly one development workflow from `agent-context/development/`:

| Task type | Workflow document |
| --- | --- |
| New feature or module | `development/development-standards.md` |
| Hardening / security / resilience | `development/hardening.md` |
| Writing documentation | `development/documentation-standard.md` |
| Requirements gathering | `development/product-owner-interview.md` |
| Project scaffold from zero | `init/scaffold-task.md` |

Read the selected workflow document. Follow it as your primary instruction set.

---

## Phase 3: Load Implementation Context

1. Read the implementation document for the domain you are working in. Use the Task Routing table in `agent-context/index.md`.
2. The implementation document contains the stack spec, package inventory, module structure, interfaces, patterns, and checklists.
3. This is sufficient to write correct code. Stop here and begin work.

---

## Phase 4: Shared Agent Assets

The vendor-agnostic source of truth for reusable agent assets is:

- `.agents/skills/` for skills
- `.agents/commands/` for commands

Vendor-specific entrypoints may symlink to those directories, but the content lives under `.agents/`.

---

## Phase 5: CLI Mode

When the work is about the CLI, load the shared agent skills from `.agents/skills/` before writing code.

Treat the work as CLI-related if any of these are true:

- The agent starts in `./cli`.
- The request mentions `cli/` paths or files under `cli/`.
- The request is explicitly about the Calypso CLI.

For CLI work:

- Use TDD. Write the test first.
- Then write the minimal stub needed for the test to compile or run.
- Then implement the behavior.
- Push changes and wait for CI jobs to run before treating the work as complete.
- Read only the skill or skills relevant to the task. Do not bulk-load every skill spec.

---

## Phase 6: Decision Policy

Proceed without asking the human when the next step is straightforward, low risk, and consistent with the current issue, PR, and plan.

If confidence is not high enough:

1. Situate the work against the Plan tracking issue and the issue dependency order.
2. Prefer the simplest path that keeps the current PR or selected issue aligned with the next planned work.
3. If still uncertain, read the relevant parts of `calypso-blueprint/`.
4. Only ask the human after those steps fail to produce a confident next action.

The default bias is forward progress, not clarification.

---

## Phase 7: Deepen Context (Only When Needed)

If at any point during implementation you encounter uncertainty, do not ask the human immediately. Escalate context in this order:

```text
CONFIDENCE CHECK
  Can I resolve this from the implementation document?
    YES -> continue working.
    NO  -> proceed to step 1 below.

1. Read the keyword index in agent-context/index.md.
2. Identify the blueprint(s) whose keywords match your uncertainty.
3. Read the relevant blueprint section, not the full document.
4. Apply what you learned. Return to implementation.

Still uncertain after reading the blueprint?
5. Read agent-communication.md section "Document Precedence Rules".
6. Search the codebase for analogous existing implementations.
7. Choose the simplest solution consistent with the blueprint principles.

Still blocked?
8. Only now ask the human. State what you tried, what you found, and the specific decision you need made.
```

This is a context escalation loop, not a one-time decision. Most tasks should complete after loading implementation context.

---

## Commit Standards

Read `agent-context/development/git-standards.md` before your first commit. Key rules:

- Conventional commit format: `type: imperative summary`
- Valid types: `feat`, `fix`, `refactor`, `test`, `docs`, `chore`, `security`
- Stage files explicitly by name. Never run `git add .`
- Never use `--no-verify`
- All tests must pass before committing

---

## Rules

- Autonomy first. Do not ask the human for help unless you have exhausted the context escalation loop.
- Prefer a low-risk forward step over a clarification question when the likely answer is obvious from local context.
- Minimal context loading. Do not read documents speculatively.
- Implementation docs before blueprints. Blueprints explain why; implementation docs tell you what to build.
- One workflow per session. Pick one workflow document and follow it to completion.
- Follow documented patterns exactly. Do not invent alternatives when an implementation document already provides one.
- Update docs you contradict. If implementation must deviate from a documented pattern, update the document before committing.
