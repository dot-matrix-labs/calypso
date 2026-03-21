#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=.agents/scripts/auto/common.sh
source "$SCRIPT_DIR/common.sh"

REPO="$(canonical_repo)"
TASKS_REPO="$(tasks_repo)"
PLAN_JSON="$(gh issue list --repo "$TASKS_REPO" --state open --json number,title,url --jq 'map(select(.title == "Plan")) | .[0]')"

if [[ -z "$PLAN_JSON" || "$PLAN_JSON" == "null" ]]; then
  jq -n '{kind: "none", reason: "plan-not-found"}'
  exit 0
fi

selection_json="$("$SCRIPT_DIR/plan-next-issue.sh")"
selection_kind="$(jq -r '.kind' <<<"$selection_json")"

if [[ "$selection_kind" == "none" ]]; then
  jq -n --arg reason "$(jq -r '.reason' <<<"$selection_json")" '{kind: "none", reason: $reason}'
  exit 0
fi

issue_json="$(jq -c '.issue' <<<"$selection_json")"
pr_number="$(jq -r '.pr.number // empty' <<<"$selection_json")"

if [[ -n "$pr_number" ]]; then
  full_pr_json="$("$SCRIPT_DIR/pr-status.sh" "$pr_number")"
  jq -n \
    --argjson plan "$PLAN_JSON" \
    --argjson issue "$issue_json" \
    --argjson pr "$full_pr_json" \
    '{
      kind: "pr",
      reason: "highest-priority-plan-issue-has-open-pr",
      plan: {
        number: $plan.number,
        url: $plan.url
      },
      issue: $issue,
      pr: $pr
    }'
  exit 0
fi

jq -n \
  --argjson plan "$PLAN_JSON" \
  --argjson issue "$issue_json" \
  '{
    kind: "issue",
    reason: "highest-priority-plan-issue-without-pr",
    plan: {
      number: $plan.number,
      url: $plan.url
    },
    issue: $issue
  }'
exit 0

jq -n '{kind: "none", reason: "no-eligible-issue"}'
