
# Calypso Blueprint

> [!IMPORTANT]
> **New project?** Follow [scaffold-task.md](../process/scaffold-task.md) before anything else. That document provisions the cluster, creates the GitHub repo, and seeds the scaffold.

> [!IMPORTANT]
> **Session start (existing project):** At the start of every session, read all files in `agent-context/` before proceeding. Do NOT begin any development or documentation work until this is complete.

> [!IMPORTANT]
> **Development gate:** You are only to proceed with feature development after all steps in [scaffold-task.md](../process/scaffold-task.md) are verified as complete.

---

## Blueprints

| File | Contents |
|---|---|
| [environment-blueprint.md](./environment-blueprint.md) | Dev host requirements, Linux dependencies, agent session bootstrap |
| [architecture-blueprint.md](./architecture-blueprint.md) | Stack, repo structure, data/integration guidelines, dependency policy |
| [process-blueprint.md](./process-blueprint.md) | Scaffold → V1 lifecycle, PRD, implementation plan, next-prompt loop |
| [testing-blueprint.md](./testing-blueprint.md) | Testing philosophy, test categories, CI/CD workflows |
| [deployment-blueprint.md](./deployment-blueprint.md) | Containerized deployment, K8s rolling updates, structured logging, trace propagation |
| [auth-blueprint.md](./auth-blueprint.md) | Authentication, authorization, session management, agent auth |
| [data-blueprint.md](./data-blueprint.md) | Data persistence, encryption, privacy, analytics tier |
| [ux-blueprint.md](./ux-blueprint.md) | UX posture, agent UX, admin as user, beauty gate |
| [worker-blueprint.md](./worker-blueprint.md) | Worker container design, task queue subscription, write-through API, delegated user tokens, per-worker-type DB roles |
