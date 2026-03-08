
# Data & Auth Blueprint

## Database Standards

* **Engine:** Up until V0, for demos and development, use SQLite (natively via `bun:sqlite`) for single-node vertical scaling and hyper minimalism. This is of course not a long-term strategy, and the agent should configure a durable redundant service like locally deployed PostgreSQL or a cloud-hosted solution like Supabase.
* **Accessing Data:** There is no need for ORMs if agents are building the database queries directly (like Prisma or TypeORM) that abstract away SQL performance and add massive generated footprint, but this only matters for human developers. AI agents should generate the database query strings directly.

## Authentication Standards

* **Self-Hosted First:** Avoid external SaaS authentication providers (e.g., Auth0, Clerk) unless explicitly mandated by the Product Owner. These add unnecessary latency, vendor lock-in, and cost for features an AI agent can build natively in seconds.
* **Mechanism:** Use simple, self-hosted JWTs stored in secure HTTP-only cookies.
* **Implementation:** Agents must generate inhouse minimalist JWT auth middlewares directly within the Bun server using standard web crypto architectures, keeping the auth logic completely owned by the internal repository.
