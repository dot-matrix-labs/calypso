# Agent Session Bootstrap — Vendor Configuration Guide

This document explains how to configure each major AI coding agent so that it reads the Calypso blueprint and project PRD at the start of **every** session, task, or subprocess. Agents suffer from context amnesia between sessions; this configuration is the guardrail that prevents them from inventing conventions or violating architecture standards.

## The Two Files Every Agent Must Read First

Before doing anything else in a Calypso project, an agent must read:

1. `docs/standards/calypso-blueprint.md` — architecture, stack, process, and testing standards
2. `docs/prd.md` — the canonical product requirements for this specific project

If these files do not yet exist locally, the agent must bootstrap them first:

```bash
curl -sSL https://raw.githubusercontent.com/dot-matrix-labs/calypso/main/scripts/bootstrap-standards.sh | bash
```

---

## Vendor-Specific Configuration

### 1. Claude Code (Anthropic)

**Mechanism:** `CLAUDE.md` at the project root is automatically injected into every session context. Claude Code also reads parent-directory and home-directory `CLAUDE.md` files.

**Setup:** Add the following to your project's `CLAUDE.md`:

```markdown
## Session Bootstrap — Read First

At the start of every session, before taking any action, you MUST read:

1. `docs/standards/calypso-blueprint.md`
2. `docs/prd.md`

If either file is missing, run:
  curl -sSL https://raw.githubusercontent.com/dot-matrix-labs/calypso/main/scripts/bootstrap-standards.sh | bash

Do not proceed with any development, documentation, or planning tasks until both files are read.
```

**Global variant:** To apply this across all Calypso projects on a machine, add the same block to `~/.claude/CLAUDE.md`.

**Subprocess note:** Claude Code subagents launched via the `Agent` tool inherit the parent session's context, including `CLAUDE.md`. No additional configuration is needed for subagents in the same project.

---

### 2. OpenAI Codex CLI

**Mechanism:** `AGENTS.md` at the project root (and in any parent directory up to the repo root) is automatically read at the start of every Codex session.

**Setup:** Create or update `AGENTS.md` at the project root:

```markdown
## Session Bootstrap — Read First

At the start of every task, before taking any action, read:

1. docs/standards/calypso-blueprint.md
2. docs/prd.md

If either file is missing, run:
  curl -sSL https://raw.githubusercontent.com/dot-matrix-labs/calypso/main/scripts/bootstrap-standards.sh | bash

Do not proceed until both files are confirmed read and understood.
```

**Subprocess note:** Codex subagents inherit the `AGENTS.md` from the working directory. If subagents are spawned in subdirectories, place an `AGENTS.md` there too, or ensure the root-level file is in a parent directory they all share.

---

### 3. Gemini CLI (Google)

**Mechanism:** `GEMINI.md` at the project root is automatically read at session start. A `~/.gemini/GEMINI.md` global file is also supported.

**Setup:** Create `GEMINI.md` at the project root:

```markdown
## Session Bootstrap — Read First

At the start of every session, before any action, read:

1. docs/standards/calypso-blueprint.md
2. docs/prd.md

If either file is missing, run:
  curl -sSL https://raw.githubusercontent.com/dot-matrix-labs/calypso/main/scripts/bootstrap-standards.sh | bash

Do not proceed with development or planning tasks until both files are read.
```

---

### 4. Cursor

**Mechanism:** Project rules in `.cursor/rules/` are injected into agent context. Rules with `alwaysApply: true` in their frontmatter are included in every request, regardless of what files are open.

**Setup:** Create `.cursor/rules/calypso-bootstrap.mdc`:

```
---
description: Calypso session bootstrap — always read standards and PRD first
alwaysApply: true
---

At the start of every session or task, before any action, read:

1. docs/standards/calypso-blueprint.md
2. docs/prd.md

If either file is missing, run:
  curl -sSL https://raw.githubusercontent.com/dot-matrix-labs/calypso/main/scripts/bootstrap-standards.sh | bash

Do not proceed with development, documentation, or planning until both files are confirmed read.
```

**Legacy note:** Older Cursor versions use `.cursorrules` at the project root instead of `.cursor/rules/`. If using a legacy version, place the same content in `.cursorrules`.

---

### 5. Windsurf (Codeium)

**Mechanism:** `.windsurfrules` at the project root is read at session start. Global rules can also be set in Windsurf Settings > AI > Custom Instructions.

