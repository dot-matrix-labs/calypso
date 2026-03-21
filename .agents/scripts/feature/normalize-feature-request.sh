#!/usr/bin/env bash
set -euo pipefail

REQUEST_FILE="${1:-}"
if [[ -z "$REQUEST_FILE" || ! -f "$REQUEST_FILE" ]]; then
  printf 'usage: %s <feature-json-file>\n' "$(basename "$0")" >&2
  exit 1
fi

jq '
  .name = ((.name // "") | tostring | gsub("^\\s+|\\s+$"; ""))
  | .motivation = ((.motivation // "") | tostring | gsub("^\\s+|\\s+$"; ""))
  | .intended_experience = ((.intended_experience // "") | tostring | gsub("^\\s+|\\s+$"; ""))
  | .constraints = (
      if (.constraints | type) == "string" then
        [(.constraints | gsub("^\\s+|\\s+$"; ""))]
      elif (.constraints | type) == "array" then
        [.constraints[] | tostring | gsub("^\\s+|\\s+$"; "") | select(length > 0)]
      else
        []
      end
    )
  | if (.name | ascii_downcase) == "tbd" then .name = "" else . end
  | if (.motivation | ascii_downcase) == "tbd" then .motivation = "" else . end
  | if (.intended_experience | ascii_downcase) == "tbd" then .intended_experience = "" else . end
  | .constraints = [.constraints[] | select((ascii_downcase) != "tbd")]
' "$REQUEST_FILE"
