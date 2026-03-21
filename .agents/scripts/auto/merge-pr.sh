#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PR_NUMBER="${1:-}"
if [[ -z "$PR_NUMBER" ]]; then
  printf 'usage: %s <pr-number>\n' "$(basename "$0")" >&2
  exit 1
fi

status="$("$SCRIPT_DIR/merge-ready.sh" "$PR_NUMBER")"
if [[ "$(jq -r '.ready' <<<"$status")" != "true" ]]; then
  printf '%s\n' "$status" >&2
  exit 2
fi

gh pr merge "$PR_NUMBER" --merge --delete-branch
