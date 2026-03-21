#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=.agents/scripts/auto/common.sh
source "$SCRIPT_DIR/common.sh"

TARGET="${1:-}"
REPO="$(canonical_repo)"

if [[ -z "$TARGET" ]]; then
  PR_JSON="$(gh pr view --repo "$REPO" --json number,title,url,body,state,isDraft,mergeable,mergeStateStatus,headRefName,baseRefName,mergedAt)"
  CHECKS_JSON="$(gh pr checks --repo "$REPO" --json name,state,bucket,workflow,link)"
else
  PR_JSON="$(gh pr view "$TARGET" --repo "$REPO" --json number,title,url,body,state,isDraft,mergeable,mergeStateStatus,headRefName,baseRefName,mergedAt)"
  CHECKS_JSON="$(gh pr checks "$TARGET" --repo "$REPO" --json name,state,bucket,workflow,link)"
fi

linked_issue_number="$(jq -r '.body' <<<"$PR_JSON" | extract_closing_issue_number || true)"
if [[ -n "$linked_issue_number" ]]; then
  ISSUE_JSON="$("$SCRIPT_DIR/issue-status.sh" "$linked_issue_number")"
else
  ISSUE_JSON='null'
fi

jq -n \
  --argjson pr "$PR_JSON" \
  --argjson checks "$CHECKS_JSON" \
  --arg linked_issue_number "${linked_issue_number:-}" \
  --argjson issue "$ISSUE_JSON" \
  '
  {
    number: $pr.number,
    title: $pr.title,
    url: $pr.url,
    head_ref: $pr.headRefName,
    base_ref: $pr.baseRefName,
    state: $pr.state,
    is_draft: $pr.isDraft,
    merged: ($pr.mergedAt != null),
    mergeable: $pr.mergeable,
    merge_state_status: $pr.mergeStateStatus,
    linked_issue_number: (if $linked_issue_number == "" then null else ($linked_issue_number | tonumber) end),
    checks: {
      total: ($checks | length),
      passing: ($checks | map(select(.bucket == "pass")) | length),
      failing: ($checks | map(select(.bucket == "fail" or .bucket == "cancel")) | length),
      pending: ($checks | map(select(.bucket == "pending")) | length),
      details: $checks,
      all_green: (($checks | length) > 0 and (($checks | map(select(.bucket == "fail" or .bucket == "cancel")) | length) == 0) and (($checks | map(select(.bucket == "pending")) | length) == 0))
    },
    issue: $issue
  }'
