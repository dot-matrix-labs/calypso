#!/usr/bin/env bash
set -euo pipefail

PR_NUMBER="${1:-}"
if [[ -z "$PR_NUMBER" ]]; then
  printf 'usage: %s <pr-number>\n' "$(basename "$0")" >&2
  exit 1
fi

gh pr ready "$PR_NUMBER"
