#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=.agents/scripts/auto/common.sh
source "$SCRIPT_DIR/common.sh"

TARGET="${1:-}"
if [[ -z "$TARGET" ]]; then
  printf 'usage: %s <pr-number-or-branch>\n' "$(basename "$0")" >&2
  exit 1
fi

branch_name="$TARGET"
if [[ "$TARGET" =~ ^[0-9]+$ ]]; then
  branch_name="$(gh pr view "$TARGET" --repo "$(canonical_repo)" --json headRefName -q .headRefName)"
fi

worktree_path="$(issue_worktree_path "$branch_name")"
if [[ ! -d "$worktree_path" ]]; then
  printf 'missing worktree for branch %s at %s\n' "$branch_name" "$worktree_path" >&2
  exit 2
fi

git -C "$worktree_path" fetch origin main >/dev/null 2>&1
git -C "$worktree_path" rebase origin/main
git -C "$worktree_path" push --force-with-lease origin "$branch_name"

branch_status="$("$SCRIPT_DIR/remote-branch-status.sh" "$branch_name")"

jq -n \
  --arg branch "$branch_name" \
  --arg worktree "$worktree_path" \
  --argjson branch_status "$branch_status" \
  '{
    branch: $branch,
    worktree: $worktree,
    branch_status: $branch_status
  }'
