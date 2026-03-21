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

TASKS_REPO="$(tasks_repo)"
REPO="$(canonical_repo)"
issue_json="$(gh issue view "$ISSUE_NUMBER" --repo "$TASKS_REPO" --json number,title,url,state)"
issue_title="$(jq -r '.title' <<<"$issue_json")"

pr_json="$(gh pr list --repo "$REPO" --state all --json number,title,body,headRefName,url,isDraft,state \
  --jq 'map(select((.body | split("\n"))[]? | test("^(Closes|Fixes|Resolves) #'$ISSUE_NUMBER'$"; "i"))) | .[0]')"

if [[ -n "$pr_json" && "$pr_json" != "null" ]]; then
  branch_name="$(jq -r '.headRefName' <<<"$pr_json")"
else
  branch_name="$(issue_branch_name "$ISSUE_NUMBER" "$issue_title")"
fi

worktree_path="$(issue_worktree_path "$branch_name")"
if [[ -d "$worktree_path" ]]; then
  worktree_exists=true
else
  worktree_exists=false
fi

jq -n \
  --argjson issue "$issue_json" \
  --arg branch "$branch_name" \
  --arg worktree "$worktree_path" \
  --argjson pr "${pr_json:-null}" \
  --argjson exists "$worktree_exists" \
  '{
    issue: $issue,
    branch: $branch,
    worktree: $worktree,
    worktree_exists: $exists,
    pr: $pr
  }'
