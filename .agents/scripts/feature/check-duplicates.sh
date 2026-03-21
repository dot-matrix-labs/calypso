#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=.agents/scripts/feature/common.sh
source "$SCRIPT_DIR/common.sh"

REQUEST_FILE="${1:-}"
require_json_file "$REQUEST_FILE"

name="$(jq -r '.name' "$REQUEST_FILE")"

gh issue list --repo "$(tasks_repo)" --state all --limit 100 \
  --search "$name in:title" --json number,title,state,url
