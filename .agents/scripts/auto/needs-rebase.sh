#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=.agents/scripts/auto/common.sh
source "$SCRIPT_DIR/common.sh"

TARGET="${1:-}"
if [[ -z "$TARGET" ]]; then
  printf 'usage: %s <pr-number-or-branch>\n' "$(basename "$0")" >&2
  exit 1
fi

branch_name="$TARGET"
if [[ "$TARGET" =~ ^[0-9]+$ ]]; then
  branch_name="$(gh pr view "$TARGET" --repo "$(canonical_repo)" --json headRefName -q .headRefName)"
fi

branch_status="$("$SCRIPT_DIR/remote-branch-status.sh" "$branch_name")"
behind_main="$(jq -r '.main_sync.behind' <<<"$branch_status")"
needs_rebase=false
reasons='[]'

if [[ "$behind_main" != "0" ]]; then
  needs_rebase=true
  reasons="$(jq -c '. + ["branch-behind-origin-main"]' <<<"$reasons")"
fi

jq -n \
  --arg branch "$branch_name" \
  --argjson branch_status "$branch_status" \
  --argjson needs_rebase "$needs_rebase" \
  --argjson reasons "$reasons" \
  '{
    branch: $branch,
    needs_rebase: $needs_rebase,
    reasons: $reasons,
    branch_status: $branch_status
  }'
