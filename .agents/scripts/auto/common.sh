#!/usr/bin/env bash
set -euo pipefail

require_cmd() {
  local name="$1"
  if ! command -v "$name" >/dev/null 2>&1; then
    printf 'missing required command: %s\n' "$name" >&2
    exit 127
  fi
}

require_cmd gh
require_cmd jq
require_cmd awk
require_cmd grep
require_cmd git
require_cmd sed

canonical_repo() {
  gh repo view --json nameWithOwner -q .nameWithOwner
}

repo_root() {
  dirname "$(git rev-parse --path-format=absolute --git-common-dir)"
}

tasks_repo() {
  local base candidate
  base="$(canonical_repo)"
  candidate="$(gh repo view --json nameWithOwner -q '(.owner.login) + "/" + (.name) + "-tasks"')"

  if gh repo view "$candidate" >/dev/null 2>&1; then
    printf '%s\n' "$candidate"
  else
    printf '%s\n' "$base"
  fi
}

extract_issue_refs() {
  grep -oE '#[0-9]+' | tr -d '#' | awk '!seen[$0]++'
}

extract_closing_issue_number() {
  awk 'BEGIN{IGNORECASE=1} /^(Closes|Fixes|Resolves) #[0-9]+/ {print; exit}' \
    | grep -oE '[0-9]+' \
    | head -n1
}

section_body() {
  local section="$1"
  awk -v section="$section" '
    /^## / {
      if (in_section) {
        exit
      }
      in_section = ($0 == "## " section)
      next
    }
    in_section {
      print
    }
  '
}

count_checkboxes() {
  local pattern="$1"
  grep -cE "$pattern" || true
}

json_file() {
  local path="$1"
  jq -c . "$path"
}

sanitize_slug() {
  tr '[:upper:]' '[:lower:]' \
    | sed -E 's/^[[:space:]]+|[[:space:]]+$//g' \
    | sed -E 's/^[a-z]+:[[:space:]]*//' \
    | sed -E 's/[^a-z0-9]+/-/g' \
    | sed -E 's/^-+|-+$//g' \
    | cut -c1-48
}

issue_branch_name() {
  local issue_number="$1"
  local issue_title="$2"
  local prefix slug

  prefix="$(sed -E 's/^([a-z]+):.*/\1/;t;d' <<<"$issue_title" || true)"
  if [[ -z "$prefix" ]]; then
    prefix="feat"
  fi

  slug="$(printf '%s' "$issue_title" | sanitize_slug)"
  if [[ -z "$slug" ]]; then
    slug="issue"
  fi

  printf '%s/%s-%s\n' "$prefix" "$issue_number" "$slug"
}

issue_worktree_path() {
  local branch_name="$1"
  printf '%s/.agents/worktrees/%s\n' "$(repo_root)" "${branch_name//\//-}"
}

managed_issue_branch_regex() {
  printf '%s\n' '^[a-z]+/[0-9]+-[a-z0-9-]+$'
}

worktree_root() {
  printf '%s/.agents/worktrees\n' "$(repo_root)"
}
