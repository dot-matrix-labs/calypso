# Calypso FAQ

## For humans: Why do I need a blueprint

Software rarely fails due to code quality — it fails because of unclear requirements, chaotic architecture, and hype-driven choices. Calypso enforces architecture-first design, staged maturity, and stack consistency. Humans focus on decisions; AI handles repetitive work. The result: maintainable systems that scale from prototype to production without collapsing under complexity.

## Development Environment

### "Why not on my Mac?"
You don't deploy on a Mac. Building on Mac to deploy on Ubuntu is an anti-pattern. Develop on bare-metal Linux so dev/test/deploy environments are identical.

### "Why not Docker for consistency?"
Docker adds networking, volume, and build complexity for a single-stack app. A `systemd` service + `.env` is fundamentally easier for AI agents to write and debug than Dockerfile layers.

## Architecture & Testing

### "Why Bun?"
Faster start times, built-in TypeScript execution (no `ts-node`), built-in testing. Fewer toolchain dependencies.

### "Why never fabricate API responses?"
Fabricated data tests your *imagination* of how an API behaves, not reality. Golden fixtures from real requests capture actual behavior. This eliminates a massive source of production bugs.

### "Why no Redux/MobX?"
React hooks + context handle 90% of cases. Heavy state libraries encourage global state and tight coupling — harder for agents to reason about.

### "Why DIY over npm install?"
Every dependency is code you don't control with its own transitive deps, security flaws, and breaking changes. For trivial utilities, an agent generates a clean, tested implementation in seconds. Reserve "Buy" for complex, high-liability features (Stripe, PDF generation).

### "Why no ORMs?"
ORMs abstract away SQL performance and add massive generated footprint. AI agents can generate performant, type-safe queries directly. Fewer dependencies, no workflow assumptions.

### "Why no Docker for the app but Docker for the DB?"
App = single Bun process. Running via systemd is simpler and eliminates networking/volume debugging. Databases benefit from containerized deployment for version pinning and isolation.

### "What about security?"
`.env` files are for dev only. Production uses systemd `EnvironmentFile=` (root-owned, 0600). At V1, a dedicated secrets manager. See `standards/security.md`.

### "Can multiple agents work together?"
Yes. Git is the coordination layer: agents claim tasks from the shared plan, work on branches, merge via PR. See `standards/multi-agent.md`.

### "What happens when there are no feature tasks?"
Hardening mode: test coverage, dependency elimination, code condensation, security audit, telemetry verification. Can run as nightly CI. See `standards/hardening.md`.

### "How do agents know about production errors?"
Telemetry database (`telemetry.db`) with read-only SQL access. Agents query at session start. See `standards/telemetry.md`.
