#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=.agents/scripts/auto/common.sh
source "$SCRIPT_DIR/common.sh"

BRANCH_NAME="${1:-}"
if [[ -z "$BRANCH_NAME" ]]; then
  printf 'usage: %s <branch-name>\n' "$(basename "$0")" >&2
  exit 1
fi

git fetch origin main "$BRANCH_NAME" >/dev/null 2>&1 || git fetch origin main >/dev/null 2>&1

local_exists=false
remote_exists=false
tracking=false

if git show-ref --verify --quiet "refs/heads/$BRANCH_NAME"; then
  local_exists=true
fi

if git ls-remote --exit-code --heads origin "$BRANCH_NAME" >/dev/null 2>&1; then
  remote_exists=true
fi

ahead=0
behind=0
behind_main=0
ahead_main=0

if [[ "$local_exists" == "true" && "$remote_exists" == "true" ]]; then
  if git rev-parse --abbrev-ref "${BRANCH_NAME}@{upstream}" >/dev/null 2>&1; then
    tracking=true
  fi
  ahead="$(git rev-list --count "origin/$BRANCH_NAME..$BRANCH_NAME" 2>/dev/null || printf '0')"
  behind="$(git rev-list --count "$BRANCH_NAME..origin/$BRANCH_NAME" 2>/dev/null || printf '0')"
fi

if [[ "$local_exists" == "true" ]]; then
  ahead_main="$(git rev-list --count "origin/main..$BRANCH_NAME" 2>/dev/null || printf '0')"
  behind_main="$(git rev-list --count "$BRANCH_NAME..origin/main" 2>/dev/null || printf '0')"
fi

jq -n \
  --arg branch "$BRANCH_NAME" \
  --argjson local_exists "$local_exists" \
  --argjson remote_exists "$remote_exists" \
  --argjson tracking "$tracking" \
  --argjson ahead "$ahead" \
  --argjson behind "$behind" \
  --argjson ahead_main "$ahead_main" \
  --argjson behind_main "$behind_main" \
  '{
    branch: $branch,
    local_exists: $local_exists,
    remote_exists: $remote_exists,
    tracking: $tracking,
    remote_sync: {
      ahead: $ahead,
      behind: $behind
    },
    main_sync: {
      ahead: $ahead_main,
      behind: $behind_main,
      branched_from_latest_main: ($behind_main == 0)
    }
  }'
