#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=.agents/scripts/replan/common.sh
source "$SCRIPT_DIR/common.sh"

normalize_title() {
  local title="$1"
  python3 - "$title" <<'PY'
import re, sys
title = sys.argv[1]
title = re.sub(r'(?i)\b(?:phase|batch|step)\s+\d+\b[: -]*', '', title)
title = re.sub(r'\s{2,}', ' ', title).strip(' -:')
print(title)
PY
}

normalize_body() {
  python3 - <<'PY'
import re, sys
body = sys.stdin.read()

required_sections = {
    "Motivation": "TBD.",
    "Behaviour": "TBD.",
    "Dependencies": "None.",
    "Scope": "TBD.",
    "Acceptance criteria": "- [ ] TBD",
    "Test plan": "- [ ] TBD",
    "Stage": "**Current:** Specified",
}

body = re.sub(r'(?im)^(?:phase|batch|step)\s+\d+[: -].*$\n?', '', body)

def section_present(name):
    return re.search(rf'^## {re.escape(name)}\s*$', body, re.M) is not None

parts = [body.rstrip()]
for name, default in required_sections.items():
    if not section_present(name):
      parts.append(f"## {name}\n\n{default}")

text = "\n\n".join([p for p in parts if p])

def ensure_checkbox(section):
    pattern = re.compile(rf'(^## {re.escape(section)}\n)(.*?)(?=^## |\Z)', re.M | re.S)
    match = pattern.search(text)
    if not match:
        return text
    content = match.group(2).strip()
    if re.search(r'(?m)^- \[[ xX]\] ', content):
        return text
    replacement = f"{match.group(1)}\n- [ ] TBD\n\n"
    return text[:match.start()] + replacement + text[match.end():]

text = ensure_checkbox("Acceptance criteria")
text = ensure_checkbox("Test plan")
print(text.rstrip() + "\n", end="")
PY
}

normalize_one_issue() {
  local issue_number="$1"
  local payload title body url normalized_title normalized_body

  payload="$(issue_payload "$issue_number")"
  title="$(jq -r '.title' <<<"$payload")"
  body="$(jq -r '.body // ""' <<<"$payload")"
  url="$(jq -r '.url' <<<"$payload")"

  if [[ "$title" == "Plan" ]]; then
    jq -n --argjson number "$issue_number" --arg url "$url" '{ok: true, number: $number, url: $url, skipped: "plan-issue"}'
    return 0
  fi

  normalized_title="$(normalize_title "$title")"
  normalized_body="$(printf '%s' "$body" | normalize_body)"

  if [[ "$normalized_title" != "$title" || "$normalized_body" != "$body" ]]; then
    gh issue edit "$issue_number" --repo "$(tasks_repo)" --title "$normalized_title" --body "$normalized_body" >/dev/null
  fi

  jq -n \
    --argjson number "$issue_number" \
    --arg title "$normalized_title" \
    --arg url "$url" \
    '{ok: true, number: $number, title: $title, url: $url}'
}

if [[ $# -gt 0 ]]; then
  normalize_one_issue "$1"
  exit 0
fi

issues="$(gh issue list --repo "$(tasks_repo)" --state open --limit 200 --json number,title)"
results='[]'
while IFS= read -r issue_number; do
  [[ -n "$issue_number" ]] || continue
  result="$(normalize_one_issue "$issue_number")"
  results="$(jq -c --argjson result "$result" '. + [$result]' <<<"$results")"
done < <(jq -r '.[] | select(.title != "Plan") | .number' <<<"$issues")

jq -n --argjson results "$results" '{ok: true, results: $results}'
