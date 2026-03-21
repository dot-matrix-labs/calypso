#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
selection="$("$SCRIPT_DIR/state-summary.sh")"

cat <<EOF
Shared deterministic auto-loop scripts are available under .agents/scripts/auto.
Use them for GitHub state instead of re-deriving obvious facts with model reasoning.

Current auto-loop state snapshot:
$(printf '%s\n' "$selection" | jq .)
EOF
