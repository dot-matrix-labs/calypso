#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=.agents/scripts/feature/common.sh
source "$SCRIPT_DIR/common.sh"

REQUEST_FILE="${1:-}"
require_json_file "$REQUEST_FILE"

reasons='[]'

if [[ ! -f "docs/prd.md" ]]; then
  reasons="$(jq -c '. + ["missing-prd"]' <<<"$reasons")"
fi

if [[ ! -d "calypso-blueprint" ]]; then
  reasons="$(jq -c '. + ["missing-blueprint-directory"]' <<<"$reasons")"
fi

plan_json="$(find_plan_issue_json)"
if [[ -z "$plan_json" || "$plan_json" == "null" ]]; then
  reasons="$(jq -c '. + ["missing-plan-issue"]' <<<"$reasons")"
fi

if ! "$SCRIPT_DIR/check-duplicates.sh" "$REQUEST_FILE" >/dev/null 2>&1; then
  reasons="$(jq -c '. + ["duplicate-check-failed"]' <<<"$reasons")"
fi

jq -n --argjson reasons "$reasons" '{
  ok: ($reasons | length == 0),
  reasons: $reasons
}'
