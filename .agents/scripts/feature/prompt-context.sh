#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
input="$(cat)"
prompt="$(jq -r '.prompt // ""' <<<"$input")"

if [[ ! "$prompt" =~ (^|[[:space:]/-])(feature|calypso-feature)($|[[:space:]]) ]]; then
  exit 0
fi

plan_json="$("$SCRIPT_DIR/../replan/collect-plan-issues.sh" 2>/dev/null || printf '{}')"

printf 'Deterministic feature state from shared repo scripts:\n'
printf '%s\n' "$plan_json" | jq '{plan: .plan, planned_issue_count: (.issues | length // 0)}' 2>/dev/null || true
