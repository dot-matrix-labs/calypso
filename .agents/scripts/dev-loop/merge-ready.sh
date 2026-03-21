#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=.agents/scripts/dev-loop/common.sh
source "$SCRIPT_DIR/common.sh"

TARGET="${1:-}"
PR_JSON="$("$SCRIPT_DIR/pr-status.sh" "$TARGET")"

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

jq -n \
  --argjson pr "$PR_JSON" \
  --argjson ready "$ready" \
  --argjson reasons "$reasons" \
  '{
    ready: $ready,
    reasons: $reasons,
    pr: $pr
  }'
