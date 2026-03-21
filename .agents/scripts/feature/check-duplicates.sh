#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=.agents/scripts/feature/common.sh
source "$SCRIPT_DIR/common.sh"

REQUEST_FILE="${1:-}"
require_json_file "$REQUEST_FILE"

normalized_request="$("$SCRIPT_DIR/normalize-feature-request.sh" "$REQUEST_FILE")"
name="$(jq -r '.name' <<<"$normalized_request")"
canonical_query="$(canonicalize_title "$name")"

matches="$(gh issue list --repo "$(tasks_repo)" --state all --limit 100 \
  --search "$name in:title" --json number,title,state,body,url)"

jq -n \
  --arg name "$name" \
  --arg canonical_query "$canonical_query" \
  --argjson matches "$matches" \
  '{
    query: $name,
    canonical_query: $canonical_query,
    exact_title_matches: [
      $matches[]
      | (.title | ascii_downcase | sub("^[a-z]+:\\s*"; "") | gsub("[^a-z0-9]+"; " ") | gsub("^\\s+|\\s+$"; "")) as $candidate
      | select($candidate == $canonical_query)
      | {number, title, state, url}
    ],
    likely_overlap_matches: [
      $matches[]
      | (.title | ascii_downcase | sub("^[a-z]+:\\s*"; "") | gsub("[^a-z0-9]+"; " ") | gsub("^\\s+|\\s+$"; "")) as $candidate
      | select($candidate != $canonical_query)
      | {number, title, state, url}
    ],
    prior_closed_candidates: [
      $matches[] | select(.state == "CLOSED")
      | {number, title, state, url}
    ],
    likely_improvement_candidates: [
      $matches[]
      | select(.state == "CLOSED")
      | select(
          ((.title | ascii_downcase) | contains($canonical_query))
          or ((.body // "" | ascii_downcase) | contains($canonical_query))
        )
      | {number, title, state, url}
    ]
  }'
