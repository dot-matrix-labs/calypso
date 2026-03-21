#!/usr/bin/env bash
set -euo pipefail

PLAN_FILE="${1:-}"
if [[ -z "$PLAN_FILE" ]]; then
  printf 'usage: %s <plan-json-file>\n' "$(basename "$0")" >&2
  exit 1
fi

jq -e '
  .plan_issue_number
  and (.ordered_issues | type == "array")
  and ((.ordered_issues | length) > 0)
' "$PLAN_FILE" >/dev/null || {
  printf 'invalid plan json: missing plan_issue_number or ordered_issues\n' >&2
  exit 2
}

if jq -e '
  (.ordered_issues | map(.number) | length) != (.ordered_issues | map(.number) | unique | length)
' "$PLAN_FILE" >/dev/null; then
  printf 'invalid plan json: duplicate issue numbers in ordered_issues\n' >&2
  exit 3
fi

if jq -e '
  any(.ordered_issues[];
    (.number == null)
    or (.title == null)
    or (.risk == null)
    or (.rationale == null)
    or ((.dependencies // []) | type != "array")
    or ((.dependents // []) | type != "array")
  )
' "$PLAN_FILE" >/dev/null; then
  printf 'invalid plan json: ordered issue is missing required fields\n' >&2
  exit 4
fi

if jq -e '
  has("batches") or has("phases") or has("parallel_groups")
  or any(.ordered_issues[]; has("batch") or has("phase") or has("parallel_group"))
' "$PLAN_FILE" >/dev/null; then
  printf 'invalid plan json: parallel or phase metadata is forbidden\n' >&2
  exit 5
fi

if ! jq -e '
  . as $root
  | all(.ordered_issues[];
      . as $issue
      | all((.dependencies // [])[];
          . as $dep
          | ($root.ordered_issues | any(.number == $dep))
        )
    )
' "$PLAN_FILE" >/dev/null; then
  printf 'invalid plan json: dependency references must exist in ordered_issues\n' >&2
  exit 6
fi

if ! jq -e '
  . as $root
  | [range(0; (.ordered_issues | length))] as $idxs
  | all($idxs[];
      . as $i
      | ($root.ordered_issues[$i]) as $issue
      | all(($issue.dependencies // [])[];
          . as $dep
          | ($root.ordered_issues | map(.number) | index($dep)) as $dep_index
          | ($dep_index != null) and ($dep_index < $i)
        )
    )
' "$PLAN_FILE" >/dev/null; then
  printf 'invalid plan json: dependency order violation\n' >&2
  exit 7
fi

jq -n --arg file "$PLAN_FILE" '{ok: true, file: $file}'
