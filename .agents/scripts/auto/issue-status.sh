#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=.agents/scripts/auto/common.sh
source "$SCRIPT_DIR/common.sh"

ISSUE_NUMBER="${1:-}"
if [[ -z "$ISSUE_NUMBER" ]]; then
  printf 'usage: %s <issue-number>\n' "$(basename "$0")" >&2
  exit 1
fi

REPO="$(tasks_repo)"
ISSUE_JSON="$(gh issue view "$ISSUE_NUMBER" --repo "$REPO" --json number,title,url,state,body)"
BODY="$(jq -r '.body' <<<"$ISSUE_JSON")"

acceptance_section="$(printf '%s\n' "$BODY" | section_body "Acceptance criteria")"
test_plan_section="$(printf '%s\n' "$BODY" | section_body "Test plan")"
dependencies_section="$(printf '%s\n' "$BODY" | section_body "Dependencies")"

acceptance_total="$(printf '%s\n' "$acceptance_section" | count_checkboxes '^- \[[ xX]\]')"
acceptance_unchecked="$(printf '%s\n' "$acceptance_section" | count_checkboxes '^- \[ \]')"
test_total="$(printf '%s\n' "$test_plan_section" | count_checkboxes '^- \[[ xX]\]')"
test_unchecked="$(printf '%s\n' "$test_plan_section" | count_checkboxes '^- \[ \]')"

dep_numbers=()
while IFS= read -r dep; do
  [[ -n "$dep" ]] || continue
  dep_numbers+=("$dep")
done < <(printf '%s\n' "$dependencies_section" | extract_issue_refs)

deps_json='[]'
dependencies_closed=true
if [[ "${#dep_numbers[@]}" -gt 0 ]]; then
  dep_payload='[]'
  for dep in "${dep_numbers[@]}"; do
    dep_view="$(gh issue view "$dep" --repo "$REPO" --json number,title,state,url)"
    dep_payload="$(jq -c --argjson dep "$dep_view" '. + [$dep]' <<<"$dep_payload")"
    if [[ "$(jq -r '.state' <<<"$dep_view")" != "CLOSED" ]]; then
      dependencies_closed=false
    fi
  done
  deps_json="$dep_payload"
fi

jq -n \
  --argjson issue "$ISSUE_JSON" \
  --argjson dependencies "$deps_json" \
  --argjson acceptance_total "$acceptance_total" \
  --argjson acceptance_unchecked "$acceptance_unchecked" \
  --argjson test_total "$test_total" \
  --argjson test_unchecked "$test_unchecked" \
  --argjson dependencies_closed "$dependencies_closed" \
  '
  {
    number: $issue.number,
    title: $issue.title,
    url: $issue.url,
    state: $issue.state,
    acceptance: {
      total: $acceptance_total,
      unchecked: $acceptance_unchecked,
      complete: ($acceptance_total > 0 and $acceptance_unchecked == 0)
    },
    test_plan: {
      total: $test_total,
      unchecked: $test_unchecked,
      complete: ($test_total > 0 and $test_unchecked == 0)
    },
    checklist: {
      total: ($acceptance_total + $test_total),
      unchecked: ($acceptance_unchecked + $test_unchecked),
      complete: (($acceptance_total + $test_total) > 0 and ($acceptance_unchecked + $test_unchecked) == 0)
    },
    dependencies: $dependencies,
    dependencies_closed: $dependencies_closed
  }'
