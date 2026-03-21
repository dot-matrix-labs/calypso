#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
selection="$("$SCRIPT_DIR/select-next-work.sh")"

cat <<EOF
Shared deterministic dev-loop scripts are available under .agents/scripts/dev-loop.
Use them for GitHub state instead of re-deriving obvious facts with model reasoning.

Current selection snapshot:
$(printf '%s\n' "$selection" | jq .)
EOF
