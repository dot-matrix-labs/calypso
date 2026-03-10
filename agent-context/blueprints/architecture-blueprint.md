
# Architecture Blueprint

> [!IMPORTANT]
> This blueprint defines the structural architecture for AI-agent-built web applications: how code is organized, how boundaries are enforced, and how dependencies are governed.

---

## Vision

A web application's architecture is the set of constraints that make the easy path and the correct path the same path. When architecture is absent or implicit, every developer — human or AI — makes local decisions that are individually reasonable and collectively incoherent. The server leaks into the client. The client calls the database. Utility functions appear in four different directories. Dependencies accumulate until no one can explain what half of them do.

Architecture for agent-built software carries an additional requirement: the structure must be legible to an agent encountering the codebase for the first time, in any session, without verbal explanation. An agent cannot ask a colleague where things go. It reads the directory tree, infers the rules, and acts. If the directory tree does not encode the rules, the agent will invent its own — and they will be wrong.

A well-architected codebase has a small number of top-level directories with clear, non-overlapping responsibilities. Server code and client code never share a runtime. Shared types live in an explicit shared location, not duplicated across boundaries. Dependencies are deliberate, justified, and minimal — because every dependency an agent installs is a dependency every future agent must understand, update, and debug. The cost of ignoring this blueprint is a codebase that grows in complexity faster than it grows in capability, until the agent spends more tokens navigating the mess than building the product.

---

## Threat Model

| Scenario | What must be protected |
|---|---|
| Server-side code imported into browser bundle | Runtime separation — server secrets, Node APIs, and database access must never reach the client |
| Browser-side code executed on the server | Security boundary — untrusted client logic must not run in a trusted context |
| Agent places new code in the wrong directory | Structural consistency — the directory tree must unambiguously encode where each type of code belongs |
| Shared types drift between client and server | Contract integrity — a single source of truth for data shapes must exist and be enforced |
| Agent adds a dependency for a trivial function | Dependency minimalism — each dependency is an ongoing maintenance and security liability |
| Dependency has a deep transitive tree that introduces vulnerabilities | Supply chain integrity — the total dependency surface must be auditable and minimal |
| Agent generates a new package or service without clear boundaries | Modularity — new packages must have explicit responsibilities that don't overlap with existing ones |
| API contracts change without updating consumers | Type safety — API inputs and outputs must be validated against actual production schemas |
| Monorepo structure becomes too deep or too flat to navigate | Navigability — an agent must be able to locate any component within seconds by reading the tree |

---

## Core Principles

### Boundaries are physical, not conceptual

Code that runs in different runtimes lives in different directories with different build pipelines. The separation is enforced by the build system, not by developer discipline. A browser import that reaches for a server module fails at build time, not at runtime in production.

### The directory tree is the architecture diagram

An agent reading the top-level directory listing must be able to answer: what are the deployable units, where are the shared contracts, where does business logic live, where do tests live. If the tree does not answer these questions at a glance, the architecture has failed its primary audience.

### Dependencies are liabilities, not features

Every external package is a commitment to track its releases, audit its security, understand its behavior, and handle its deprecation. The threshold for adding a dependency is not "does it save time now" but "is this functionality infeasible to build and maintain internally." AI agents shift the buy-vs-build calculus heavily toward build, because generating a focused, tested internal implementation costs tokens once; maintaining an external dependency costs tokens forever.

### Types are shared, logic is not

Data shapes that cross boundaries (API request/response types, domain types, shared enums) live in a single canonical location imported by both sides. Business logic, rendering logic, and data access logic are never shared across the client-server boundary — even if they look similar. Sharing logic creates coupling; sharing types creates contracts.


### Simplicity scales; cleverness does not

A monorepo with five top-level directories and obvious naming survives ten developers, a hundred files, and a thousand commits. A monorepo with clever abstractions, auto-generated barrels, and dynamic module resolution breaks the moment an agent tries to understand it. Choose boring, obvious structure over sophisticated, implicit structure every time.

---

## Design Patterns

### Pattern 1: Strict Runtime Separation

**Problem:** In a full-stack TypeScript codebase, server and client code use the same language, making it trivially easy to import server code into the client or vice versa.

