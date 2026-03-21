#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=.agents/scripts/replan/common.sh
source "$SCRIPT_DIR/common.sh"

plan_payload="$("$SCRIPT_DIR/collect-plan-issues.sh")"
pr_payload="$("$SCRIPT_DIR/collect-open-prs.sh")"
issue_audit="$("$SCRIPT_DIR/audit-issues.sh")"
pr_audit="$("$SCRIPT_DIR/audit-prs.sh")"

jq -n \
  --argjson plan "$plan_payload" \
  --argjson prs "$pr_payload" \
  --argjson issue_audit "$issue_audit" \
  --argjson pr_audit "$pr_audit" \
  '{
    plan: $plan.plan,
    issues: $plan.issues,
    open_prs: $prs,
    audits: {
      issues: $issue_audit,
      prs: $pr_audit
    }
  }'
