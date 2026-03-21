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

status_json="$("$SCRIPT_DIR/worktree-status.sh" "$ISSUE_NUMBER")"
state="clean"
reasons='[]'
next_action="develop"

if [[ "$(jq -r '.worktree_exists' <<<"$status_json")" != "true" ]]; then
  state="missing-worktree"
  reasons="$(jq -c '. + ["worktree-missing"]' <<<"$reasons")"
  next_action="resume_issue"
elif [[ "$(jq -r '.detached_head' <<<"$status_json")" == "true" ]]; then
  state="detached-head"
  reasons="$(jq -c '. + ["detached-head"]' <<<"$reasons")"
  next_action="resume_issue"
elif [[ "$(jq -r '.current_branch // ""' <<<"$status_json")" != "$(jq -r '.branch' <<<"$status_json")" ]]; then
  state="wrong-branch"
  reasons="$(jq -c '. + ["wrong-branch-checked-out"]' <<<"$reasons")"
  next_action="resume_issue"
elif [[ "$(jq -r '.dirty' <<<"$status_json")" == "true" ]]; then
  state="dirty-but-resumable"
  reasons="$(jq -c '. + ["uncommitted-worktree-changes"]' <<<"$reasons")"
  next_action="resume_dirty_worktree"
fi

if [[ "$(jq -r '.branch_status.remote_sync.behind' <<<"$status_json")" != "0" ]]; then
  state="remote-diverged"
  reasons="$(jq -c '. + ["branch-behind-remote"]' <<<"$reasons")"
  next_action="resume_issue"
fi

jq -n \
  --arg state "$state" \
  --arg next_action "$next_action" \
  --argjson reasons "$reasons" \
  --argjson status "$status_json" \
  '{
    state: $state,
    next_action: $next_action,
    reasons: $reasons,
    status: $status
  }'
