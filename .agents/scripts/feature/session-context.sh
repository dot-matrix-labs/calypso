#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
plan_json="$("$SCRIPT_DIR/../replan/collect-plan-issues.sh" 2>/dev/null || printf '{}')"

cat <<EOF
Shared deterministic feature scripts are available under .agents/scripts/feature.
Use them for request validation, duplicates, issue rendering, and Plan updates.

Current feature context snapshot:
$(printf '%s\n' "$plan_json" | jq '{plan: .plan, planned_issue_count: (.issues | length // 0)}' 2>/dev/null || true)
EOF
