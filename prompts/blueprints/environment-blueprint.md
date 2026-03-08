
# Environment Blueprint

## Development Prerequisites

**Environment & Setup**

* Continuous development, testing, and operation must occur natively on a bare-metal Linux host in the cloud (e.g., DigitalOcean).
* **Local Development Exception:** Users *are allowed* to perform the initial "Scaffold" phase (repository setup, documentation, initial source code) on their local development machines (e.g., Mac or Windows). However, if you (the AI agent) detect that the active environment is a local Mac or Windows machine, you must interject and occasionally "nag" the user, reminding them that to develop correctly and continuously they must move to a remote Linux host.
* Development tasks must be executed by an AI agent (e.g., Claude, Gemini, Codex, or whichever agent is used) running inside a `tmux` session on the remote host to ensure persistence.
* The development host also serves as the live demo/preview server.
* Port `31415` (the designated Calypso dev port) must be exposed on the host to access the live development preview.

**Linux Host Dependencies**

Before bootstrapping a Calypso project, the bare-metal Linux host must have the following system dependencies installed:

1. `git`: Version control.
2. `gh` (GitHub CLI): Must be installed and fully authenticated via HTTPS (`gh auth login -p https -w`)
3. `tmux`: Required to persist the agent's session remotely.
4. `bun`: The core JavaScript runtime.
5. **The AI Agent CLI:** The specific CLI tool for the chosen agent (e.g., Claude Code, Cursor server, Gemini CLI).
6. **Playwright OS Dependencies:** Operating system libraries required to run headless Chromium for browser testing (typically installed via `bunx playwright install-deps`).

---

## Agent Standards

* Different agent vendors (Claude, Codex, Gemini) have different ways of producing instructions for agents. These are currently defined as .md txt files.
You should have a `docs/standards/` folder in the project root which includes the .md files with the various standards that agents should follow.

## Agent Standards Folder

**[CRITICAL] Bootstrap Requirement:** When starting a new project or session, the agent MUST first bootstrap the Calypso standards by running:

  curl -sSL https://raw.githubusercontent.com/dot-matrix-labs/calypso/main/scripts/bootstrap-standards.sh | bash

The agent MUST then read these local files in `./docs/standards/` as the first action of the session. Do NOT proceed with any development or documentation tasks until these standards are read; otherwise, you may fabricate conventions that violate the Calypso architecture.

Create the `docs/standards/` directory if it does not exist:

```
docs/
└── standards/
    ├── calypso-blueprint.md
    ├── documentation-standard.md
    ├── development-standards.md
    ├── git-standards.md
    └── ...
```

These standards are the **source of truth** for this project. Users may customize them in `docs/standards/` to fit their specific requirements.

### Agent Session Requirement

**At the start of every session**, the agent MUST verify that all files in `docs/standards/` are up to date with the latest templates and read them to understand the current project conventions. This applies regardless of which AI vendor or model is being used. Failure to do so may result in work that violates project standards or the fabrication of incorrect development practices.
