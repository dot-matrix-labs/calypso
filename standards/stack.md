# Stack & Dependencies

## Environment
- Bare-metal Linux. Dev/test/deploy on same host.
- Local Mac/Windows allowed ONLY for initial Scaffold phase — nag user to move to remote Linux.
- Agent sessions run in `tmux`. Dev host = live preview on port `31415`.

## Host Dependencies
`git`, `gh` (HTTPS auth), `tmux`, `bun`, AI agent CLI, Playwright OS deps (`bunx playwright install-deps`).

## Stack
TypeScript only | Bun runtime | React UI | Tailwind CSS | Vitest + Playwright testing.
No other languages. No Redux/Zustand/MobX. No npm/npx — use `bun`/`bunx`.

## Repo Structure
```
/apps/web        # browser bundle (React + Tailwind)
/apps/server     # Bun server
/packages/{ui,core,services,integrations}
/tests/{unit,integration,e2e}
/docs
```
Strict browser/server separation. CI enforces separate builds.

## Data & Integration
- REST APIs. Universal TypeScript types for all API I/O.
- No GraphQL/WebSockets/Protobufs unless PRD demands it.
- API contracts versioned and type-checked against production responses.

## Dependencies
- **DIY** if buildable in ~50 tested lines. **Buy** only for complex/high-liability (Stripe, PDF).
- Lock versions. Document all deps in `docs/dependencies.md` with justification.
- No cascading transitive dependencies.
