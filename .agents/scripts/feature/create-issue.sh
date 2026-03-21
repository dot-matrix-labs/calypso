#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=.agents/scripts/feature/common.sh
source "$SCRIPT_DIR/common.sh"

ISSUE_FILE="${1:-}"
if [[ -z "$ISSUE_FILE" || ! -f "$ISSUE_FILE" ]]; then
  printf 'usage: %s <issue-json-file>\n' "$(basename "$0")" >&2
  exit 1
fi

"$SCRIPT_DIR/validate-issue-json.sh" "$ISSUE_FILE" >/dev/null
title="$(jq -r '.title' "$ISSUE_FILE")"
body="$("$SCRIPT_DIR/render-issue-body.sh" "$ISSUE_FILE")"
url="$(gh issue create --repo "$(tasks_repo)" --title "$title" --body "$body")"
issue_number="$(grep -oE '[0-9]+$' <<<"$url")"

jq -n \
  --argjson number "$issue_number" \
  --arg title "$title" \
  --arg body "$body" \
  --arg url "$url" \
  '{ok: true, number: $number, title: $title, body: $body, url: $url}'
