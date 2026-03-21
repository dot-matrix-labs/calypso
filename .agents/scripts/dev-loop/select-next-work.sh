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

OPEN_PRS_JSON="$(gh pr list --repo "$REPO" --state open --json number,title,body,headRefName,isDraft,url)"

for issue_number in "${plan_issue_numbers[@]}"; do
  [[ -n "$issue_number" ]] || continue

  pr_number="$(jq -r --arg issue "$issue_number" '
    map(select((.body | split("\n"))[]? | test("^(Closes|Fixes|Resolves) #" + $issue + "$"; "i"))) | .[0].number // empty
  ' <<<"$OPEN_PRS_JSON")"

  if [[ -z "$pr_number" ]]; then
    continue
  fi

  pr_json="$("$SCRIPT_DIR/pr-status.sh" "$pr_number")"

  jq -n \
    --argjson plan "$PLAN_JSON" \
    --argjson pr "$pr_json" \
    '{
      kind: "pr",
      reason: "open-pr-priority",
      plan: {
        number: $plan.number,
        url: $plan.url
      },
      pr: $pr
    }'
  exit 0
done

"$SCRIPT_DIR/plan-next-issue.sh"
