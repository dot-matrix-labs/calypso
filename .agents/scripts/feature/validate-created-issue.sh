#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=.agents/scripts/feature/common.sh
source "$SCRIPT_DIR/common.sh"

CREATED_FILE="${1:-}"
if [[ -z "$CREATED_FILE" || ! -f "$CREATED_FILE" ]]; then
  printf 'usage: %s <created-issue-json-file>\n' "$(basename "$0")" >&2
  exit 1
fi

issue_number="$(jq -r '.number' "$CREATED_FILE")"
payload="$(issue_payload "$issue_number")"
title="$(jq -r '.title' <<<"$payload")"
body="$(jq -r '.body // ""' <<<"$payload")"
reasons='[]'

for heading in "## Motivation" "## Behaviour" "## Dependencies" "## Scope" "## Acceptance criteria" "## Test plan" "## Stage"; do
  if ! grep -Fq "$heading" <<<"$body"; then
    reasons="$(jq -c --arg r "missing-heading:${heading#\#\# }" '. + [$r]' <<<"$reasons")"
  fi
done

if [[ "$(printf '%s\n' "$body" | section_body "Acceptance criteria" | count_checkboxes '^- \[[ xX]\]')" == "0" ]]; then
  reasons="$(jq -c '. + ["missing-acceptance-checkboxes"]' <<<"$reasons")"
fi
if [[ "$(printf '%s\n' "$body" | section_body "Test plan" | count_checkboxes '^- \[[ xX]\]')" == "0" ]]; then
  reasons="$(jq -c '. + ["missing-test-plan-checkboxes"]' <<<"$reasons")"
fi
if ! grep -Fq "**Current:** Specified" <<<"$body"; then
  reasons="$(jq -c '. + ["stage-not-specified"]' <<<"$reasons")"
fi

jq -n \
  --argjson number "$issue_number" \
  --arg title "$title" \
  --argjson reasons "$reasons" \
  '{
    ok: ($reasons | length == 0),
    number: $number,
    title: $title,
    reasons: $reasons
  }'
