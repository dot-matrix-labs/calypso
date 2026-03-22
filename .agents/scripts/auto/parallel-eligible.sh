#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=.agents/scripts/auto/common.sh
source "$SCRIPT_DIR/common.sh"

TASKS_REPO="$(tasks_repo)"
PLAN_JSON="$(gh issue list --repo "$TASKS_REPO" --state open --json number,title --jq 'map(select(.title == "Plan")) | .[0]')"

if [[ -z "$PLAN_JSON" || "$PLAN_JSON" == "null" ]]; then
  jq -n '{eligible: [], reason: "plan-not-found"}'
  exit 0
fi

PLAN_NUMBER="$(jq -r '.number' <<<"$PLAN_JSON")"
PLAN_BODY="$(gh issue view "$PLAN_NUMBER" --repo "$TASKS_REPO" --json body -q .body)"

mapfile -t plan_issues < <(printf '%s\n' "$PLAN_BODY" | extract_issue_refs)

if [[ "${#plan_issues[@]}" -eq 0 ]]; then
  jq -n '{eligible: [], reason: "no-plan-issues"}'
  exit 0
fi

# Build state map: issue number -> state
declare -A issue_states
for issue_number in "${plan_issues[@]}"; do
  [[ -n "$issue_number" ]] || continue
  state="$(gh issue view "$issue_number" --repo "$TASKS_REPO" --json state -q .state)"
  issue_states["$issue_number"]="$state"
done

# Find the currently selected (first OPEN) issue
selected=""
for issue_number in "${plan_issues[@]}"; do
  [[ -n "$issue_number" ]] || continue
  if [[ "${issue_states[$issue_number]}" == "OPEN" ]]; then
    selected="$issue_number"
    break
  fi
done

if [[ -z "$selected" ]]; then
  jq -n '{eligible: [], reason: "no-open-issues"}'
  exit 0
fi

# For each issue after the selected one, check if all its dependencies are CLOSED
eligible='[]'
past_selected=false
for issue_number in "${plan_issues[@]}"; do
  [[ -n "$issue_number" ]] || continue

  if [[ "$issue_number" == "$selected" ]]; then
    past_selected=true
    continue
  fi

  [[ "$past_selected" == "true" ]] || continue
  [[ "${issue_states[$issue_number]}" == "OPEN" ]] || continue

  # Get the issue's dependencies section
  issue_body="$(gh issue view "$issue_number" --repo "$TASKS_REPO" --json body -q .body)"
  dep_section="$(printf '%s\n' "$issue_body" | section_body "Dependencies")"
  mapfile -t deps < <(printf '%s\n' "$dep_section" | extract_issue_refs)

  all_deps_closed=true
  for dep in "${deps[@]}"; do
    [[ -n "$dep" ]] || continue
    dep_state="${issue_states[$dep]:-}"
    if [[ -z "$dep_state" ]]; then
      dep_state="$(gh issue view "$dep" --repo "$TASKS_REPO" --json state -q .state)"
    fi
    if [[ "$dep_state" != "CLOSED" ]]; then
      all_deps_closed=false
      break
    fi
  done

  if [[ "$all_deps_closed" == "true" ]]; then
    issue_title="$(gh issue view "$issue_number" --repo "$TASKS_REPO" --json title -q .title)"
    eligible="$(jq -c --argjson num "$issue_number" --arg title "$issue_title" \
      '. + [{number: $num, title: $title}]' <<<"$eligible")"
  fi
done

jq -n --argjson selected "$selected" --argjson eligible "$eligible" '{
  selected: $selected,
  eligible: $eligible
}'
