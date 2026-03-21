#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=.agents/scripts/replan/common.sh
source "$SCRIPT_DIR/common.sh"

extract_issue_from_branch() {
  local branch_name="$1"
  if [[ "$branch_name" =~ /([0-9]+)- ]]; then
    printf '%s\n' "${BASH_REMATCH[1]}"
  fi
}

normalize_one_pr() {
  local pr_number="$1"
  local pr_json body branch linked_issue_number inferred_issue_number normalized_body

  pr_json="$(pr_payload "$pr_number")"
  body="$(jq -r '.body // ""' <<<"$pr_json")"
  branch="$(jq -r '.headRefName' <<<"$pr_json")"
  linked_issue_number="$(printf '%s\n' "$body" | extract_closing_issue_number || true)"

  inferred_issue_number="$linked_issue_number"
  if [[ -z "$inferred_issue_number" ]]; then
    mapfile -t refs < <(printf '%s\n' "$body" | extract_issue_refs || true)
    if [[ "${#refs[@]}" == "1" ]]; then
      inferred_issue_number="${refs[0]}"
    fi
  fi
  if [[ -z "$inferred_issue_number" ]]; then
    inferred_issue_number="$(extract_issue_from_branch "$branch" || true)"
  fi

  if [[ -z "$inferred_issue_number" ]]; then
    jq -n --argjson number "$pr_number" '{ok: false, number: $number, reason: "could-not-infer-issue"}'
    return 0
  fi

  normalized_body="Closes #$inferred_issue_number"
  if [[ "$body" != "$normalized_body" ]]; then
    gh pr edit "$pr_number" --repo "$(canonical_repo)" --body "$normalized_body" >/dev/null
  fi

  jq -n \
    --argjson number "$pr_number" \
    --argjson issue_number "$inferred_issue_number" \
    --arg body "$normalized_body" \
    '{ok: true, number: $number, issue_number: $issue_number, body: $body}'
}

if [[ $# -gt 0 ]]; then
  normalize_one_pr "$1"
  exit 0
fi

prs="$(gh pr list --repo "$(canonical_repo)" --state open --limit 200 --json number)"
results='[]'
while IFS= read -r pr_number; do
  [[ -n "$pr_number" ]] || continue
  result="$(normalize_one_pr "$pr_number")"
  results="$(jq -c --argjson result "$result" '. + [$result]' <<<"$results")"
done < <(jq -r '.[].number' <<<"$prs")

jq -n --argjson results "$results" '{ok: true, results: $results}'
