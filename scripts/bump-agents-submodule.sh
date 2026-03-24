#!/usr/bin/env bash
# bump-agents-submodule.sh — Advance the .agents submodule pin and open a PR.
#
# Creates a dedicated branch, updates .agents to its latest upstream commit,
# commits the change, and opens a draft PR against main with the included
# submodule commits listed in the body.
#
# Must be run from the repository root.
#
# Usage:
#   ./scripts/bump-agents-submodule.sh
#
# Required environment (for PR creation):
#   GITHUB_TOKEN — GitHub PAT with repo scope (or use gh auth login)
#
# Dependencies: git, gh (GitHub CLI)

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"

SUBMODULE_PATH=".agents"
SUBMODULE_REMOTE="origin/main"

# ── Ensure working tree is clean ─────────────────────────────────────────────
if ! git diff --quiet || ! git diff --cached --quiet; then
  echo "error: working tree or index is dirty — commit or stash changes first" >&2
  exit 1
fi

# ── Resolve current and upstream commits ─────────────────────────────────────
CURRENT_SHA=$(git submodule status "$SUBMODULE_PATH" | awk '{print $1}' | tr -d '-+')
git -C "$SUBMODULE_PATH" fetch --quiet origin main

UPSTREAM_SHA=$(git -C "$SUBMODULE_PATH" rev-parse "$SUBMODULE_REMOTE")

if [ "$CURRENT_SHA" = "$UPSTREAM_SHA" ]; then
  echo "info: .agents submodule is already at the latest commit ($CURRENT_SHA) — nothing to do"
  exit 0
fi

SHORT_NEW=$(echo "$UPSTREAM_SHA" | cut -c1-7)
SHORT_OLD=$(echo "$CURRENT_SHA" | cut -c1-7)

echo "info: advancing .agents from $SHORT_OLD → $SHORT_NEW"

# ── Create branch ─────────────────────────────────────────────────────────────
BRANCH="chore/bump-agents-submodule-${SHORT_NEW}"
BASE_BRANCH="main"

if git show-ref --quiet "refs/heads/$BRANCH"; then
  echo "error: branch $BRANCH already exists — delete it and rerun" >&2
  exit 1
fi

git fetch --quiet origin "$BASE_BRANCH"
git checkout -b "$BRANCH" "origin/$BASE_BRANCH"

# ── Advance submodule ─────────────────────────────────────────────────────────
git -C "$SUBMODULE_PATH" checkout --quiet "$UPSTREAM_SHA"
git add "$SUBMODULE_PATH"

# ── Build commit message ──────────────────────────────────────────────────────
COMMIT_LOG=$(git -C "$SUBMODULE_PATH" log \
  --oneline \
  --no-merges \
  "${CURRENT_SHA}..${UPSTREAM_SHA}" 2>/dev/null || true)

FIRST_SUMMARY=$(git -C "$SUBMODULE_PATH" log \
  --no-merges \
  --format="%s" \
  "${CURRENT_SHA}..${UPSTREAM_SHA}" 2>/dev/null | head -1 || true)

if [ -n "$FIRST_SUMMARY" ]; then
  DESCRIPTION="$(echo "$FIRST_SUMMARY" | cut -c1-72)"
else
  DESCRIPTION="update to $SHORT_NEW"
fi

COMMIT_MSG="chore: bump calypso-agents submodule to $SHORT_NEW ($DESCRIPTION)"

git commit --message "$COMMIT_MSG"

# ── Build PR body ─────────────────────────────────────────────────────────────
if [ -n "$COMMIT_LOG" ]; then
  COMMIT_LIST=$(echo "$COMMIT_LOG" | sed 's/^/- /')
else
  COMMIT_LIST="- (no non-merge commits between $SHORT_OLD and $SHORT_NEW)"
fi

PR_BODY="## Summary

Bumps the \`.agents\` submodule from \`${SHORT_OLD}\` to \`${SHORT_NEW}\`.

### Included commits

${COMMIT_LIST}"

# ── Push and open PR ──────────────────────────────────────────────────────────
git push --set-upstream origin "$BRANCH"

gh pr create \
  --title "$COMMIT_MSG" \
  --body "$PR_BODY" \
  --base "$BASE_BRANCH" \
  --head "$BRANCH" \
  --draft

echo "done: PR opened for $BRANCH"