**Setup:** Create `.windsurfrules` at the project root:

```
At the start of every session, before any action, read:

1. docs/standards/calypso-blueprint.md
2. docs/prd.md

If either file is missing, run:
  curl -sSL https://raw.githubusercontent.com/dot-matrix-labs/calypso/main/scripts/bootstrap-standards.sh | bash

Do not proceed with any development or planning tasks until both files are read.
```

**Global variant:** For a machine-wide default, add the same instruction to Windsurf Settings > AI > Global Rules.

---

### 6. GitHub Copilot (VS Code / JetBrains)

**Mechanism:** `.github/copilot-instructions.md` is injected as custom instructions into every Copilot Chat request for the repository.

**Setup:** Create `.github/copilot-instructions.md`:

```markdown
## Session Bootstrap

At the start of every session or task, before taking any action, read:

1. `docs/standards/calypso-blueprint.md`
2. `docs/prd.md`

If either file is missing, run:
  curl -sSL https://raw.githubusercontent.com/dot-matrix-labs/calypso/main/scripts/bootstrap-standards.sh | bash

Do not proceed with development, documentation, or planning until both files are confirmed read.
```

**Limitation:** Copilot Chat does not automatically open and read files — it injects the instruction text but you may need to explicitly ask it to read the files if they are long. For critical sessions, paste the key constraints from both files directly into your opening prompt.

---

### 7. Aider

**Mechanism:** Aider can be configured to always read specific files via `.aider.conf.yml` or by passing `--read` on the CLI. Read files are included in the model's context at startup.

**Setup:** Create `.aider.conf.yml` at the project root:

```yaml
read:
  - docs/standards/calypso-blueprint.md
  - docs/prd.md
```

**CLI equivalent** (for one-off sessions or CI):

```bash
aider --read docs/standards/calypso-blueprint.md --read docs/prd.md
```

**Note:** Aider also auto-reads a `CONVENTIONS.md` file if present. You can use this as a lightweight pointer:

```markdown
# CONVENTIONS.md
See docs/standards/calypso-blueprint.md and docs/prd.md for all project conventions.
Read both files before taking any action.
```

---

### 8. Cline (VS Code Extension)

**Mechanism:** `.clinerules` at the project root is read at the start of every task and injected into the system prompt.

**Setup:** Create `.clinerules` at the project root:

```
At the start of every task, before any action, read:

1. docs/standards/calypso-blueprint.md
2. docs/prd.md

If either file is missing, run:
  curl -sSL https://raw.githubusercontent.com/dot-matrix-labs/calypso/main/scripts/bootstrap-standards.sh | bash

Do not proceed with any development or planning until both files are confirmed read.
```

**Global variant:** Cline also supports global custom instructions in VS Code Settings > Cline > Custom Instructions.

---

### 9. Devin (Cognition)

**Mechanism:** Devin uses Playbooks — structured instruction documents defined in the Devin UI — that are attached to sessions or organizations. A `DEVIN.md` file in the repository root is also read when Devin is pointed at a repo.

**Setup — `DEVIN.md`:** Create `DEVIN.md` at the project root:

```markdown
## Session Bootstrap — Read First

Before taking any action in this repository, read:

1. docs/standards/calypso-blueprint.md
2. docs/prd.md

If either file is missing, run:
  curl -sSL https://raw.githubusercontent.com/dot-matrix-labs/calypso/main/scripts/bootstrap-standards.sh | bash

Do not begin development, planning, or documentation work until both files are read and confirmed.
```

**Setup — Playbook:** In the Devin UI, create an organization-level Playbook with the same content. Attach it to all sessions involving Calypso projects.

---

## The Source File: `AGENT.md`

`AGENT.md` at the project root is the single source of truth for agent bootstrap instructions. It is human-maintained. The deploy script below reads it and places it in the correct vendor-specific location and format.

Create `AGENT.md` once per project:

```markdown
## Session Bootstrap — Read First

At the start of every session, before any action, read:

1. docs/standards/calypso-blueprint.md
2. docs/prd.md

If either file is missing, run:
  curl -sSL https://raw.githubusercontent.com/dot-matrix-labs/calypso/main/scripts/bootstrap-standards.sh | bash

Do not proceed with development, documentation, or planning until both files are confirmed read.
```

## Deploy Script: `scripts/install-agent-config.sh`

