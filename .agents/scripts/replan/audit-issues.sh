#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=.agents/scripts/replan/common.sh
source "$SCRIPT_DIR/common.sh"

open_issues="$(gh issue list --repo "$(tasks_repo)" --state open --limit 200 --json number,title,body,url)"
violations='[]'
pattern="$(forbidden_plan_metadata_pattern)"

while IFS= read -r encoded; do
  row() { printf '%s' "$encoded" | base64 -d; }
  number="$(row | jq -r '.number')"
  title="$(row | jq -r '.title')"
  body="$(row | jq -r '.body // ""')"
  url="$(row | jq -r '.url')"

  if [[ "$title" == "Plan" ]]; then
    continue
  fi

  issue_reasons='[]'
  for heading in "## Motivation" "## Behaviour" "## Dependencies" "## Scope" "## Acceptance criteria" "## Test plan" "## Stage"; do
    if ! grep -Fq "$heading" <<<"$body"; then
      issue_reasons="$(jq -c --arg r "missing-heading:${heading#\#\# }" '. + [$r]' <<<"$issue_reasons")"
    fi
  done

  acceptance_section="$(printf '%s\n' "$body" | section_body "Acceptance criteria")"
  test_plan_section="$(printf '%s\n' "$body" | section_body "Test plan")"
  if [[ "$(printf '%s\n' "$acceptance_section" | count_checkboxes '^- \[[ xX]\]')" == "0" ]]; then
    issue_reasons="$(jq -c '. + ["missing-acceptance-checkboxes"]' <<<"$issue_reasons")"
  fi
  if [[ "$(printf '%s\n' "$test_plan_section" | count_checkboxes '^- \[[ xX]\]')" == "0" ]]; then
    issue_reasons="$(jq -c '. + ["missing-test-plan-checkboxes"]' <<<"$issue_reasons")"
  fi

  if grep -Piq "$pattern" <<<"$title"; then
    issue_reasons="$(jq -c '. + ["forbidden-plan-metadata-in-title"]' <<<"$issue_reasons")"
  fi

  body_without_dependencies="$(python3 - <<'PY'
import sys
body = sys.stdin.read()
inside = False
for line in body.splitlines():
    if line.startswith("## "):
        inside = (line == "## Dependencies")
        if not inside:
            print(line)
        continue
    if not inside:
        print(line)
PY
<<<"$body")"
  if grep -Piq "$pattern" <<<"$body_without_dependencies"; then
    issue_reasons="$(jq -c '. + ["forbidden-plan-metadata-in-body"]' <<<"$issue_reasons")"
  fi

  if [[ "$issue_reasons" != "[]" ]]; then
    violations="$(jq -c \
      --argjson reasons "$issue_reasons" \
      --argjson number "$number" \
      --arg title "$title" \
      --arg url "$url" \
      '. + [{number: $number, title: $title, url: $url, reasons: $reasons}]' <<<"$violations")"
  fi
done < <(jq -r '.[] | @base64' <<<"$open_issues")

jq -n \
  --argjson violations "$violations" \
  '{
    ok: ($violations | length == 0),
    violations: $violations
  }'
