#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=.agents/scripts/auto/common.sh
source "$SCRIPT_DIR/../auto/common.sh"

feature_request_file_usage() {
  printf 'expected a JSON file with keys: name, motivation, intended_experience, constraints\n' >&2
}

require_json_file() {
  local path="$1"
  if [[ -z "$path" || ! -f "$path" ]]; then
    feature_request_file_usage
    exit 1
  fi
}

find_plan_issue_json() {
  gh issue list --repo "$(tasks_repo)" --state open --json number,title,url --jq 'map(select(.title == "Plan")) | .[0]'
}

canonicalize_title() {
  local value="$1"
  printf '%s' "$value" \
    | tr '[:upper:]' '[:lower:]' \
    | sed -E 's/^[a-z]+:[[:space:]]*//' \
    | sed -E 's/[^a-z0-9]+/ /g' \
    | sed -E 's/^[[:space:]]+|[[:space:]]+$//g'
}
