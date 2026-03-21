#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

run_json="$("$SCRIPT_DIR/run.sh")"

if [[ "$(jq -r '.state // ""' <<<"$run_json")" == "" ]]; then
  printf '%s\n' "$run_json"
  exit 0
fi

jq '{
  state,
  next_action,
  cleanup: {
    removed: (.cleanup.removed | length),
    skipped: (.cleanup.skipped | length)
  },
  selected_issue: {
    number: .selection.issue.number,
    title: .selection.issue.title
  },
  selected_pr: (
    if (.pr.number // null) == null then null else {
      number: .pr.number,
      url: .pr.url,
      draft: .pr.is_draft
    } end
  ),
  worktree: {
    path: .local.status.worktree,
    exists: .local.status.worktree_exists,
    dirty: .local.status.dirty,
    untracked: .local.status.untracked,
    detached_head: .local.status.detached_head
  },
  blockers: (
    [
      (.prep.reasons[]?),
      (.local.reasons[]?),
      (.merge.reasons[]?)
    ] | map(select(. != null)) | unique
  ),
  branch: {
    name: .local.status.branch,
    behind_remote: .local.status.branch_status.remote_sync.behind,
    behind_main: .local.status.branch_status.main_sync.behind
  },
  ci: {
    total: .pr.checks.total,
    failing: .pr.checks.failing,
    pending: .pr.checks.pending,
    all_green: .pr.checks.all_green
  },
  checklist: {
    total: .pr.issue.checklist.total,
    unchecked: .pr.issue.checklist.unchecked,
    complete: .pr.issue.checklist.complete
  },
  diagnosis: (
    if (.diagnosis.reason_code // null) == null then null else {
      reason_code: .diagnosis.reason_code,
      recommended_action: .diagnosis.recommended_action,
      observations: .diagnosis.observations
    } end
  )
}' <<<"$run_json"
