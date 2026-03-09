# Proposal: Property Graph on PostgreSQL — Schema as Data, Not DDL

## Problem

Relational schemas encode business assumptions as DDL. Every `CREATE TABLE` is a bet that the entity will look like that for a while. Every `ALTER TABLE` is an admission that the bet was wrong, paid for in migration risk, downtime, and coordination cost. In a static business with a stable domain model, migrations happen quarterly and the cost is manageable.

In an agent-driven system, the domain model is not stable. Agents discover new entity types, new relationships, new properties — daily, not quarterly. A migration-per-change model means the database becomes the bottleneck for business velocity. And the data that agents produce and consume is natively relational in the graph sense: entities with typed relationships to other entities, traversed in patterns that weren't anticipated when the schema was designed.

Graph databases solve the shape problem but surrender PostgreSQL's proven security surface: role-based access, row-level security, battle-tested encryption at rest, WAL-based PITR, and the operational maturity of a 30-year-old system. Moving to Neo4j or DGraph means re-solving every security problem the auth and data blueprints already address.

The answer is not a different database. It's a different relationship with PostgreSQL.

---

## Solution: Property Graph on PostgreSQL

Three tables. One migration. No more migrations after that.

```sql
-- ─── The only migration ───────────────────────────────────

CREATE TABLE entities (
  id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  type        TEXT NOT NULL,                    -- 'user', 'order', 'invoice', 'agent_task'
  properties  JSONB NOT NULL DEFAULT '{}',      -- all fields live here
  tenant_id   UUID,                             -- multi-tenant isolation
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
  created_by  UUID,                             -- actor (user or agent) who created this
  version     INT NOT NULL DEFAULT 1            -- optimistic concurrency control
);

CREATE TABLE relations (
  id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  source_id   UUID NOT NULL REFERENCES entities(id),
  target_id   UUID NOT NULL REFERENCES entities(id),
  type        TEXT NOT NULL,                    -- 'owns', 'purchased', 'assigned_to', 'derived_from'
  properties  JSONB NOT NULL DEFAULT '{}',      -- edge metadata (weight, role, timestamp, etc.)
  tenant_id   UUID,
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
  created_by  UUID
);

CREATE TABLE entity_types (
  type        TEXT PRIMARY KEY,
  schema      JSONB NOT NULL,                   -- JSON Schema: expected properties, required fields
  sensitive   TEXT[] NOT NULL DEFAULT '{}',      -- property names that must be encrypted before insert
  kms_key_id  TEXT,                             -- KMS key ID for this entity type's sensitive fields
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ─── Indexes ──────────────────────────────────────────────

CREATE INDEX idx_entities_type        ON entities (type);
CREATE INDEX idx_entities_tenant      ON entities (tenant_id);
CREATE INDEX idx_entities_properties  ON entities USING GIN (properties);
CREATE INDEX idx_relations_source     ON relations (source_id);
CREATE INDEX idx_relations_target     ON relations (target_id);
CREATE INDEX idx_relations_type       ON relations (type);
CREATE INDEX idx_relations_properties ON relations USING GIN (properties);
```

### What this buys you

**No more migrations for business changes.** Adding a new entity type is `INSERT INTO entity_types`. Adding a property to an existing type is `UPDATE entity_types SET schema = ...`. Neither requires DDL, downtime, or a rollback plan. The migration runner still exists, but it runs exactly once — to create these three tables plus the audit and analytics schemas.

**Graph traversal is native.** Recursive CTEs give you arbitrary-depth traversal:

```sql
-- All entities reachable from a given node within N hops
WITH RECURSIVE graph AS (
  SELECT target_id, type, 1 AS depth
  FROM relations WHERE source_id = $1
  UNION ALL
  SELECT r.target_id, r.type, g.depth + 1
  FROM relations r JOIN graph g ON r.source_id = g.target_id
  WHERE g.depth < $2
)
SELECT DISTINCT e.* FROM graph g JOIN entities e ON e.id = g.target_id;
```

**References between entities are always edges, never property values.** This is the discipline that makes the graph queryable. An `order` doesn't have a `customer_id` property — it has a `placed_by` relation to a `user` entity. The relation is explicit, typed, queryable, and carries its own metadata.

