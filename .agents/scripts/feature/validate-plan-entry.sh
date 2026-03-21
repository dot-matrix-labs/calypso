#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=.agents/scripts/feature/common.sh
source "$SCRIPT_DIR/common.sh"

CREATED_FILE="${1:-}"
if [[ -z "$CREATED_FILE" || ! -f "$CREATED_FILE" ]]; then
  printf 'usage: %s <created-issue-json-file>\n' "$(basename "$0")" >&2
  exit 1
fi

plan_json="$(find_plan_issue_json)"
if [[ -z "$plan_json" || "$plan_json" == "null" ]]; then
  printf 'open Plan issue not found\n' >&2
  exit 2
fi

plan_number="$(jq -r '.number' <<<"$plan_json")"
plan_body="$(gh issue view "$plan_number" --repo "$(tasks_repo)" --json body -q .body)"
issue_number="$(jq -r '.number' "$CREATED_FILE")"
entry="$("$SCRIPT_DIR/render-plan-entry.sh" "$CREATED_FILE")"
count="$(grep -cF "#$issue_number" <<<"$plan_body" || true)"
reasons='[]'

if [[ "$count" != "1" ]]; then
  reasons="$(jq -c '. + ["plan-entry-count-not-one"]' <<<"$reasons")"
fi
if grep -Eq '^- \[[ xX]\]' <<<"$plan_body"; then
  reasons="$(jq -c '. + ["plan-contains-checkboxes"]' <<<"$reasons")"
fi
if grep -Eiq '\b(phase|batch|step)[[:space:]]+[0-9]+' <<<"$plan_body"; then
  reasons="$(jq -c '. + ["plan-contains-order-metadata"]' <<<"$reasons")"
fi
if ! grep -Fq "$entry" <<<"$plan_body"; then
  reasons="$(jq -c '. + ["plan-entry-format-mismatch"]' <<<"$reasons")"
fi

jq -n \
  --argjson plan_issue_number "$plan_number" \
  --argjson issue_number "$issue_number" \
  --arg entry "$entry" \
  --argjson reasons "$reasons" \
  '{
    ok: ($reasons | length == 0),
    plan_issue_number: $plan_issue_number,
    issue_number: $issue_number,
    entry: $entry,
    reasons: $reasons
  }'
