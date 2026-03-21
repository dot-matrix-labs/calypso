#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=.agents/scripts/auto/common.sh
source "$SCRIPT_DIR/../auto/common.sh"

find_plan_issue() {
  gh issue list --repo "$(tasks_repo)" --state open --json number,title,url \
    --jq 'map(select(.title == "Plan")) | .[0]'
}

require_plan_issue() {
  local plan_json
  plan_json="$(find_plan_issue)"
  if [[ -z "$plan_json" || "$plan_json" == "null" ]]; then
    printf 'open Plan issue not found\n' >&2
    exit 2
  fi
  printf '%s\n' "$plan_json"
}

issue_payload() {
  local issue_number="$1"
  gh issue view "$issue_number" --repo "$(tasks_repo)" --json number,title,body,state,url
}

pr_payload() {
  local pr_number="$1"
  gh pr view "$pr_number" --repo "$(canonical_repo)" \
    --json number,title,body,state,isDraft,mergedAt,url,headRefName,baseRefName
}

forbidden_plan_metadata_pattern() {
  printf '%s\n' '(?i)(phase|batch|step)[[:space:]]+[0-9]+'
}
