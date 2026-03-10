# Documentation Standard (Business Application)

<!-- last-edited: 2026-03-10 -->

CONTEXT MAP
  this ──governs─────────▶ docs/ and README.md files within the PROJECT BEING BUILT
  this ──does NOT govern─▶ agent-context/ (governed by agent-communication.md)

> **Scope:** This document governs documentation structure within the **business application project** that Calypso agents build — the `docs/` directory, per-directory `README.md` files, and source code inline documentation. It does NOT govern `agent-context/` documents (those are governed by `agent-communication.md`).

This document defines the Documentation Fractal methodology for Calypso projects.

## Core Principle

**"The codebase is the map."**

Every directory is a node in a tree. Every node must describe itself. Autonomous agents should be able to navigate the codebase without vector databases, RAG, or external search tools.

## Rules

### 1. The Root Anchor

The project root must contain a `README.md` that serves as the "High Orbit" view. It points to the main entry points (e.g., `./docs/README.md`, `./src/README.md`).

### 2. No Stranded Docs

There should be **no documentation files** (other than `README.md`) outside of the canonical `./docs/` directory.

- **Why?** Scatter-gun documentation gets lost. By centralizing "substantive" documentation in `./docs/`, agents know exactly where to look for knowledge.
- **Exception**: `README.md` files are allowed (and required) in every sub-directory to explain *what that directory contains*.

### 3. The Fractal Structure

Every subdirectory (node) should contain a `README.md` that:

1. **Summarizes** the directory's purpose.
2. **Lists** key files or subdirectories.
3. **Links** back to the "parent" knowledge base if deeper context is needed (usually in `./docs/`).

```
root/
├── README.md              # High Orbit: "What is this project?" -> Links to docs/README.md
├── src/
│   ├── README.md          # "This is the source code." -> Links to architecture docs in docs/technical/
│   ├── modules/
│   │   ├── auth/
│   │   │   ├── README.md  # "This handles auth." -> Links to auth specs in docs/features/
```

### 4. Canonical Naming

All documentation files inside `./docs/` must use **kebab-case** (`my-doc.md`) to ensure consistent URL routing and OS compatibility.

### 5. README.md in Every Directory

Every directory in the project should contain a `README.md` that explains:
- What the directory contains
- Key files and their purposes
- How it relates to other parts of the project

## Benefits for Agents

1. **O(1) Navigation**: An agent entering any directory can read `README.md` to understand the context immediately.
2. **Deterministic Search**: Agents don't need to guess "where are the docs?". They are always in `./docs/` or in the local `README.md`.
3. **Self-healing**: If an agent adds a new directory, the documentation standard reminds them to add a `README.md`.

## Enforcement

To enforce this standard, create a `pre-push` git hook at `.git/hooks/pre-push`:

```bash
#!/bin/sh

# Pre-push hook to enforce documentation standard
# Prevents pushing documentation files outside of ./docs/ directory

while read local_ref local_sha remote_ref remote_sha; do
    if [ "$remote_ref" = "refs/heads/"* ]; then
        if [ "$local_sha" = "0000000000000000000000000000000000000000" ]; then
            range="$remote_sha"
        else
            range="$remote_sha..$local_sha"
        fi

        invalid_docs=$(git diff --name-only $range 2>/dev/null | grep -E '\.(md|txt|rst)$' | grep -v '^docs/' | grep -v '/README.md$' | grep -v '^README.md$' || true)

        if [ -n "$invalid_docs" ]; then
            echo "ERROR: Documentation files detected outside of ./docs/ directory."
            echo ""
            echo "The following documentation files would be pushed outside of ./docs/:"
            echo "$invalid_docs"
            echo ""
            echo "Per the Documentation Fractal standard, all documentation files"
            echo "(other than README.md in each directory) must be placed in ./docs/"
            exit 1
        fi
    fi
done

exit 0
```

Make it executable: `chmod +x .git/hooks/pre-push`

