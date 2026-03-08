
# Data & Auth Blueprint

> [!IMPORTANT]
> This blueprint defines how AI-agent-built applications store data and authenticate users: database strategy, query authoring, session management, and the progression from embedded databases to production-grade persistence.

---

## Vision

Data persistence and authentication are the two capabilities that transform a demo into a product. Without persistence, every session starts from zero. Without authentication, every user is everyone. These capabilities are also the two areas where the industry has accumulated the most accidental complexity — ORMs that abstract SQL into an opaque layer of generated code, authentication services that charge per monthly active user for functionality that amounts to hashing a password and signing a token.

In agent-built software, the calculus is different. An AI agent can write SQL directly, understand query plans, and generate type-safe database access code without an ORM's abstraction layer. An agent can implement JWT authentication from standard cryptographic primitives without a SaaS provider's SDK. The abstractions that protect human developers from complexity are unnecessary when the developer can hold the full complexity in context — and they are harmful when they obscure what is actually happening, making debugging and optimization impossible without understanding the abstraction's internals.

The correct approach starts minimal and progresses deliberately. An embedded database for early development and demos. Direct SQL queries with type-safe wrappers. Self-hosted JWT authentication with no external dependencies. As the product matures, the database migrates to a durable, redundant engine — but the query patterns and authentication architecture remain the same. The cost of ignoring this blueprint is either premature complexity (an ORM and an auth service before the product has its first user) or permanent fragility (raw queries with no type safety and a hand-rolled auth system with subtle security flaws).

---

## Threat Model

| Scenario | What must be protected |
|---|---|
| SQLite database file is corrupted or lost on a single host | Data durability — early-stage accepts this risk; production requires redundant storage |
| Agent generates SQL with string interpolation, enabling injection | Query safety — all user input must be parameterized, never interpolated |
| ORM generates inefficient queries that the agent cannot diagnose | Query transparency — the agent must see and control the SQL that executes |
| Authentication token is stolen from browser storage | Session security — tokens must be in HTTP-only cookies, inaccessible to JavaScript |
| JWT signing key is committed to version control | Secret management — signing keys are runtime secrets, never in code or config files |
| External auth provider has an outage, locking out all users | Auth availability — self-hosted auth has no external dependency for the login path |
| Agent implements JWT without algorithm pinning, enabling algorithm confusion attacks | Token integrity — the signing algorithm must be explicitly pinned, not inferred from the token |
| Database schema changes break existing queries | Schema evolution — migrations must be explicit, versioned, and reversible |
| Browser code directly queries the database, bypassing server authorization | Access boundary — all database access goes through the server; the browser never connects to the database |

---

## Core Principles

### The agent writes the SQL

An ORM generates SQL that the agent cannot see, cannot optimize, and cannot debug without understanding the ORM's internals. An agent that writes SQL directly produces queries that are transparent, tunable, and fully within context. The query is the code — there is no hidden layer between intent and execution.

### Start embedded, graduate to durable

An embedded database requires zero configuration, zero networking, and zero operational overhead. It is the correct choice for scaffold, prototype, and demo stages where the priority is speed and simplicity. When the product reaches alpha and real data enters the system, the database migrates to a durable, redundant engine. The migration is planned from the start — the query patterns are designed to be engine-portable.

### Authentication is owned, not rented

Self-hosted authentication means the login path has no external dependency, no per-user pricing, and no vendor lock-in. The authentication system is code in the repository — reviewable, testable, and debuggable by any agent. External auth providers are justified only when the product requires a capability that is genuinely infeasible to build internally (e.g., federated enterprise SSO with SAML).

### Tokens are opaque to the browser

Authentication tokens stored in JavaScript-accessible storage (localStorage, sessionStorage, non-HTTP-only cookies) are accessible to any script running on the page — including injected scripts from XSS vulnerabilities. HTTP-only cookies are invisible to JavaScript by browser design. The token travels with every request automatically; the browser code never touches it.

### Type safety spans the full data path

The type of a database row, the type of an API response, and the type consumed by the UI component are the same type — defined once, imported everywhere. A schema change that adds a column surfaces as a type error in every consumer that does not handle the new field. The type system is the migration safety net.

