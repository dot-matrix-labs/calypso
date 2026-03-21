#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=.agents/scripts/auto/common.sh
source "$SCRIPT_DIR/common.sh"

REPO="$(tasks_repo)"
PLAN_JSON="$(gh issue list --repo "$REPO" --state open --json number,title,url --jq 'map(select(.title == "Plan")) | .[0]')"

if [[ -z "$PLAN_JSON" || "$PLAN_JSON" == "null" ]]; then
  jq -n '{kind: "none", reason: "plan-not-found"}'
  exit 0
fi

PLAN_BODY="$(gh issue view "$(jq -r '.number' <<<"$PLAN_JSON")" --repo "$REPO" --json body -q .body)"

mapfile -t plan_issue_numbers < <(printf '%s\n' "$PLAN_BODY" | extract_issue_refs)

for issue_number in "${plan_issue_numbers[@]}"; do
  [[ -n "$issue_number" ]] || continue

  issue_json="$("$SCRIPT_DIR/issue-status.sh" "$issue_number")"
  issue_state="$(jq -r '.state' <<<"$issue_json")"
  dependencies_closed="$(jq -r '.dependencies_closed' <<<"$issue_json")"

  if [[ "$issue_state" != "OPEN" || "$dependencies_closed" != "true" ]]; then
    continue
  fi

  pr_json="$(gh pr list --repo "$(canonical_repo)" --state open --json number,title,body,headRefName,isDraft,url \
    --jq 'map(select((.body | split("\n"))[]? | test("^(Closes|Fixes|Resolves) #'$issue_number'$"; "i"))) | .[0]')"

  jq -n --argjson issue "$issue_json" --argjson pr "${pr_json:-null}" '{
    kind: "issue",
    reason: "next-plan-issue",
    issue: $issue,
    pr: (if $pr == null then null else {
      number: $pr.number,
      title: $pr.title,
      url: $pr.url,
      head_ref: $pr.headRefName,
      is_draft: $pr.isDraft
    } end)
  }'
  exit 0
done

jq -n '{kind: "none", reason: "no-eligible-issue"}'
