
# Process Blueprint

## Development Lifecycle

0. **Quickstart / Scaffold:**
   * **Version Control:** Initialize git (`git init`), authenticate GitHub CLI using HTTPS (`gh auth login -p https -w`), and create the remote repository (`gh repo create`).
   * **CI Setup:** Immediately create the CI jobs (e.g., GitHub Actions in `.github/workflows/`) so they run from day one.
   * **TDD Environment:** Calypso runs on hosted Linux and has no GUI or display server. Agents are headless by nature and must never attempt to open a browser window or launch a GUI application. All browser interaction happens through a headless Chromium instance driven by Playwright. Visual output is evaluated by capturing screenshots and inspecting them programmatically or via a vision-capable model. You should always use a headless instance, execute headless browser tests (e.g., Playwright), and strictly do Test-Driven Development (TDD). You should stub all the testsuites before building any features: server unit, integration, browser unit, browser component, browser e2e.

1. **Collect Specifications:** The AI agent must generate an `.md` document containing comprehensive onboarding interview questions for the Product Owner to extract requirements. An explicit template prompt is provided to instruct the agent on generating these questions. The agent then writes a canonical Product Requirements Doc to `docs/prd.md` based on the answers. The Product Owner/Manager will own and update this document moving forward.

1a. **Implementation Plan:** After the PRD is established, the agent creates `docs/plans/implementation-plan.md`. This is a living checklist of concrete work tasks, distinct from the PRD: the PRD describes *what* the product must do; the implementation plan describes *how* the agent will build it, in what order, and what remains to be done.

   * Tasks are written as markdown checkboxes (`- [ ]`), grouped by phase or area.
   * The plan is updated at **every git commit** — this is enforced by a pre-commit hook (see git-standards). Updates are twofold:
     1. **Discovery:** New tasks or re-ordered tasks learned during implementation are added.
     2. **Completion:** Finished tasks are checked off (`- [x]`).
   * The implementation plan is the agent's working memory across the full arc of a project. It answers: what has been done, what remains, and in what order.

1b. **Next Prompt:** Alongside the implementation plan, the agent maintains `docs/plans/next-prompt.md`. This file contains a single, self-contained, immediately executable prompt describing the **very next action** the agent should take.

   * It is updated at **every git commit**, enforced by the same pre-commit hook as the implementation plan.
   * It is written in second person, addressed to the agent picking up the next commit: "Read X, then do Y, paying attention to Z."
   * It must include enough context to begin the next task without human input — what was just completed, what comes next, and any constraints or gotchas relevant to that work.
   * This closes the loop: each commit ends by writing the prompt for the next commit, creating a **self-advancing state machine**. A commit is the unit of progress; an agent session spans many commits and may execute them continuously without waiting for a human prompt between each one.
   * Humans can override the next task at any time by editing `docs/plans/next-prompt.md` directly.

   The relationship between the three planning documents:
   | File | Scope | Owner |
   |---|---|---|
   | `docs/prd.md` | What the product must do | Human (Product Owner) |
   | `docs/plans/implementation-plan.md` | All tasks, ordered, with completion state | Agent, updated each commit |
   | `docs/plans/next-prompt.md` | The single next action | Agent, updated each commit |

2. **Prototype:** mock data, minimal UI, basic flows, no persistence.
3. **Demoware:** partial integrations, realistic UI, stable demo workflows.
4. **Alpha:** full persistence, authentication, core business logic.
5. **Beta:** external integrations, performance, reliability, metrics.
6. **V1:** production-ready stability, observability, backups.