---

## Design Patterns

### Pattern 1: Typed Query Builder (no ORM)

**Problem:** Raw SQL strings are error-prone and invisible to the type system. ORMs solve this but add abstraction, generated code, and runtime overhead. The agent needs type safety without opacity.

**Solution:** Write SQL queries as parameterized strings and wrap them in typed functions that accept typed inputs and return typed outputs. The function signature is the contract; the SQL inside is visible and editable. The agent generates these wrappers directly — there is no code generation step or schema introspection tool.

**Trade-offs:** The agent must keep the SQL and the TypeScript types in sync manually. A column rename in the database requires updating both the SQL and the type definition. Mitigation: integration tests that run queries against the real database catch mismatches immediately.

### Pattern 2: Progressive Database Migration

**Problem:** Starting with a production-grade database (managed service, connection pooling, replication) adds operational complexity before the product has any data. Starting with a toy database and rewriting everything later is equally wasteful.

**Solution:** Design the data access layer against an abstraction that works with both an embedded database and a production database. Use the embedded database through demo stage. Migrate to the production engine at alpha by changing the connection configuration and running a schema migration — without rewriting queries. The query patterns (parameterized SQL, typed wrappers) remain identical.

**Trade-offs:** SQL dialect differences between embedded and production engines can surface at migration time. Mitigation: use standard SQL where possible and test with the production engine in CI before the migration.

### Pattern 3: Self-Hosted JWT Authentication

**Problem:** Authentication is a solved problem, but the solutions (SaaS providers, heavy libraries) add external dependencies, cost, and opacity. An agent cannot debug an auth failure that happens inside a third-party service.

**Solution:** Implement JWT authentication using standard cryptographic operations available in every modern runtime. The server signs tokens with a secret key using a pinned algorithm. Tokens are stored in HTTP-only, secure, same-site cookies. The server validates the token on every request via middleware. The entire auth system is fewer than 200 lines of code that the agent can read, test, and modify.

**Trade-offs:** The team owns the security of the auth implementation. Subtle mistakes (not pinning the algorithm, using symmetric signing in a distributed system, not checking expiry) can create vulnerabilities. Mitigation: the implementation checklist in this blueprint specifies the exact security properties that must be verified.

### Pattern 4: Server-Only Data Access

**Problem:** In a full-stack application with a shared language, it is tempting to import database utilities into browser code "just for types" or "just for validation." This creates a path from the browser to the database that bypasses server-side authorization.

**Solution:** All database access code lives exclusively in the server application. The browser communicates with the server via API endpoints. The server enforces authorization before executing any query. Shared types (row shapes, API contracts) live in a shared package — but the shared package contains no runtime code that accesses the database.

**Trade-offs:** Some validation logic may be duplicated between client and server (e.g., form validation). The duplication is acceptable — client validation is a UX convenience; server validation is the security boundary. They serve different purposes and may have different rules.

---

## Plausible Architectures

### Architecture A: Embedded Database, Single Server (scaffold through demo)

```
┌─────────────────────────────────────────────────┐
│  Application Server                             │
│                                                 │
│  ┌───────────────┐  ┌───────────────────────┐  │
│  │ API Routes    │  │ Auth Middleware        │  │
│  │ (REST)        │  │ (JWT verify, cookie)  │  │
│  └───────┬───────┘  └───────────────────────┘  │
│          │                                      │
│          ▼                                      │
│  ┌───────────────┐                              │
│  │ Data Access   │                              │
│  │ (typed SQL)   │                              │
│  └───────┬───────┘                              │
│          │                                      │
│          ▼                                      │
│  ┌───────────────┐                              │
│  │ Embedded DB   │  ← single file on disk       │
│  │ (SQLite)      │                              │
│  └───────────────┘                              │
└─────────────────────────────────────────────────┘
```

**When appropriate:** Early development through demo stage. No separate database server. Zero configuration. Data lives in a single file that can be backed up by copying it.

**Trade-offs:** No concurrent write support beyond what the embedded engine provides. No replication. No point-in-time recovery. Acceptable because the data is not yet critical — the product is being shaped, not operated.