**The type registry is machine-readable.** Agents inspect `entity_types` to understand the data model. They can discover what types exist, what properties each type expects, which properties are sensitive, and what relations are common. This is not documentation — it's live metadata that the validation layer enforces.

**JSONB GIN indexes make property queries fast.** `WHERE properties @> '{"email": "x@y.com"}'` uses the GIN index. For hot-path lookups on specific properties, partial indexes give column-like performance:

```sql
CREATE INDEX idx_users_email ON entities ((properties->>'email')) WHERE type = 'user';
```

---

## Interaction with Existing Policies

### Encryption

Key-per-table becomes **key-per-type**. The `entity_types` table declares `sensitive` (array of property names) and `kms_key_id`. The `FieldEncryptor` reads the type registry, encrypts the listed properties within the JSONB before insert, and decrypts after read. An encrypted property is a base64url envelope string inside the JSONB — the database stores it like any other string property. The envelope format (`keyVersion || iv || ciphertext`) is unchanged.

### Three-database structure

Unchanged. `calypso_app` holds `entities`, `relations`, `entity_types`. `calypso_analytics` holds analytics entities and relations (pseudonymized). `calypso_audit` holds audit log entries. Three roles, same privilege model.

### Analytics tier

Analytics events become entities and relations in `calypso_analytics` — attributed to session pseudonyms, not user IDs. The graph model is actually more natural for analytics: an event is an entity; its relationship to a session pseudonym is an edge. Agent consumers traverse the analytics graph without ever touching the transactional graph.

### Audit

Audit entries reference entity IDs and relation IDs. The `AuditEntry` gains `entityId` and `entityType` fields instead of `resourceType` and `resourceId`. Same log-first semantics, same INSERT-only role.

### Schema evolution

Type schema versioning replaces migrations. When a property is added, existing entities without it are valid — the schema specifies defaults. When a property is removed, the application stops reading it; existing values are inert. When a property's type changes, the schema version increments and the application handles both shapes during a transition window. This is how document databases handle schema evolution, and it works because the validation layer is in the application, not the database.

---

## What This Does NOT Change

- **Security posture** — PostgreSQL roles, row-level security, PITR, encryption at rest — all unchanged
- **Auth model** — passkeys, JWTs, agent scopes, revocation — all unchanged
- **M-of-N, KMS separation, audit-log-first** — all unchanged
- **Agent tier separation** — agents still cannot reach `calypso_app`; they read from `calypso_analytics`

---

## Trade-offs

**JSONB property queries are slower than columnar queries for table scans.** For indexed lookups (`@>`, `->>` with a partial index) the difference is negligible. For analytical scans across millions of rows, columnar wins. This is acceptable because analytical workloads run against the analytics tier, not the transactional graph.

**No referential integrity on property values.** A property that contains a UUID is not FK-enforced. The discipline is: inter-entity references are always edges. Properties hold values, not references. This is a convention enforced by the type registry and validation layer, not by the database.

**Recursive CTEs have depth limits under load.** For traversals deeper than ~5 hops on large graphs, CTEs get expensive. For business data graphs (org structures, order chains, agent task trees) this is rarely a problem — most traversals are 2-3 hops. If deep traversal becomes a hot path, a materialized view precomputes the closure.

**Losing column-level constraints.** `NOT NULL`, `CHECK`, `UNIQUE` on individual fields are not available inside JSONB. Uniqueness (e.g., no two users with the same email) requires a partial unique index: `CREATE UNIQUE INDEX ON entities ((properties->>'email')) WHERE type = 'user'`. This is more verbose than a column constraint but equally enforced.

---

## Alternative Considered: Apache AGE

Apache AGE is a PostgreSQL extension that adds native openCypher (graph query language) support. It deserves serious evaluation because it solves the same problem from the PostgreSQL side rather than requiring us to model graph patterns ourselves.

### How AGE stores data

AGE uses **standard PostgreSQL tables with table inheritance**. When you create a graph, AGE creates a PostgreSQL schema. Vertex and edge labels become child tables inheriting from `_ag_label_vertex` and `_ag_label_edge`. Properties are stored as `agtype` — a custom type that is a superset of JSONB. Underneath, it is regular PostgreSQL tables all the way down, which means WAL replication, pg_dump, PITR, and standard backups all work.

### Schema strategy

