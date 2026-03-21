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

if ! grep -Fq "#$issue_number" <<<"$plan_body"; then
  if [[ -n "$plan_body" ]]; then
    plan_body="${plan_body}"$'\n'"$entry"
  else
    plan_body="Planned implementation order for all outstanding features. Work proceeds strictly one issue at a time."$'\n\n'"$entry"
  fi
  gh issue edit "$plan_number" --repo "$(tasks_repo)" --body "$plan_body" >/dev/null
fi

jq -n \
  --argjson plan_issue_number "$plan_number" \
  --argjson issue_number "$issue_number" \
  --arg entry "$entry" \
  '{ok: true, plan_issue_number: $plan_issue_number, issue_number: $issue_number, entry: $entry}'
