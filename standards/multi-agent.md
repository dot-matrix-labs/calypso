# Multi-Agent Protocol

Git is the coordination layer. No external orchestration.

## Work Queue
`docs/plans/implementation-plan.md` on `main` is the shared backlog:
```
- [ ] Unclaimed — available
- [ ] [CLAIMED:session-id] In progress
- [x] Completed and merged
- [ ] [BLOCKED:reason] Needs human input
```
Claim: pull main → select highest-priority unclaimed → mark `[CLAIMED:id]` → commit+push to main (metadata only). Push fails? Pull, pick another. First claim wins. Claims expire after 2h with no PR.

## Branches
`feat/<slug>`, `fix/<slug>`, `review/<pr-number>`. One agent per branch. Each branch has its own `next-prompt.md`. On merge: overwrite with "Task complete. Return to implementation-plan.md."

## Merging
- All branches merge via PR. No direct pushes to main except claim updates.
- PRs under 20 files (pre-push hook enforced).
- Second PR to main resolves conflicts. Read other PR's Git-Brain metadata for intent. Architectural conflicts → `[BLOCKED:architectural conflict with PR #N]`.

## Roles
- **Builder:** claims tasks, writes code+tests, opens PRs (Plan-Stub-TDD)
- **Review:** reads diffs + metadata, checks standards conformance, runs tests, approves/requests changes. No code.
- **Triage:** reads telemetry DB, creates tasks with severity + repro steps. No fixes.
- **Security:** scans PRs for secrets, auth gaps, SQL injection, missing validation/headers. Blocks on critical.
- **Dispatcher (optional):** reorders implementation plan based on deps/risk/urgency. Runs after each PR merge.

## Session Handoff
Session ends mid-task → commit with metadata → update `next-prompt.md` → push branch. New session reads `next-prompt.md` to continue.

## Scaling
| Agents | Model |
|---|---|
| 1 | Serial: single branch, next-prompt state machine |
| 2-3 | Branch-per-agent, human dispatcher |
| 4-8 | Add agent dispatcher + review + triage agents |
| 8+ | Partition monorepo into domains with separate plans |

## Constraints
- Never two agents on same branch. Never push code to main. Never resolve architectural conflicts without human. Always rebase before PR.
