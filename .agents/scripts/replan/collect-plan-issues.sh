#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=.agents/scripts/replan/common.sh
source "$SCRIPT_DIR/common.sh"

plan_json="$(require_plan_issue)"
plan_number="$(jq -r '.number' <<<"$plan_json")"
plan_body="$(gh issue view "$plan_number" --repo "$(tasks_repo)" --json body -q .body)"

mapfile -t issue_numbers < <(printf '%s\n' "$plan_body" | extract_issue_refs)

issues='[]'
for issue_number in "${issue_numbers[@]}"; do
  [[ -n "$issue_number" ]] || continue
  payload="$(issue_payload "$issue_number")"
  issues="$(jq -c --argjson issue "$payload" '. + [$issue]' <<<"$issues")"
done

jq -n \
  --argjson plan "$plan_json" \
  --arg plan_body "$plan_body" \
  --argjson issues "$issues" \
  '{
    plan: $plan,
    plan_body: $plan_body,
    issues: $issues
  }'
