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

canonical_repo() {
  gh repo view --json nameWithOwner -q .nameWithOwner
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
