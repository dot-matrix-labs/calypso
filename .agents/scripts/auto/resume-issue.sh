#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=.agents/scripts/auto/common.sh
source "$SCRIPT_DIR/common.sh"

ISSUE_NUMBER="${1:-}"
if [[ -z "$ISSUE_NUMBER" ]]; then
  printf 'usage: %s <issue-number>\n' "$(basename "$0")" >&2
  exit 1
fi

prep="$("$SCRIPT_DIR/ensure-issue-worktree.sh" "$ISSUE_NUMBER")"
reconciled="$("$SCRIPT_DIR/reconcile-local-state.sh" "$ISSUE_NUMBER")"

jq -n \
  --argjson prep "$prep" \
  --argjson reconciled "$reconciled" \
  '{
    ok: true,
    prep: $prep,
    reconciled: $reconciled
  }'