AGE is schema-flexible in the same way as our proposal. Labels (vertex/edge types) are created at runtime — no DDL migration from the application's perspective. Properties are schema-free within labels: two vertices with the same label can have entirely different property keys. This matches our type-registry approach but with the label system built into the extension rather than managed by application code.

### Query language: openCypher vs. recursive CTEs

The key advantage is **openCypher for graph traversal**:

```sql
-- AGE: variable-length path traversal
SELECT * FROM cypher('app', $$
  MATCH (a:User)-[:OWNS*1..3]->(resource)
  WHERE a.id = '550e8400-e29b-41d4-a716-446655440000'
  RETURN resource
$$) AS (resource agtype);
```

Compare to the recursive CTE equivalent — openCypher is dramatically more readable for multi-hop traversals and variable-length paths. For simple adjacency lookups, CTEs are marginally faster (~0.8ms vs ~1.5-3.7ms). For complex patterns, Cypher is both faster and more maintainable.

**Hybrid queries** work by embedding Cypher inside SQL via the `cypher()` function in the `FROM` clause. Cypher results can be JOINed with regular SQL tables, used in CTEs, and composed with standard SQL. This means graph data and relational data coexist and interoperate in a single query.

### Compatibility with our security model

- **PostgreSQL roles and permissions:** Supported. AGE 1.7.0 added proper ACL permission flags.
- **Row-Level Security:** Added in 1.7.0 (executor-level enforcement). USING policies filter; WITH CHECK policies raise errors. Critical for multi-tenant isolation.
- **Indexing:** `agtype` supports BTree, Hash, and GIN indexes on the underlying label tables.
- **WAL, replication, PITR:** All work — data is in regular PostgreSQL tables.

### Operational concerns

| Factor | Status |
|---|---|
| Apache top-level project | Yes — graduated from incubator |
| AWS RDS availability | **Not available** |
| GCP Cloud SQL availability | **Not available** |
| Azure Database for PostgreSQL | **Supported** |
| Self-hosted PostgreSQL | Works with PG14-18 |
| RLS support | Added in 1.7.0 (latest) |
| Production maturity | Active development, limited public production case studies |

### Assessment

AGE is architecturally sound — it stores data in regular PostgreSQL tables, inherits all PostgreSQL security features (with the 1.7.0 RLS addition), and openCypher is categorically better than recursive CTEs for graph traversal.

**The blocker is managed service availability.** AGE is not available on AWS RDS or GCP Cloud SQL. For a deployment that must run on a managed PostgreSQL service (which is the right operational choice), AGE limits you to Azure or self-hosted PostgreSQL. Self-hosted PostgreSQL trades extension availability for operational burden — patching, scaling, backup management — that a managed service handles.

### Recommendation

**Track AGE for future adoption. Build on the DIY property graph model now.**

Our three-table model (entities, relations, entity_types) with JSONB properties and recursive CTEs gives us the graph storage model and schema flexibility immediately, on any PostgreSQL deployment. If AGE reaches AWS RDS (the most likely managed service for production), adopting it later is straightforward: the underlying storage model is nearly identical (AGE uses inherited tables with `agtype` properties; we use JSONB properties in a flat table). The migration path is: create an AGE graph, bulk-insert entities as vertices and relations as edges, and rewrite recursive CTEs as openCypher queries. The data model, encryption envelope, type registry, and security posture are unchanged.

The decision point to revisit: when AGE is available on the target managed PostgreSQL service AND the application has graph traversal patterns deeper than 3-4 hops that are performance-sensitive.

---

## Proposed Document Changes

| Document | Change |
|---|---|
| Data blueprint — Core Principles | Replace "PostgreSQL is the database — write standard SQL" with "Schema as data, not DDL" principle. Add graph model rationale. |
| Data blueprint — Design Patterns | Add Pattern: Property Graph on PostgreSQL. Add Pattern: Type Registry. Remove or reframe patterns that assume fixed tables. |
| Data blueprint — Architectures | Update Architecture A diagram to show entities/relations/entity_types. |
| Data blueprint — Checklist | Replace migration-focused items with type registry items. |
| Data implementation | Rewrite database section: three-table schema, type registry, graph query patterns, encryption on JSONB properties. |
| Auth implementation | Minor: note that auth data (passkeys, revocation, agent registry) are entity types in the graph, not fixed tables. |
