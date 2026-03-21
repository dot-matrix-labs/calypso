#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=.agents/scripts/dev-loop/common.sh
source "$SCRIPT_DIR/common.sh"

REPO="$(canonical_repo)"
TASKS_REPO="$(tasks_repo)"
PLAN_JSON="$(gh issue list --repo "$TASKS_REPO" --state open --json number,title,url --jq 'map(select(.title == "Plan")) | .[0]')"

if [[ -z "$PLAN_JSON" || "$PLAN_JSON" == "null" ]]; then
  jq -n '{kind: "none", reason: "plan-not-found"}'
  exit 0
fi

PLAN_NUMBER="$(jq -r '.number' <<<"$PLAN_JSON")"
PLAN_BODY="$(gh issue view "$PLAN_NUMBER" --repo "$TASKS_REPO" --json body -q .body)"
mapfile -t plan_issue_numbers < <(printf '%s\n' "$PLAN_BODY" | extract_issue_refs)

for issue_number in "${plan_issue_numbers[@]}"; do
  [[ -n "$issue_number" ]] || continue
  issue_json="$("$SCRIPT_DIR/issue-status.sh" "$issue_number")"
  issue_state="$(jq -r '.state' <<<"$issue_json")"
  dependencies_closed="$(jq -r '.dependencies_closed' <<<"$issue_json")"

  if [[ "$issue_state" != "OPEN" || "$dependencies_closed" != "true" ]]; then
    continue
  fi

  pr_json="$(gh pr list --repo "$REPO" --state open --json number,title,body,headRefName,isDraft,url \
    --jq 'map(select((.body | split("\n"))[]? | test("^(Closes|Fixes|Resolves) #'$issue_number'$"; "i"))) | .[0]')"

  if [[ -n "$pr_json" && "$pr_json" != "null" ]]; then
    full_pr_json="$("$SCRIPT_DIR/pr-status.sh" "$(jq -r '.number' <<<"$pr_json")")"
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
done

jq -n '{kind: "none", reason: "no-eligible-issue"}'
