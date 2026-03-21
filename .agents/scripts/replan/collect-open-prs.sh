#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=.agents/scripts/replan/common.sh
source "$SCRIPT_DIR/common.sh"

gh pr list --repo "$(canonical_repo)" --state all --limit 200 \
  --json number,title,body,state,isDraft,mergedAt,url,headRefName,baseRefName \
  | jq 'map(. + {
      linked_issue_number: (
        if ((.body // "") | test("^(Closes|Fixes|Resolves) #[0-9]+$"; "mi")) then
          ((.body | capture("(?<keyword>Closes|Fixes|Resolves) #(?<num>[0-9]+)"; "i").num) | tonumber)
        else null end
      )
    })'
