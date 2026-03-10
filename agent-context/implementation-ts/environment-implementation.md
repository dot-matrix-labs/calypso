# Environment — Calypso TypeScript Implementation

<!-- last-edited: 2026-03-10 -->

CONTEXT MAP
  this ──implements──▶ blueprints/environment-blueprint.md
  this ◀──referenced by── index.md

> Implements: Environment Blueprint (`agent-context/blueprints/environment-blueprint.md`)

The principles, threat model, and patterns in that document apply equally to other stacks. This document covers the concrete realization using Bun, tmux, and the Calypso toolchain.

---

## Host Dependencies

The Calypso development host requires exactly these system-level tools:

| Tool | Purpose |
|---|---|
| `git` | Version control |
| `gh` | GitHub CLI, authenticated via HTTPS (`gh auth login -p https -w`) |
| `tmux` | Terminal multiplexer for persistent sessions |
| `bun` | JavaScript/TypeScript runtime and package manager |
| Agent CLI | Claude Code, Cursor server, Gemini CLI, or equivalent |
| Playwright OS deps | Headless Chromium libraries (`bunx playwright install-deps`) |

## Session Start

At the start of every session, read all files in `agent-context/` before proceeding with any development or documentation work.

## Preview Server

The Calypso dev server binds to port `31415`. This port is the project-wide convention and must be exposed on the host firewall.

## Dependency Justification

| Package | Reason to Buy | Justified |
|---|---|---|
| `tmux` | Session persistence with decades of stability; DIY terminal multiplexer is absurd | Yes |
| `bun` | Runtime, bundler, test runner, package manager in one binary; replaces Node + npm + webpack + jest | Yes |
| `gh` | GitHub API integration with auth management; DIY is fragile and under-tested | Yes |
| Playwright | Headless browser automation with cross-browser support; no viable DIY alternative | Yes |