### Architecture B: Managed Database, Server with Migrations (alpha through production)

```
┌──────────────────────────┐     ┌──────────────────────┐
│  Application Server      │     │  Database Server      │
│                          │     │  (managed or local)   │
│  API Routes              │     │                       │
│  Auth Middleware          │     │  Relational engine    │
│  Data Access (typed SQL) │────▶│  with replication     │
│  Migration Runner        │     │  and backups          │
└──────────────────────────┘     └──────────────────────┘
```

**When appropriate:** Alpha stage and beyond. Real user data enters the system. The database is a separate process (or managed service) with durability guarantees, automated backups, and point-in-time recovery.

**Trade-offs:** Operational overhead: connection management, credential rotation, backup verification. Justified because the data is now valuable and must survive host failures.

### Architecture C: Federated Auth with Self-Hosted Fallback (enterprise, multi-tenant)

```
┌──────────────────────────────────────────────────────┐
│  Application Server                                  │
│                                                      │
│  ┌────────────────────────────────────────────────┐  │
│  │  Auth Router                                   │  │
│  │                                                │  │
│  │  ├── /auth/local    → Self-hosted JWT flow     │  │
│  │  ├── /auth/sso      → External IdP (SAML/OIDC)│  │
│  │  └── /auth/verify   → Unified token validation │  │
│  └────────────────────────────────────────────────┘  │
│                                                      │
│  Internal JWT is always the session token.           │
│  External IdP is used only for initial identity      │
│  verification; the session is still self-hosted.     │
└──────────────────────────────────────────────────────┘
```

**When appropriate:** Enterprise customers that require SSO. The external identity provider verifies who the user is; the application issues its own JWT for the session. This keeps the session layer self-hosted while supporting federated login.

**Trade-offs:** Adds complexity: SAML/OIDC libraries, redirect flows, certificate management. Only justified when enterprise SSO is a product requirement, not a speculative feature.

---

## Reference Implementation — Calypso TypeScript

> The following is the Calypso TypeScript reference implementation. The principles and patterns above apply equally to other stacks; this section illustrates one concrete realization using Bun, SQLite, PostgreSQL, and standard Web Crypto.

### Database: Early Stage (SQLite)

- Engine: `bun:sqlite` (built into Bun runtime, zero dependencies)
- File location: `./data/app.db` (gitignored)
- Queries: parameterized SQL via `db.prepare(sql).all(params)`
- Migrations: numbered `.sql` files in `/apps/server/migrations/`, applied in order at startup

### Database: Production Stage (PostgreSQL)

- Engine: PostgreSQL (locally installed or cloud-hosted, e.g., Supabase)
- Connection: standard connection string in `.env`
- Queries: same parameterized SQL pattern; dialect differences handled in migration files
- Backups: automated via PostgreSQL's native tooling or provider's backup service

### Authentication

- **Signing:** HMAC-SHA256 via Web Crypto API (`crypto.subtle.sign`)
- **Algorithm pinning:** JWT header `alg` is verified to be `HS256` before validation; any other value is rejected
- **Token storage:** HTTP-only, Secure, SameSite=Strict cookie
- **Expiry:** Configurable; default 24 hours; enforced server-side on every request
- **Middleware:** Single function that extracts the cookie, verifies the JWT, and attaches the user to the request context; applied to all protected routes

### Package Structure

```
/apps/server
  /middleware/auth.ts       # JWT verify middleware
  /routes/auth.ts           # Login, logout, token refresh
  /migrations/              # Numbered SQL migration files
  /db.ts                    # Database connection and typed query functions
/packages/core
  /types/user.ts            # User type, shared between server and client
  /types/auth.ts            # Auth-related types (login request, token payload)
```

### Dependency Justification

| Package | Reason | Buy or DIY |
|---|---|---|
| `bun:sqlite` | Built into runtime; zero-dependency embedded database | Buy (runtime built-in) |
| PostgreSQL client | Database wire protocol; infeasible to implement | Buy |
| JWT library | Standard JWT sign/verify is ~50 lines with Web Crypto; no library needed | DIY |
| ORM (Prisma, TypeORM, Drizzle) | Adds abstraction, generated code, and runtime overhead; agent writes SQL directly | Do not buy |
| Auth SaaS (Auth0, Clerk) | Adds latency, cost, vendor lock-in for functionality the agent builds in minutes | Do not buy (unless mandated) |

