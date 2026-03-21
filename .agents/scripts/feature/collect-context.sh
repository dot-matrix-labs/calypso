#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=.agents/scripts/feature/common.sh
source "$SCRIPT_DIR/common.sh"

REQUEST_FILE="${1:-}"
require_json_file "$REQUEST_FILE"
normalized_request="$("$SCRIPT_DIR/normalize-feature-request.sh" "$REQUEST_FILE")"
context_validation="$("$SCRIPT_DIR/validate-feature-context.sh" "$REQUEST_FILE")"

plan_json="$(find_plan_issue_json)"
plan_body=""
if [[ -n "$plan_json" && "$plan_json" != "null" ]]; then
  plan_body="$(gh issue view "$(jq -r '.number' <<<"$plan_json")" --repo "$(tasks_repo)" --json body -q .body)"
fi

duplicates="$("$SCRIPT_DIR/check-duplicates.sh" "$REQUEST_FILE")"

jq -n \
  --argjson request "$normalized_request" \
  --argjson plan "${plan_json:-null}" \
  --arg plan_body "$plan_body" \
  --argjson duplicates "$duplicates" \
  --argjson context_validation "$context_validation" \
  --arg prd_path "docs/prd.md" \
  --arg blueprint_path "calypso-blueprint/" \
  '{
    request: $request,
    plan: $plan,
    plan_body: $plan_body,
    duplicates: $duplicates,
    context_validation: $context_validation,
    local_context: {
      prd_path: $prd_path,
      blueprint_path: $blueprint_path
    }
  }'
