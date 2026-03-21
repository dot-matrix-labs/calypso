#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=.agents/scripts/replan/common.sh
source "$SCRIPT_DIR/common.sh"

PLAN_FILE="${1:-}"
if [[ -z "$PLAN_FILE" ]]; then
  printf 'usage: %s <plan-json-file>\n' "$(basename "$0")" >&2
  exit 1
fi

"$SCRIPT_DIR/validate-plan-json.sh" "$PLAN_FILE" >/dev/null

updates='[]'

while IFS= read -r encoded; do
  row() { printf '%s' "$encoded" | base64 -d; }
  issue_number="$(row | jq -r '.number')"
  body="$(gh issue view "$issue_number" --repo "$(tasks_repo)" --json body -q .body)"
  dependencies="$(row | jq -r '.dependencies[]? | "#"+(.|tostring)' 2>/dev/null || true)"
  dependents="$(row | jq -r '.dependents[]? | "#"+(.|tostring)' 2>/dev/null || true)"

  dep_block="## Dependencies"$'\n\n'
  if [[ -n "$dependencies" ]]; then
    while IFS= read -r dep; do
      [[ -n "$dep" ]] || continue
      dep_block+="- ${dep}"$'\n'
    done <<<"$dependencies"
  else
    dep_block+="None."$'\n'
  fi

  dependent_block=$'\n'"## Dependents"$'\n\n'
  if [[ -n "$dependents" ]]; then
    while IFS= read -r dep; do
      [[ -n "$dep" ]] || continue
      dependent_block+="- ${dep}"$'\n'
    done <<<"$dependents"
  else
    dependent_block+="None."$'\n'
  fi

  updated_body="$(python3 - "$body" "$dep_block" "$dependent_block" <<'PY'
import re, sys
body, dep_block, dependent_block = sys.argv[1], sys.argv[2], sys.argv[3]

def replace_section(text, heading, replacement):
    pattern = re.compile(rf"^## {re.escape(heading)}\n(?:.*?\n)*(?=^## |\Z)", re.M)
    if pattern.search(text):
        return pattern.sub(replacement if replacement.endswith("\n") else replacement + "\n", text, count=1)
    return text + ("\n" if not text.endswith("\n") else "") + replacement + ("\n" if not replacement.endswith("\n") else "")

body = replace_section(body, "Dependencies", dep_block)
body = replace_section(body, "Dependents", dependent_block)
print(body, end="")
PY
)"

  gh issue edit "$issue_number" --repo "$(tasks_repo)" --body "$updated_body" >/dev/null
  updates="$(jq -c --argjson number "$issue_number" '. + [{number: $number}]' <<<"$updates")"
done < <(jq -r '.ordered_issues[] | @base64' "$PLAN_FILE")

jq -n --argjson updates "$updates" '{ok: true, updates: $updates}'