**Solution:** Server code and client code live in separate top-level directories with separate entry points and separate build configurations. No import path from the client directory can resolve to a file in the server directory. Shared types are extracted into a third location that both sides import from, but that location contains only type definitions — no runtime code.

**Trade-offs:** Duplicating a utility function across client and server feels wasteful. But the alternative — a shared runtime utils package — becomes a magnet for coupling. Accept the duplication; it is cheaper than the debugging when shared logic behaves differently in two runtimes.

### Pattern 2: Buy vs. DIY Decision Framework

**Problem:** AI agents, like human developers, default to installing packages for solved problems. But in an agent-built codebase, each dependency is a black box the agent must re-learn in every session, and a supply chain node the agent cannot audit.

**Solution:** Apply a two-question test before adding any dependency: (1) Is this functionality infeasible to build internally within reasonable effort? (2) Is the package mature, minimal in footprint, and well-maintained? Both must be yes. If the functionality is a utility function, a simple data transformation, or a thin wrapper — the agent builds it internally with tests. If it is a cryptographic primitive, a payment gateway SDK, or a browser automation engine — buy it.

**Trade-offs:** Building internally means owning maintenance of that code. For trivial utilities this is cheap. For complex functionality (e.g., a JWT library) the line is blurry — the decision must be documented and revisitable.

### Pattern 3: Monorepo with Explicit Package Boundaries

**Problem:** As a codebase grows, code organization becomes ambiguous. Where does a new service go? Where does shared UI live? Agents resolve ambiguity by guessing, and guesses accumulate into inconsistency.

**Solution:** The repository has a fixed set of top-level directories, each with a clear responsibility. Applications (deployable units) live in one area. Packages (shared libraries) live in another. Tests live in a third. New packages are added only when their responsibility does not overlap with any existing package. The package list is documented and enforced.

**Trade-offs:** A fixed structure resists organic growth. Some code will not fit neatly into the predefined buckets. The answer is to expand the structure deliberately (add a new package with documented responsibility), not to bend existing packages to hold unrelated code.

### Pattern 4: Type-Safe API Contracts

**Problem:** When client and server are developed independently (possibly by different agents or in different sessions), API contracts drift. The client expects one shape; the server returns another. The mismatch surfaces at runtime.

**Solution:** API contracts are defined as TypeScript types in the shared types package. Both client and server import from this canonical source. Integration tests validate that actual API responses match these types. The types are versioned alongside the API — a breaking change in the API requires a type change, which forces a type error in all consumers.

**Trade-offs:** Requires discipline to update types before (or simultaneously with) API changes. An agent that modifies the server response without updating the shared type will break the build — which is the desired behavior, but requires that CI catches it before merge.

---

## Plausible Architectures

### Architecture A: Monorepo with Collocated Packages (small team, single product)

```
┌────────────────────────────────────────────────────┐
│  Repository Root                                   │
│                                                    │
│  /apps                                             │
│    /web        ← Browser bundle (UI framework +    │
│    │              styles, client-only)              │
│    /server     ← API server (runtime, ESM)         │
│                                                    │
│  /packages                                         │
│    /core       ← Business logic, domain types      │
│    /ui         ← Shared UI components              │
│    /services   ← API clients, external adapters    │
│    /integrations ← Third-party SDK wrappers        │
│                                                    │
│  /tests                                            │
│    /unit       ← Pure logic tests                  │
│    /integration ← API contract tests               │
│    /e2e        ← Full-page browser tests           │
│                                                    │
│  /docs         ← Architecture, product, roadmap    │
└────────────────────────────────────────────────────┘
```

**When appropriate:** Single product, one to three agents, early through mid-stage development. The entire codebase fits in one repository with no need for independent deployment of packages.

**Trade-offs vs. other architectures:** All code shares a single version. No independent package versioning. Simpler to navigate but harder to extract a package for reuse later.

### Architecture B: Multi-App Monorepo (multiple products, shared foundation)

```
┌────────────────────────────────────────────────────┐
│  Repository Root                                   │
│                                                    │
│  /apps                                             │
│    /web-admin   ← Admin dashboard                  │
│    /web-public  ← Public-facing application        │
│    /server-api  ← Core API server                  │
│    /server-jobs ← Background job runner             │
│                                                    │
│  /packages                                         │
│    /shared-types ← API contracts, domain types     │
│    /ui-kit       ← Design system components        │
│    /core         ← Business logic                  │
│    /db           ← Database access layer           │
│                                                    │
│  /tests          ← Organized by app and suite      │
│  /docs                                             │
└────────────────────────────────────────────────────┘
```

