#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=.agents/scripts/auto/common.sh
source "$SCRIPT_DIR/common.sh"

TARGET="${1:-}"
PR_JSON="$("$SCRIPT_DIR/pr-status.sh" "$TARGET")"
REBASE_JSON="$("$SCRIPT_DIR/needs-rebase.sh" "$TARGET")"

reasons='[]'
ready=true

if [[ "$(jq -r '.is_draft' <<<"$PR_JSON")" == "true" ]]; then
  ready=false
  reasons="$(jq -c '. + ["pr-is-draft"]' <<<"$reasons")"
fi

if [[ "$(jq -r '.checks.all_green' <<<"$PR_JSON")" != "true" ]]; then
  ready=false
  reasons="$(jq -c '. + ["checks-not-green"]' <<<"$reasons")"
fi

if [[ "$(jq -r '.issue.checklist.complete // false' <<<"$PR_JSON")" != "true" ]]; then
  ready=false
  reasons="$(jq -c '. + ["issue-checklist-incomplete"]' <<<"$reasons")"
fi

if [[ "$(jq -r '.mergeable' <<<"$PR_JSON")" != "MERGEABLE" ]]; then
  ready=false
  reasons="$(jq -c '. + ["pr-not-mergeable"]' <<<"$reasons")"
fi

if [[ "$(jq -r '.needs_rebase' <<<"$REBASE_JSON")" == "true" ]]; then
  ready=false
  reasons="$(jq -c '. + ["branch-behind-origin-main"]' <<<"$reasons")"
fi

# Plan position enforcement: all preceding Plan issues must be CLOSED
linked_issue="$(jq -r '.linked_issue_number // empty' <<<"$PR_JSON")"
if [[ -n "$linked_issue" ]]; then
  TASKS_REPO="$(tasks_repo)"
  PLAN_JSON_BODY="$(gh issue list --repo "$TASKS_REPO" --state open --json number,title --jq 'map(select(.title == "Plan")) | .[0].number // empty')"
  if [[ -n "$PLAN_JSON_BODY" ]]; then
    PLAN_BODY="$(gh issue view "$PLAN_JSON_BODY" --repo "$TASKS_REPO" --json body -q .body)"
    mapfile -t plan_issues < <(printf '%s\n' "$PLAN_BODY" | extract_issue_refs)
    predecessor_open=false
    for plan_issue in "${plan_issues[@]}"; do
      [[ -n "$plan_issue" ]] || continue
      if [[ "$plan_issue" == "$linked_issue" ]]; then
        break
      fi
      issue_state="$(gh issue view "$plan_issue" --repo "$TASKS_REPO" --json state -q .state)"
      if [[ "$issue_state" != "CLOSED" ]]; then
        predecessor_open=true
        break
      fi
    done
    if [[ "$predecessor_open" == "true" ]]; then
      ready=false
      reasons="$(jq -c '. + ["plan-predecessor-not-merged"]' <<<"$reasons")"
    fi
  fi
fi

jq -n \
  --argjson pr "$PR_JSON" \
  --argjson rebase "$REBASE_JSON" \
  --argjson ready "$ready" \
  --argjson reasons "$reasons" \
  '{
    ready: $ready,
    reasons: $reasons,
    pr: $pr,
    rebase: $rebase
  }'
