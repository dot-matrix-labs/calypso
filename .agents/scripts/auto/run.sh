#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=.agents/scripts/auto/common.sh
source "$SCRIPT_DIR/common.sh"

cleanup="$("$SCRIPT_DIR/cleanup-stale-worktrees.sh")"
selection="$("$SCRIPT_DIR/select-next-work.sh")"
kind="$(jq -r '.kind' <<<"$selection")"

if [[ "$kind" == "none" ]]; then
  payload="$(jq -n \
    --argjson cleanup "$cleanup" \
    --argjson selection "$selection" \
    '{
      cleanup: $cleanup,
      selection: $selection
    }')"
  diagnosis="$(printf '%s\n' "$payload" | "$SCRIPT_DIR/diagnose-state.sh" || true)"
  if [[ -n "$diagnosis" ]]; then
    jq -n \
      --argjson cleanup "$cleanup" \
      --argjson selection "$selection" \
      --argjson diagnosis "$diagnosis" \
      '{
        state: $diagnosis.state,
        next_action: "diagnose",
        cleanup: $cleanup,
        selection: $selection,
        diagnosis: $diagnosis
      }'
  else
    jq -n \
      --argjson cleanup "$cleanup" \
      --argjson selection "$selection" \
      '{
        state: "idle",
        next_action: "stop",
        cleanup: $cleanup,
        selection: $selection
      }'
  fi
  exit 0
fi

issue_number="$(jq -r '.issue.number' <<<"$selection")"
prep="$("$SCRIPT_DIR/verify-issue-prep.sh" "$issue_number")"

if [[ "$(jq -r '.ok' <<<"$prep")" != "true" ]]; then
  payload="$(jq -n \
    --argjson cleanup "$cleanup" \
    --argjson selection "$selection" \
    --argjson prep "$prep" \
    '{
      cleanup: $cleanup,
      selection: $selection,
      prep: $prep
    }')"
  diagnosis="$(printf '%s\n' "$payload" | "$SCRIPT_DIR/diagnose-state.sh" || true)"
  jq -n \
    --argjson cleanup "$cleanup" \
    --argjson selection "$selection" \
    --argjson prep "$prep" \
    --argjson diagnosis "$diagnosis" \
    '{
      state: ($diagnosis.state // "blocked"),
      next_action: "diagnose",
      cleanup: $cleanup,
      selection: $selection,
      prep: $prep,
      diagnosis: $diagnosis
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

payload="$(jq -n \
  --argjson cleanup "$cleanup" \
  --argjson selection "$selection" \
  --argjson prep "$prep" \
  --argjson pr "$pr_status" \
  --argjson local "$local_state" \
  --argjson rebase "$rebase_status" \
  --argjson merge "$merge_status" \
  '{
    cleanup: $cleanup,
    selection: $selection,
    prep: $prep,
    pr: $pr,
    local: $local,
    rebase: $rebase,
    merge: $merge
  }')"
diagnosis="$(printf '%s\n' "$payload" | "$SCRIPT_DIR/diagnose-state.sh" || true)"

if [[ -n "$diagnosis" ]]; then
  state="$(jq -r '.state' <<<"$diagnosis")"
  next_action="diagnose"
fi

jq -n \
  --arg state "$state" \
  --arg next_action "$next_action" \
  --argjson cleanup "$cleanup" \
  --argjson selection "$selection" \
  --argjson prep "$prep" \
  --argjson pr "$pr_status" \
  --argjson local "$local_state" \
  --argjson rebase "$rebase_status" \
  --argjson merge "$merge_status" \
  --argjson diagnosis "${diagnosis:-null}" \
  '{
    state: $state,
    next_action: $next_action,
    cleanup: $cleanup,
    selection: $selection,
    prep: $prep,
    pr: $pr,
    local: $local,
    rebase: $rebase,
    merge: $merge,
    diagnosis: $diagnosis
  }'
