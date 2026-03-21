# Looping State Machine Examples

These examples are intentionally small. Copy one example directory into your
project's `.calypso/` directory, then run:

```bash
calypso template validate
```

The examples all follow the same pattern:

1. Start at the intake state.
2. Choose or inspect the next unit of work.
3. Do the work.
4. Summarize or mark the result.
5. Transition to `restart`.
6. Loop back to the first state until the operator interrupts the run.

## Documented Example: Project Task Iteration

The `project-task-iteration/` example is the simplest "keep going until exit"
loop:

- `task-intake` collects the next available work.
- `choose-task` picks the next concrete task for the pass.
- `execute-task` performs the task.
- `report` summarizes what changed.
- `restart` sends the workflow back to `task-intake`.

The loop is explicit in `state-machine.yml`:

```yaml
- from: restart
  to: task-intake
```

That edge is what keeps the state machine running until the user stops it.
