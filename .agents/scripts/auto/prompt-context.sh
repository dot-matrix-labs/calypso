#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
input="$(cat)"
prompt="$(jq -r '.prompt // ""' <<<"$input")"

if [[ ! "$prompt" =~ (^|[[:space:]/-])(auto|calypso-auto)($|[[:space:]]) ]]; then
  exit 0
fi

selection="$("$SCRIPT_DIR/state-summary.sh")"

printf 'Deterministic auto-loop state from shared repo scripts:\n'
printf '%s\n' "$selection" | jq .
