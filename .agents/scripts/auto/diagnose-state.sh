#!/usr/bin/env bash
set -euo pipefail

input="$(cat)"

jq '
  def diagnosis($state; $reason_code; $recommended_action; $observations; $safe_options):
    {
      state: $state,
      reason_code: $reason_code,
      observations: $observations,
      safe_options: $safe_options,
      recommended_action: $recommended_action
    };

  if (.selection.kind // "") == "none" then
    if (.selection.reason // "") == "no-eligible-issue" then
      empty
    else
      diagnosis(
        "blocked";
        (if (.selection.reason // "") == "plan-not-found" then "plan-not-found" else "selection-unavailable" end);
        "restore-plan";
        [
          "Auto could not select a planned issue.",
          ("selection.reason=" + (.selection.reason // "unknown"))
        ];
        ["restore-plan", "run-calypso-replan"]
      )
    end
  elif (.prep.ok? == false) then
    diagnosis(
      "blocked";
      "prep-invalid";
      "fix-prep";
      (
        ["Deterministic issue preparation did not pass."]
        + ((.prep.reasons // []) | map("prep:" + .))
      );
      ["fix-prep", "rerun-calypso-auto"]
    )
  elif (.local.state // "") == "remote-diverged" then
    diagnosis(
      "ambiguous";
      "remote-diverged";
      "inspect-branch-divergence";
      (
        ["Local branch state diverged from the remote branch."]
        + ((.local.reasons // []) | map("local:" + .))
      );
      ["inspect-branch-divergence", "sync-branch-then-rerun"]
    )
  else
    empty
  end
' <<<"$input"
