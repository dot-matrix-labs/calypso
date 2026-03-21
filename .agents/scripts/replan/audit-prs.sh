#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=.agents/scripts/replan/common.sh
source "$SCRIPT_DIR/common.sh"

prs="$("$SCRIPT_DIR/collect-open-prs.sh")"
violations='[]'

while IFS= read -r encoded; do
  row() { printf '%s' "$encoded" | base64 -d; }
  number="$(row | jq -r '.number')"
  title="$(row | jq -r '.title')"
  body="$(row | jq -r '.body // ""')"
  url="$(row | jq -r '.url')"
  state="$(row | jq -r '.state')"
  merged_at="$(row | jq -r '.mergedAt')"
  linked_issue_number="$(row | jq -r '.linked_issue_number // empty')"

  pr_reasons='[]'
  if [[ ! "$body" =~ ^(Closes|Fixes|Resolves)\ #[0-9]+$ ]]; then
    pr_reasons="$(jq -c '. + ["body-must-be-single-closing-reference"]' <<<"$pr_reasons")"
  fi

  if [[ -z "$linked_issue_number" ]]; then
    pr_reasons="$(jq -c '. + ["missing-linked-issue"]' <<<"$pr_reasons")"
  fi

  if [[ "$merged_at" != "null" && -n "$linked_issue_number" ]]; then
    linked_state="$(gh issue view "$linked_issue_number" --repo "$(tasks_repo)" --json state -q .state 2>/dev/null || printf 'MISSING')"
    if [[ "$linked_state" != "CLOSED" ]]; then
      pr_reasons="$(jq -c '. + ["merged-pr-did-not-close-linked-issue"]' <<<"$pr_reasons")"
    fi
  fi

  if [[ "$pr_reasons" != "[]" ]]; then
    violations="$(jq -c \
      --argjson reasons "$pr_reasons" \
      --argjson number "$number" \
      --arg title "$title" \
      --arg url "$url" \
      --arg state "$state" \
      '. + [{number: $number, title: $title, url: $url, state: $state, reasons: $reasons}]' <<<"$violations")"
  fi
done < <(jq -r '.[] | @base64' <<<"$prs")

jq -n \
  --argjson violations "$violations" \
  '{
    ok: ($violations | length == 0),
    violations: $violations
  }'
