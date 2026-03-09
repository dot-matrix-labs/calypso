
# Calypso Blueprint

> [!IMPORTANT]
> **Agent Development Gate:** You are only to proceed with feature development after all steps in [scaffold-task.md](../process/scaffold-task.md) are verified as complete.

> [!IMPORTANT]
> **Bootstrap Requirement:** At the start of every session, run:
> ```
> curl -sSL https://raw.githubusercontent.com/dot-matrix-labs/calypso/main/scripts/bootstrap-standards.sh | bash
> ```
> Then read all files in `./docs/standards/` before proceeding. Do NOT begin any development or documentation work until this is complete.

---

## Blueprints

| File | Contents |
|---|---|
| [environment-blueprint.md](./environment-blueprint.md) | Dev host requirements, Linux dependencies, agent session bootstrap |
| [architecture-blueprint.md](./architecture-blueprint.md) | Stack, repo structure, data/integration guidelines, dependency policy |
| [process-blueprint.md](./process-blueprint.md) | Scaffold → V1 lifecycle, PRD, implementation plan, next-prompt loop |
| [testing-blueprint.md](./testing-blueprint.md) | Testing philosophy, test categories, CI/CD workflows |
| [deployment-blueprint.md](./deployment-blueprint.md) | Bare-metal deployment, systemd, logging, telemetry |
| [auth-blueprint.md](./auth-blueprint.md) | Authentication, authorization, session management, agent auth |
| [data-blueprint.md](./data-blueprint.md) | Data persistence, encryption, privacy, analytics tier |
| [ux-blueprint.md](./ux-blueprint.md) | UX posture, agent UX, admin as user, beauty gate |
