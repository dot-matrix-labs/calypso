#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=.agents/scripts/replan/common.sh
source "$SCRIPT_DIR/common.sh"

PLAN_FILE="${1:-}"
if [[ -z "$PLAN_FILE" ]]; then
  printf 'usage: %s <plan-json-file>\n' "$(basename "$0")" >&2
  exit 1
fi

"$SCRIPT_DIR/validate-plan-json.sh" "$PLAN_FILE" >/dev/null

plan_issue_number="$(jq -r '.plan_issue_number' "$PLAN_FILE")"
body="$(jq -r '
  [
    "Planned implementation order for all outstanding features. Work proceeds strictly one issue at a time.",
    "",
    "> Last replanned: " + (now | todateiso8601 | split("T")[0]),
    "",
    (.ordered_issues[] | "- #" + (.number|tostring) + " - " + .title + " [risk: " + (.risk|tostring) + "]")
  ] | join("\n")
' "$PLAN_FILE")"

gh issue edit "$plan_issue_number" --repo "$(tasks_repo)" --body "$body" >/dev/null

jq -n --argjson plan_issue_number "$plan_issue_number" --arg body "$body" '{
  ok: true,
  plan_issue_number: $plan_issue_number,
  body: $body
}'
