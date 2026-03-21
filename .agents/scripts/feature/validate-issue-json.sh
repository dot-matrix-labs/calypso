#!/usr/bin/env bash
set -euo pipefail

ISSUE_FILE="${1:-}"
if [[ -z "$ISSUE_FILE" || ! -f "$ISSUE_FILE" ]]; then
  printf 'usage: %s <issue-json-file>\n' "$(basename "$0")" >&2
  exit 1
fi

jq -e '
  (.title | type == "string" and length > 0)
  and (.motivation | type == "string" and length > 0)
  and (.behaviour | type == "string" and length > 0)
  and (.scope.in | type == "array" and length > 0)
  and (.scope.out | type == "array" and length > 0)
  and (.acceptance_criteria | type == "array" and length >= 2)
  and (.test_plan | type == "array" and length >= 2)
  and (.stage == "Specified")
' "$ISSUE_FILE" >/dev/null || {
  printf 'invalid issue json: missing required feature issue fields\n' >&2
  exit 2
}

jq -n --arg file "$ISSUE_FILE" '{ok: true, file: $file}'
