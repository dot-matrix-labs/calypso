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

REPO="$(canonical_repo)"
TASKS_REPO="$(tasks_repo)"

ISSUE_JSON="$(gh issue view "$ISSUE_NUMBER" --repo "$TASKS_REPO" --json number,title,url,state)"
ISSUE_TITLE="$(jq -r '.title' <<<"$ISSUE_JSON")"

PR_JSON="$(gh pr list --repo "$REPO" --state open --json number,title,body,headRefName,url,isDraft \
  --jq 'map(select((.body | split("\n"))[]? | test("^(Closes|Fixes|Resolves) #'$ISSUE_NUMBER'$"; "i"))) | .[0]')"

created_branch=false
created_pr=false
pushed_branch=false
created_bootstrap_commit=false

git fetch origin main >/dev/null 2>&1

if [[ -n "$PR_JSON" && "$PR_JSON" != "null" ]]; then
  BRANCH_NAME="$(jq -r '.headRefName' <<<"$PR_JSON")"
else
  BRANCH_NAME="$(issue_branch_name "$ISSUE_NUMBER" "$ISSUE_TITLE")"
fi

WORKTREE_PATH="$(issue_worktree_path "$BRANCH_NAME")"

if [[ ! -d "$WORKTREE_PATH" ]]; then
  if git show-ref --verify --quiet "refs/heads/$BRANCH_NAME"; then
    git worktree add "$WORKTREE_PATH" "$BRANCH_NAME" >/dev/null
  elif git ls-remote --exit-code --heads origin "$BRANCH_NAME" >/dev/null 2>&1; then
    git fetch origin "$BRANCH_NAME" >/dev/null 2>&1
    git branch --track "$BRANCH_NAME" "origin/$BRANCH_NAME" >/dev/null 2>&1 || true
    git worktree add "$WORKTREE_PATH" "$BRANCH_NAME" >/dev/null
  else
    git worktree add "$WORKTREE_PATH" -b "$BRANCH_NAME" origin/main >/dev/null
    created_branch=true
  fi
fi

if ! git ls-remote --exit-code --heads origin "$BRANCH_NAME" >/dev/null 2>&1; then
  git -C "$WORKTREE_PATH" push -u origin "$BRANCH_NAME" >/dev/null
  pushed_branch=true
fi

if [[ -z "$PR_JSON" || "$PR_JSON" == "null" ]]; then
  ahead_of_main="$(git -C "$WORKTREE_PATH" rev-list --count "origin/main..HEAD" 2>/dev/null || printf '0')"
  if [[ "$ahead_of_main" == "0" ]]; then
    git -C "$WORKTREE_PATH" commit --allow-empty -m "chore: initialize issue #$ISSUE_NUMBER worktree" >/dev/null
    git -C "$WORKTREE_PATH" push origin "$BRANCH_NAME" >/dev/null
    created_bootstrap_commit=true
  fi
  pr_url="$(gh pr create --repo "$REPO" --base main --head "$BRANCH_NAME" --draft --title "$ISSUE_TITLE" --body "Closes #$ISSUE_NUMBER")"
  PR_JSON="$(gh pr view "$BRANCH_NAME" --repo "$REPO" --json number,title,body,headRefName,url,isDraft)"
  created_pr=true
else
  pr_url="$(jq -r '.url' <<<"$PR_JSON")"
fi

BRANCH_STATUS="$("$SCRIPT_DIR/remote-branch-status.sh" "$BRANCH_NAME")"

jq -n \
  --argjson issue "$ISSUE_JSON" \
  --arg branch "$BRANCH_NAME" \
  --arg worktree "$WORKTREE_PATH" \
  --argjson pr "$PR_JSON" \
  --argjson branch_status "$BRANCH_STATUS" \
  --argjson created_branch "$created_branch" \
  --argjson pushed_branch "$pushed_branch" \
  --argjson created_bootstrap_commit "$created_bootstrap_commit" \
  --argjson created_pr "$created_pr" \
  '{
    issue: $issue,
    branch: $branch,
    worktree: $worktree,
    pr: {
      number: $pr.number,
      title: $pr.title,
      url: $pr.url,
      is_draft: $pr.isDraft
    },
    branch_status: $branch_status,
    prep_actions: {
      created_branch: $created_branch,
      pushed_branch: $pushed_branch,
      created_bootstrap_commit: $created_bootstrap_commit,
      created_pr: $created_pr
    }
  }'
