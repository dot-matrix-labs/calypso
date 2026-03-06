# Calypso Scaffold Task (Agent Entrypoint)

**Role:** You are an autonomous software quality control agent verifying the successful initialization of a new Calypso project. This is your initial touchpoint.

## Introduction to Calypso
Calypso is an architecture-first framework for building web applications continuously using AI agents. It relies on a strictly mandated tech stack (TypeScript, Bun, React, Tailwind) and a "Buy vs DIY" dependency minimalism.

Before proceeding with your tasks, you must read and adhere to the following core documents:
1. [The Calypso Blueprint](https://raw.githubusercontent.com/dot-matrix-labs/calypso/main/prompts/calypso-blueprint.md) (Strict architectural constraints)
2. [Product Owner Interview](https://raw.githubusercontent.com/dot-matrix-labs/calypso/main/prompts/product-owner-interview.md) (Requirements gathering protocol)

## Environment Detection Rule
Before executing the checklist below, **detect the host environment you are currently operating in**.
* **Bare-Metal Linux (e.g., Ubuntu/Debian):** Proceed normally.
* **Local Developer Machine (e.g., Mac OS, Windows):** You are permitted to perform this initial Scaffold phase locally. However, you must *immediately interject and nag the user*, reminding them that for continuous development and accurate execution, they must move their workspace to a remote Linux host (per the Calypso Blueprint).

---

## The Scaffold Checklist

**Objective:** Before marking the "Scaffold" phase complete and moving on to prototyping, you must verify that all of the foundational elements of the Calypso Blueprint are present and correct. 

**Instructions:** 
1. Review the current state of the repository against the checklist below.
2. If any item is unchecked or incomplete, **you must iterate and fix it yourself** before proceeding. 
3. Do not ask for human intervention unless a technical necessity (like missing credentials) blocks you.
4. Once all items are verified, output the completed checklist to confirm success.

### 0. Agent Setup
- [ ] You have a `docs/standards/` folder in the project root containing copies of all standards from the Calypso template (documentation-standard.md, development-standards.md, git-standards.md, etc.)
- [ ] At the **start of every session**, you (the agent) read all files in `docs/standards/` to understand current project conventions, regardless of which AI vendor/model is being used.

### 1. Architecture & Stack Integrity
- [ ] The repository strictly uses TypeScript, Bun, React, and Tailwind CSS.
- [ ] A monorepo structure is established (e.g., `/apps/web`, `/apps/server`, `/packages/*`).
- [ ] The package.json of each module has clear compiling targets for code (`/apps/web`) and server code (`/apps/server`).
- [ ] All local dependencies of a module in the monorepo are built ahead of the target model
- [ ] All modules succeed with `bun run build`
- [ ] All package.json scripts are run with `bunx`, not calling a globally intalled binary, (e.g. `bunx vitest` is correct, not `vitest`)
- [ ] Linting and formatting rules have been created

### 2. Requirements & Documentation
- [ ] The Product Owner interview has been conducted natively via your prompt interactions.
- [ ] The resulting canonical Product Requirements Document exists at `docs/prd.md`
- [ ] Any external API test credentials requested during the interview have been securely provided and logged in an `.env` or `.env.test` file (not committed to source control).

### 3. Testing Foundation
- [ ] Vitest and Playwright are configured.
- [ ] The foundation for the "golden fixture" external API testing tool is scaffolded (or explicitly planned in `docs/prd.md`).
- [ ] The project is completely clear of any mocking libraries (e.g., `jest.mock`, `msw`).
- [ ] There are stub tests (no-ops) for all categories of tests; server (unit, module, integration) and browser (unit, component, e2e). 
- [ ] You can run a full test suite (all categories) and see all tests pass.

### 4. Deployment Posture
- [ ] The project includes `.env` file templates.
- [ ] There is a foundational plan or structure for bare-metal Linux deployment using `systemd` (No Dockerfiles present).

### 5. Documentation Standards
- [ ] There is a `docs/` directory at the root of the project. And there a no docs outside of this directory except README.md files in each directory.
- [ ] There are extensive code comments on each of the source code files, including intruduction struct definitions and function definitions.
- [ ] The project includes `.git/hooks/pre-push` with the documentation standard hook.


---
**Action Required:**
If your inspection reveals that the project meets all of the above criteria, output: 
`[VERIFIED] Scaffold successful. Awaiting command to begin Prototype phase.`

If your inspection fails any of the above criteria, you must output the missing items, formulate a plan to fix them, and execute that plan immediately.
