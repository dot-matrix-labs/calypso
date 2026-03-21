#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=.agents/scripts/feature/common.sh
source "$SCRIPT_DIR/common.sh"

REQUEST_FILE="${1:-}"
require_json_file "$REQUEST_FILE"

jq -e '
  (.name | type == "string" and length > 0)
  and (.motivation | type == "string" and length > 0)
  and (.intended_experience | type == "string" and length > 0)
  and (
    (.constraints | type == "string" and length > 0)
    or (.constraints | type == "array" and length > 0)
  )
' "$REQUEST_FILE" >/dev/null || {
  printf 'invalid feature request: name, motivation, intended_experience, and constraints are required\n' >&2
  exit 2
}

jq -n --arg file "$REQUEST_FILE" '{ok: true, file: $file}'
