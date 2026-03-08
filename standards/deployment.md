# Deployment & Database

## Deployment
- Bare-metal Linux, `systemd`-managed. No Docker for the application.
- **Exception:** PostgreSQL container at V1 (`docker compose`).
- Secrets via `systemd` `EnvironmentFile=` (see `security.md`). `.env` for local dev only.
- `.env.test` committed for CI (sandbox creds only).
- Bun serves APIs + static assets. Reverse proxy/CDN optional.

## Database

| Phase | Engine |
|---|---|
| Prototype → Alpha | SQLite via `bun:sqlite` |
| Beta → V1 | PostgreSQL in container |

- **No ORMs.** Parameterized SQL only. No Prisma/TypeORM/Drizzle.
- Migrations: `apps/server/migrations/` as numbered `.sql` files. Forward-only. Server applies on startup via `schema_version` table.
- No direct DB calls from browser code.
