#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=.agents/scripts/dev-loop/common.sh
source "$SCRIPT_DIR/common.sh"

selection="$("$SCRIPT_DIR/select-next-work.sh")"
kind="$(jq -r '.kind' <<<"$selection")"

if [[ "$kind" == "none" ]]; then
  printf '%s\n' "$selection"
  exit 0
fi

issue_number="$(jq -r '.issue.number' <<<"$selection")"
prep="$("$SCRIPT_DIR/verify-issue-prep.sh" "$issue_number")"

if [[ "$(jq -r '.ok' <<<"$prep")" != "true" ]]; then
  jq -n \
    --argjson selection "$selection" \
    --argjson prep "$prep" \
    '{
      state: "prep_failed",
      selection: $selection,
      prep: $prep
    }'
  exit 2
fi

pr_number="$(jq -r '.prep.pr.number' <<<"$prep")"
pr_status="$("$SCRIPT_DIR/pr-status.sh" "$pr_number")"
local_state="$("$SCRIPT_DIR/reconcile-local-state.sh" "$issue_number")"
rebase_status="$("$SCRIPT_DIR/needs-rebase.sh" "$pr_number")"
merge_status="$("$SCRIPT_DIR/merge-ready.sh" "$pr_number")"

next_action="develop"
state="active"

if [[ "$(jq -r '.state' <<<"$local_state")" != "clean" ]]; then
  next_action="$(jq -r '.next_action' <<<"$local_state")"
  state="local_state_needs_attention"
elif [[ "$(jq -r '.ready' <<<"$merge_status")" == "true" ]]; then
  next_action="merge"
elif [[ "$(jq -r '.needs_rebase' <<<"$rebase_status")" == "true" ]]; then
  next_action="rebase"
fi

jq -n \
  --arg state "$state" \
  --arg next_action "$next_action" \
  --argjson selection "$selection" \
  --argjson prep "$prep" \
  --argjson pr "$pr_status" \
  --argjson local "$local_state" \
  --argjson rebase "$rebase_status" \
  --argjson merge "$merge_status" \
  '{
    state: $state,
    next_action: $next_action,
    selection: $selection,
    prep: $prep,
    pr: $pr,
    local: $local,
    rebase: $rebase,
    merge: $merge
  }'
