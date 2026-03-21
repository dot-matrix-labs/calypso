---
name: replan-evaluate
description: Evaluate compliant planned issues for dependency order, code coupling, and technical risk, then emit structured sequential plan JSON.
user_invocable: false
---

# Replan Evaluate

This skill is the only non-deterministic part of replanning.

Input should come from the deterministic replan scripts, especially:

```bash
.agents/scripts/replan/collect-plan-issues.sh
.agents/scripts/replan/collect-open-prs.sh
.agents/scripts/replan/rank-input.sh
```

## Must do

- Read the structured issue payload, not ad hoc GitHub snippets.
- Evaluate both explicit feature dependencies and inferred code/subsystem
  dependencies.
- Prefer the strictest correct sequential ordering.
- Break ties by prioritizing higher technical risk or unknowns first.
- Emit structured JSON suitable for `apply-plan.sh`.

## Must not do

- Do not plan parallel work.
- Do not emit phase, batch, or step metadata for issue titles or issue bodies.
- Do not invent issue facts not grounded in the provided payload.
- Do not emit free-form markdown as the primary output.

## Output contract

Emit JSON with this shape:

```json
{
  "plan_issue_number": 47,
  "ordered_issues": [
    {
      "number": 196,
      "title": "chore: remove TUI implementation",
      "risk": 4,
      "rationale": "Cross-cutting CLI architecture removal touches many commands.",
      "dependencies": [],
      "dependents": [201]
    }
  ]
}
```

`ordered_issues` must be a strict total order with no parallel groups.
