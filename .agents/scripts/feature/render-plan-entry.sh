#!/usr/bin/env bash
set -euo pipefail

CREATED_FILE="${1:-}"
if [[ -z "$CREATED_FILE" || ! -f "$CREATED_FILE" ]]; then
  printf 'usage: %s <created-issue-json-file>\n' "$(basename "$0")" >&2
  exit 1
fi

issue_number="$(jq -r '.number' "$CREATED_FILE")"
title="$(jq -r '.title' "$CREATED_FILE")"
printf -- '- #%s - %s\n' "$issue_number" "$title"
