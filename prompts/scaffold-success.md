# For Agents: Calypso Scaffold Success Checklist

**Role:** You are an autonomous software quality control agent verifying the successful initialization of a new Calypso project. 

**Objective:** Before marking the "Scaffold" phase complete and moving on to prototyping, you must verify that all of the foundational elements of the Calypso Blueprint and Nightshift protocol are present and correct. 

**Instructions:** 
1. Review the current state of the repository against the checklist below.
2. If any item is unchecked or incomplete, **you must iterate and fix it yourself** before proceeding. 
3. Do not ask for human intervention unless a technical necessity (like missing credentials) blocks you.
4. Once all items are verified, output the completed checklist to confirm success.

---

## The Scaffold Checklist

### 1. Nightshift Context & Tooling
- [ ] The `.nightshift/` repository directory exists.
- [ ] The Nightshift agent shim file is correctly installed for your specific agent vendor (e.g., cursor, claude, gemini).
- [ ] Nightshift "Nags" (git-hooks) are installed and actively enforcing quality gates (linting, formatting, testing) on commit.

### 2. Architecture & Stack Integrity
- [ ] The repository strictly uses TypeScript, Bun, React, and Tailwind CSS.
- [ ] A monorepo structure is established (e.g., `/apps/web`, `/apps/server`, `/packages/*`).
- [ ] There is a strict boundary between browser code (`/apps/web`) and server code (`/apps/server`).

### 3. Requirements & Documentation
- [ ] The Product Owner interview has been conducted.
- [ ] The resulting canonical Product Requirements Document exists at `docs/prd.md` according to Nightshift rules.
- [ ] Any external API test credentials requested during the interview have been securely provided and logged in an `.env` or `.env.test` file (not committed to source control).

### 4. Testing Foundation
- [ ] Vitest and Playwright are configured.
- [ ] The foundation for the "golden fixture" external API testing tool is scaffolded (or explicitly planned in `docs/prd.md`).
- [ ] The project is completely clear of any mocking libraries (e.g., `jest.mock`, `msw`).

### 5. Deployment Posture
- [ ] The project includes `.env` file templates.
- [ ] There is a foundational plan or structure for bare-metal Linux deployment using `systemd` (No Dockerfiles present).

---
**Action Required:**
If your inspection reveals that the project meets all of the above criteria, output: 
`[VERIFIED] Scaffold successful. Awaiting command to begin Prototype phase.`

If your inspection fails any of the above criteria, you must output the missing items, formulate a plan to fix them, and execute that plan immediately.
