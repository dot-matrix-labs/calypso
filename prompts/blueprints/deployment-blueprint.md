
# Deployment Blueprint

## Target Environment

* Bare metal deployment targeting Linux natively. Avoid Docker.
* Applications are strictly kept alive natively using `systemd`.
* Environment variables are specified using `.env` files.
* Test environment variables (including `FIXTURES`) are safely pushed to the repository in `.env.test` for CI execution.
* Bun serves server APIs and static assets.
* Reverse proxy optional; CDN optional.
* Deployment must remain simple; avoid unnecessary complexity.

## CI/CD

* Separate build pipelines for browser and server.
* Deploy per milestone (Alpha/Beta/V1).
* Logging, monitoring, backups enforced at Beta stage.
* Observability of errors, external integrations, and user actions.

## Constraints

* Maintain single-stack coherence (TypeScript + Bun + React + Tailwind).
* Follow the dependency discretion policy (Buy vs DIY).
* No polyglot microservices unless explicitly required.
* No direct database calls from browser code.

---

## Logging & Telemetry

**SPAN Logging, Tracing, and Summarization**

* **Browser-to-Server Handoff:** Browser errors (React error boundaries, unhandled rejections, DOM crashes) must be explicitly caught and POSTed back to the Bun server's `/api/logs` endpoint.
* **Distributed Traces:** Every request/interaction must generate a unique `traceId`. This trace must seamlessly follow the user from the browser click down to the database query, allowing perfect chronological reconstruction of any workflow.
* **LLM-friendly `uniques.log`:** In addition to a standard chronological stdout/file log, the server must maintain a `uniques.log` file.
  * This file acts as a Set of errors, deduplicating repetitive alerts.
  * An AI agent inspecting the system should only need to read `uniques.log` to see the *categories* of errors currently afflicting the system, without wasting its token context window scrolling through thousands of identical "Timeout" errors.
* **Retention Policy:** Logs should be rotated (e.g., daily) and kept for a maximum of 14 days on the bare-metal server to prevent disk exhaustion, unless explicitly offloaded to a cold storage solution like S3.
