#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=.agents/scripts/auto/common.sh
source "$SCRIPT_DIR/common.sh"

git worktree prune >/dev/null 2>&1 || true

repo="$(canonical_repo)"
merged_prs="$(gh pr list --repo "$repo" --state merged --limit 200 --json number,headRefName,mergedAt)"
pattern="$(managed_issue_branch_regex)"
current_root="$(pwd)"
removed='[]'
skipped='[]'

worktree_path=""
branch_name=""

while IFS= read -r line; do
  case "$line" in
    worktree\ *)
      worktree_path="${line#worktree }"
      branch_name=""
      ;;
    branch\ refs/heads/*)
      branch_name="${line#branch refs/heads/}"
      if [[ -z "$worktree_path" || -z "$branch_name" ]]; then
        continue
      fi
      if [[ ! "$branch_name" =~ $pattern ]]; then
        skipped="$(jq -c --arg branch "$branch_name" --arg path "$worktree_path" '. + [{branch: $branch, path: $path, reason: "nonconforming-branch-name"}]' <<<"$skipped")"
        continue
      fi
      if [[ "$worktree_path" != "$(worktree_root)"/* ]]; then
        skipped="$(jq -c --arg branch "$branch_name" --arg path "$worktree_path" '. + [{branch: $branch, path: $path, reason: "outside-managed-worktree-root"}]' <<<"$skipped")"
        continue
      fi
      if [[ "$worktree_path" == "$current_root" ]]; then
        skipped="$(jq -c --arg branch "$branch_name" --arg path "$worktree_path" '. + [{branch: $branch, path: $path, reason: "current-worktree"}]' <<<"$skipped")"
        continue
      fi
      if jq -e --arg branch "$branch_name" 'any(.[]; .headRefName == $branch and .mergedAt != null)' <<<"$merged_prs" >/dev/null; then
        git worktree remove --force "$worktree_path" >/dev/null 2>&1 || true
        if git show-ref --verify --quiet "refs/heads/$branch_name"; then
          git branch -D "$branch_name" >/dev/null 2>&1 || true
        fi
        removed="$(jq -c --arg branch "$branch_name" --arg path "$worktree_path" '. + [{branch: $branch, path: $path}]' <<<"$removed")"
      fi
      ;;
    "")
      worktree_path=""
      branch_name=""
      ;;
  esac
done < <(git worktree list --porcelain)

git worktree prune >/dev/null 2>&1 || true

jq -n --argjson removed "$removed" --argjson skipped "$skipped" '{
  ok: true,
  removed: $removed,
  skipped: $skipped
}'