Run this script with the vendor name as an argument. It reads `AGENT.md` and writes the correct file to the correct path for that vendor.

```bash
#!/usr/bin/env bash
# Usage: ./scripts/install-agent-config.sh <vendor>
# Vendors: claude, codex, gemini, cursor, windsurf, copilot, aider, cline, devin
#
# Reads AGENT.md and installs it in the correct location for the specified vendor.

set -euo pipefail

VENDOR="${1:-}"
SOURCE="AGENT.md"

if [[ -z "$VENDOR" ]]; then
  echo "Usage: $0 <vendor>"
  echo "Vendors: claude, codex, gemini, cursor, windsurf, copilot, aider, cline, devin"
  exit 1
fi

if [[ ! -f "$SOURCE" ]]; then
  echo "Error: $SOURCE not found. Create it first."
  exit 1
fi

CONTENT=$(cat "$SOURCE")

case "$VENDOR" in
  claude)
    DEST="CLAUDE.md"
    cp "$SOURCE" "$DEST"
    echo "Installed -> $DEST"
    ;;

  codex)
    DEST="AGENTS.md"
    cp "$SOURCE" "$DEST"
    echo "Installed -> $DEST"
    ;;

  gemini)
    DEST="GEMINI.md"
    cp "$SOURCE" "$DEST"
    echo "Installed -> $DEST"
    ;;

  windsurf)
    DEST=".windsurfrules"
    cp "$SOURCE" "$DEST"
    echo "Installed -> $DEST"
    ;;

  cline)
    DEST=".clinerules"
    cp "$SOURCE" "$DEST"
    echo "Installed -> $DEST"
    ;;

  devin)
    DEST="DEVIN.md"
    cp "$SOURCE" "$DEST"
    echo "Installed -> $DEST"
    ;;

  copilot)
    DEST=".github/copilot-instructions.md"
    mkdir -p .github
    cp "$SOURCE" "$DEST"
    echo "Installed -> $DEST"
    ;;

  cursor)
    DEST=".cursor/rules/calypso-bootstrap.mdc"
    mkdir -p .cursor/rules
    # Cursor rules require an MDC frontmatter block
    cat > "$DEST" <<EOF
---
description: Calypso session bootstrap — read standards and PRD before every task
alwaysApply: true
---
${CONTENT}
EOF
    echo "Installed -> $DEST"
    ;;

  aider)
    # Aider uses a YAML config pointing at files to read, not a copy of the content
    DEST=".aider.conf.yml"
    cat > "$DEST" <<EOF
read:
  - docs/standards/calypso-blueprint.md
  - docs/prd.md
EOF
    echo "Installed -> $DEST"
    echo "Note: Aider reads the standards files directly rather than AGENT.md."
    ;;

  *)
    echo "Unknown vendor: $VENDOR"
    echo "Vendors: claude, codex, gemini, cursor, windsurf, copilot, aider, cline, devin"
    exit 1
    ;;
esac
```

**Example usage:**

```bash
chmod +x scripts/install-agent-config.sh

# Install for the agent you are actively using
./scripts/install-agent-config.sh claude
./scripts/install-agent-config.sh cursor
```

**To install for all vendors at once:**

```bash
for vendor in claude codex gemini cursor windsurf copilot aider cline devin; do
  ./scripts/install-agent-config.sh "$vendor"
done
```

Edit `AGENT.md` whenever the bootstrap instructions change, then re-run the script. Commit both `AGENT.md` and the generated vendor files so the repo is ready for any agent a contributor brings.

---

## Summary Table

| Vendor         | File / Mechanism                          | Auto-read at session start |
|----------------|-------------------------------------------|----------------------------|
| Claude Code    | `CLAUDE.md`                               | Yes                        |
| Codex CLI      | `AGENTS.md`                               | Yes                        |
| Gemini CLI     | `GEMINI.md`                               | Yes                        |
| Cursor         | `.cursor/rules/*.mdc` (alwaysApply: true) | Yes                        |
| Windsurf       | `.windsurfrules`                          | Yes                        |
| GitHub Copilot | `.github/copilot-instructions.md`         | Yes (injected as text)     |
| Aider          | `.aider.conf.yml` (read: list)            | Yes                        |
| Cline          | `.clinerules`                             | Yes                        |
| Devin          | `DEVIN.md` + Playbook                     | Yes                        |