**When appropriate:** Multiple user-facing applications sharing a common backend and type system. Team of three or more agents. Mid-stage through production.

**Trade-offs vs. Architecture A:** More directories to navigate. Package boundaries must be more carefully maintained. But each app can be built and deployed independently, and shared packages prevent drift between apps.

### Architecture C: Polyrepo with Shared Types Package (independent teams, strict boundaries)

```
┌──────────────┐   ┌──────────────┐   ┌──────────────┐
│  Repo: Web   │   │  Repo: API   │   │  Repo: Types │
│  /src        │   │  /src        │   │  /src        │
│  /tests      │   │  /tests      │   │  /tests      │
│              │   │              │   │              │
│  imports ────┼───┼──────────────┼───┤  Published   │
│  @org/types  │   │  imports ────┼───┤  @org/types  │
└──────────────┘   │  @org/types  │   └──────────────┘
                   └──────────────┘
```

**When appropriate:** Independent teams or organizations that cannot share a single repository. Regulatory contexts requiring separate access controls per codebase. The types package is the only coupling point.

**Trade-offs vs. Architecture A/B:** Maximum isolation but maximum coordination cost. Type changes require publishing, versioning, and updating in all consuming repos. Agent context is limited to one repo at a time — cross-repo reasoning is expensive.

---

---

> For the Calypso TypeScript implementation of these patterns, see [architecture-implementation.md](../implementation-ts/architecture-implementation.md).

## Implementation Checklist

### Alpha Gate

- [ ] Repository initialized with `/apps/web`, `/apps/server`, `/packages/core`, `/packages/ui`, `/tests` structure
- [ ] `/apps/web` builds a browser bundle; no server imports resolve at build time
- [ ] `/apps/server` builds and starts a Bun server; no browser/DOM imports resolve
- [ ] Shared TypeScript types defined in `/packages/core` and imported by both apps
- [ ] `docs/dependencies.md` created; every external dependency listed with Buy/DIY justification
- [ ] CI runs separate build steps for web and server
- [ ] At least one REST endpoint defined with typed request/response matching `/packages/core` types
- [ ] Vitest configured and running for unit tests
- [ ] Playwright configured and running for browser tests (headless)

### Beta Gate

- [ ] All API endpoints have corresponding TypeScript types in `/packages/core`
- [ ] Integration tests validate API responses match shared types
- [ ] No `any` types in API contracts; strict TypeScript enforced
- [ ] Dependency tree audited; no unnecessary transitive dependencies
- [ ] New package addition requires documented justification in `docs/dependencies.md`
- [ ] Build times measured; no single build step exceeds 30 seconds

### V1 Gate

- [ ] Decoupling test passed: removing all Bun/React/Tailwind references from Principles and Patterns sections leaves them intact
- [ ] All packages have explicit, documented responsibilities with no overlap
- [ ] Zero unused dependencies in `package.json` / `bun.lockb`
- [ ] API contract versioning in place; breaking changes require type updates and consumer fixes
- [ ] Repository structure documented in `docs/architecture.md` and matches actual directory tree

---

## Antipatterns

- **The shared utils junk drawer.** A `/packages/utils` or `/lib/helpers` directory with no cohesive responsibility. It starts with one date formatter, accumulates string helpers, grows a random API wrapper, and eventually contains code that belongs in five different packages. Every agent adds to it because it is the path of least resistance.

- **Import path acrobatics.** Using `../../../packages/core/types` instead of workspace aliases or clean import maps. Deep relative paths obscure where code actually lives and make refactoring a find-and-replace nightmare. If the import path requires counting dots, the project structure is not serving its purpose.

- **Server code in the browser bundle.** Importing a server utility "just for one function" into the client. The bundler pulls in the entire module and its dependencies. At best, the bundle bloats. At worst, server secrets or Node-only APIs are shipped to the browser.

- **Premature microservices.** Splitting a monorepo into separate services before the product has proven its domain boundaries. Each service needs its own deployment, monitoring, and contract management. For an agent-built early-stage product, this is overhead with no benefit — the boundaries are not yet known.

