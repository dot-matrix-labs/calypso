#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=.agents/scripts/auto/common.sh
source "$SCRIPT_DIR/common.sh"

ISSUE_NUMBER="${1:-}"
if [[ -z "$ISSUE_NUMBER" ]]; then
  printf 'usage: %s <issue-number>\n' "$(basename "$0")" >&2
  exit 1
fi

lookup="$("$SCRIPT_DIR/find-issue-worktree.sh" "$ISSUE_NUMBER")"
branch_name="$(jq -r '.branch' <<<"$lookup")"
worktree_path="$(jq -r '.worktree' <<<"$lookup")"
worktree_exists="$(jq -r '.worktree_exists' <<<"$lookup")"

dirty=false
untracked=false
detached_head=false
current_branch=""
porcelain=""

if [[ "$worktree_exists" == "true" ]]; then
  porcelain="$(git -C "$worktree_path" status --porcelain 2>/dev/null || true)"
  [[ -n "$porcelain" ]] && dirty=true
  grep -qE '^\?\?' <<<"$porcelain" && untracked=true || true
  current_branch="$(git -C "$worktree_path" branch --show-current 2>/dev/null || true)"
  [[ -z "$current_branch" ]] && detached_head=true
fi

branch_status="$("$SCRIPT_DIR/remote-branch-status.sh" "$branch_name")"

jq -n \
  --argjson lookup "$lookup" \
  --arg current_branch "$current_branch" \
  --arg porcelain "$porcelain" \
  --argjson dirty "$dirty" \
  --argjson untracked "$untracked" \
  --argjson detached_head "$detached_head" \
  --argjson branch_status "$branch_status" \
  '{
    issue: $lookup.issue,
    branch: $lookup.branch,
    worktree: $lookup.worktree,
    worktree_exists: $lookup.worktree_exists,
    pr: $lookup.pr,
    current_branch: (if $current_branch == "" then null else $current_branch end),
    dirty: $dirty,
    untracked: $untracked,
    detached_head: $detached_head,
    porcelain: (if $porcelain == "" then [] else ($porcelain | split("\n") | map(select(length > 0))) end),
    branch_status: $branch_status
  }'