---

## Implementation Checklist

### Alpha Gate

- [ ] Database engine selected and running (SQLite for early stage, PostgreSQL for alpha)
- [ ] All queries use parameterized SQL; grep for string interpolation in query strings returns zero results
- [ ] Typed query wrapper functions exist for all database operations; return types match shared types in `/packages/core`
- [ ] Migration system implemented; migrations run automatically on server startup
- [ ] JWT signing implemented with algorithm pinned to HS256; signing key loaded from `.env`
- [ ] JWT stored in HTTP-only, Secure, SameSite=Strict cookie
- [ ] Token expiry enforced server-side; expired tokens rejected with 401
- [ ] Auth middleware applied to all protected routes
- [ ] Login endpoint accepts credentials, verifies against database, returns signed JWT in cookie
- [ ] No database imports in any file under `/apps/web`; verified by import analysis

### Beta Gate

- [ ] PostgreSQL (or equivalent durable engine) configured and running with automated backups
- [ ] Data migration from SQLite to PostgreSQL tested and documented
- [ ] Connection pooling configured; max connections appropriate for expected load
- [ ] Password hashing uses a memory-hard algorithm (argon2, bcrypt) with appropriate cost factor
- [ ] Token refresh mechanism implemented; users are not logged out unnecessarily
- [ ] Rate limiting on login endpoint; brute-force attempts throttled
- [ ] All auth-related behavior covered by integration tests (login, logout, expiry, invalid token, algorithm confusion)

### V1 Gate

- [ ] Database point-in-time recovery tested; backup restored to a fresh instance and verified
- [ ] Schema migration rollback tested; at least one migration reversed cleanly
- [ ] Auth system penetration-tested: algorithm confusion, token forgery, cookie theft, CSRF
- [ ] Session revocation implemented (logout invalidates the token, not just clears the cookie)
- [ ] Database query performance measured; slow queries identified and indexed
- [ ] All sensitive data at rest identified and encrypted where required (see Data Security Blueprint)

---

## Antipatterns

- **ORM as safety blanket.** Using Prisma, TypeORM, or Drizzle because "ORMs prevent SQL injection." Parameterized queries prevent SQL injection. The ORM adds a schema file, a generation step, a runtime library, and a layer of abstraction between the agent and the database. The agent pays the cost in every session but gets no capability it cannot provide itself with typed query functions.

- **Auth SaaS by default.** Reaching for Auth0 or Clerk because "authentication is hard." For a standard username/password/JWT flow, authentication is roughly 200 lines of code using standard Web Crypto. The SaaS provider charges per user, adds an external dependency to the login path, and makes debugging auth failures require reading a third-party dashboard.

- **Tokens in localStorage.** Storing JWTs in `localStorage` because it is easier to access from JavaScript. Every script on the page can read `localStorage` — including injected scripts from XSS vulnerabilities. HTTP-only cookies are invisible to JavaScript by browser specification. Use them.

- **Unparameterized queries.** Building SQL strings with template literals or string concatenation using user input. This is SQL injection — the oldest and most preventable vulnerability in web applications. Every query parameter must use the database driver's parameterization mechanism.

- **Algorithm-agnostic JWT verification.** Verifying a JWT by reading the `alg` header from the token itself and using that algorithm. An attacker can change `alg` to `none` or switch from asymmetric to symmetric signing. The algorithm must be pinned server-side and any token claiming a different algorithm must be rejected.

- **Premature PostgreSQL.** Setting up PostgreSQL, connection pooling, and automated backups before the product has its first user or any data worth preserving. The operational overhead is real and ongoing. SQLite costs nothing and lets the team focus on the product. Migrate when real data arrives, not before.

- **Shared database module in the browser.** Importing a database utility into the browser bundle "just for the types." The bundler pulls in the module and its dependencies. Even if the database code is tree-shaken away, the import path exists and a future developer or agent may follow it to add runtime database calls from the browser.
