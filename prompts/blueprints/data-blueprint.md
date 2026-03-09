# Data Blueprint

> [!IMPORTANT]
> This blueprint defines Calypso's data management posture: how data is persisted, encrypted, partitioned, and analyzed while preserving customer privacy. It is the companion to the [Auth Blueprint](./auth-blueprint.md) which covers identity and access control.

---

## Vision

Every application that handles customer data faces a fundamental tension: the business needs analytics to make decisions, and the customer needs assurance that their records are not exposed, aggregated without consent, or weaponized by a breach. Most systems resolve this tension with policy — a privacy notice, an internal access-control list, a promise. Policy is necessary but insufficient. The resolution is architectural: separate the data the business analyzes from the data the customer owns, and make the separation structural rather than procedural.

A well-designed data layer treats encryption not as a checkbox but as a series of concentric barriers. Disk encryption protects against physical theft. Database-level encryption protects against file exfiltration. Field-level encryption protects against compromised database credentials. User-held keys protect against compromised application servers. A breach at any single layer yields ciphertext, not plaintext. The alternative — plaintext behind a perimeter — means that a single misconfigured firewall rule or leaked credential exposes everything.

Data minimization is not a policy aspiration; it is a technical control. Every field collected is a field that can be breached, subpoenaed, or misused. Data that does not exist cannot leak. A system that collects "just in case" accumulates liability with no corresponding value. The discipline of justifying every stored field — and deleting what is no longer needed — reduces the blast radius of every other failure mode.

Agents — automated processes that act on behalf of the system — are constrained to aggregated, anonymized views of data. An agent that can read individual customer records is an agent that can exfiltrate individual customer records. The architectural boundary between raw transactional data and the analytics tier is not a convenience; it is the enforcement mechanism for this constraint.

The persistence layer is PostgreSQL from the first commit. Starting with an embedded database and planning to migrate later is a false economy — the migration cost is paid in full in downtime, schema rewrites, and bugs introduced during data transfer, and the security properties of the early-stage architecture are weaker in ways that matter before any customer data arrives. The three-database structure that the policy requires (transactional, analytics, audit) is available locally via Docker Compose from day one and on any managed PostgreSQL service in production. There is no architecture to migrate away from.

---

## Threat Model

| Scenario | What must be protected |
|---|---|
| Database backup exfiltrated from storage | Customer PII — names, emails, addresses, payment tokens. Backups must contain ciphertext, not plaintext. |
| Compromised database user account | Sensitive fields (PII, financial data). Application-layer encryption ensures DB credentials alone do not yield readable data. |
| Server root access obtained by attacker | Encryption keys. Keys must not reside in environment variables or on the same host as the encrypted data. |
| Rogue administrator queries raw customer records | Individual customer privacy. Access to the transactional store must be audit-logged, and agents must be restricted to aggregated views. |
| Analytics query re-identifies individuals from pseudonymized data | Pseudonymization integrity. Session-based pseudonyms must rotate, and aggregation must apply differential privacy before export. |
| Agent process accesses raw transactional data | Customer records. Agents operate on the analytics tier only; no code path connects an agent to the transactional store. |
| Single encryption key compromised | Blast radius. Per-table or per-domain key separation limits exposure to one data category, not the entire database. |
| Key compromise with no rotation capability | Long-term exposure. Key rotation must be a tested, automated procedure — not a theoretical plan. |
| Ransomware encrypts production database | Data availability and integrity. Encrypted, immutable backups stored off-host enable recovery without paying ransom. |
| Application logs contain decrypted PII | Secondary data leak. Log pipelines must strip or redact sensitive fields before persistence. |
| Schema migration drops or corrupts existing data | Data integrity. Migrations must be tested for rollback, and destructive operations must require explicit confirmation. |

---

## Core Principles

### Separate what the business analyzes from what the customer owns

The analytics tier and the transactional tier are distinct stores with distinct access controls, distinct encryption keys, and distinct data models. Analytics operates on pseudonymous, aggregated events — never on raw customer records. This separation is structural, enforced at the infrastructure level, not by application-layer access checks that a single bug can bypass. When analytics and transactions share a database, every analytics query is one `JOIN` away from a privacy violation.

### Encryption is layered — compromise of any single layer must not yield plaintext at any other layer

Four layers protect data at rest: disk encryption, database-level encryption, application-layer field encryption, and (where applicable) user-held keys. Each layer has a different threat model and a different key. Disk encryption protects against physical theft but not against a compromised database process. Database encryption protects against file exfiltration but not against a compromised database user. Field encryption protects against database compromise but not against a compromised application server. Layering ensures that no single point of failure exposes customer data.

### Schema as data, not DDL

The domain model is not stable. Business needs evolve, agents discover new entities, and relationships change daily. Relational schemas encoded as DDL (Data Definition Language) become a bottleneck if every change requires a migration, downtime, and coordination. Instead, the system uses a **Property Graph on PostgreSQL** model: a fixed, three-table schema that treats the data model itself as data. Adding a new entity type or property is an `INSERT` or `UPDATE` to the type registry, not a structural alteration of the database. Type schema versioning replaces migrations. When a property is added, existing entities without it remain valid. When a property is removed, the application stops reading it; existing values become inert.

### PostgreSQL is the database — use standard SQL with Graph patterns

The stack uses PostgreSQL across all stores. This provides proven security (RBAC, RLS), encryption, and operational maturity. Rather than a separate graph database, the system implements property graph patterns—entities, relations, and type registries—directly on PostgreSQL using JSONB for flexible properties and recursive CTEs for traversal. This combines the flexibility of a graph with the reliability of a 30-year-old relational engine.

### Data minimization is a privacy control, not a policy preference

Every field persisted is a field that can be breached, subpoenaed, or misused. The decision to collect a field requires justification: what product function depends on it, how long must it be retained, and what is the plan for deletion. The type registry enforces this for each entity type. Fields collected "for future use" are liabilities with no offsetting value. Retention policies are enforced by automated deletion, not by human discipline.

### Agents operate on aggregated data only

No agent process — whether it runs recommendations, generates reports, or performs background analysis — has a code path to the transactional customer store. Agents read from the analytics tier, which contains pseudonymous, aggregated, differentially private data. This is not a permissions check; it is an architectural boundary. An agent that could, in principle, read raw customer records is an agent that will, eventually, read raw customer records.

### Keys and data live on separate infrastructure

Encryption keys are managed by a dedicated key management service, not stored in environment variables, configuration files, or the same host as the encrypted data. Key operations (encrypt, decrypt, rotate) are API calls to an isolated service with its own access controls and audit log. Collocating keys and data means that a single host compromise yields both ciphertext and the means to decrypt it.

The KMS is a hard dependency for encryption and decryption — if it is unavailable, the application cannot serve requests that require sensitive data. This is a deliberate trade-off: an unavailable KMS is an operational incident with known recovery procedures, not a silent security degradation. The mitigation is operational (KMS high availability, health checks, alerting) not architectural (caching key material in application memory, which recreates the colocation problem). A narrow exception: a short-lived, in-memory cache of data encryption keys (DEKs) with a TTL of ≤ 5 minutes is acceptable for high-throughput paths, provided the cache is never persisted to disk and is cleared on process exit. Key encryption keys (KEKs) held by the KMS itself are never cached in the application.

### Audit precedes access

Every read of sensitive data is logged before the read is executed — not after, not asynchronously, not "when the batch job runs." The audit log uses a separate encryption key and a separate storage path from the data it protects. If the audit write fails, the data read is denied. This ordering guarantee transforms the audit log from a forensic afterthought into an active control.

---

## Design Patterns

### Pattern 1: Encrypt-Before-Insert

**Problem:** Data written to the database in plaintext is exposed to anyone with database credentials, backup access, or file-system access to the storage volume.

**Solution:** The application layer encrypts sensitive fields before passing them to the database. The database stores ciphertext. Decryption occurs in the application layer on read, using keys retrieved from the key management service. The database engine never sees plaintext for sensitive columns.

**Trade-offs:** Encrypted fields cannot be indexed, searched, or sorted by the database engine. Queries that filter on encrypted fields require application-layer decryption of candidate rows or a separate plaintext index of non-sensitive derived values (e.g., a hash for lookup). This pattern adds latency to reads and writes proportional to the number of encrypted fields.

### Pattern 2: Property Graph on PostgreSQL

**Problem:** Static schemas (DDL) are rigid and make business velocity dependent on database migrations.

**Solution:** Store all domain data in three tables: `entities` (nodes), `relations` (edges), and `entity_types` (the registry).
- **`entities`**: Stores all objects with a `type` and a `properties` JSONB column.
- **`relations`**: Stores typed edges between entities (`source_id`, `target_id`, `type`, `properties`).
- **`entity_types`**: Stores the schema (JSON Schema), sensitivity metadata, and KMS key IDs for each type.

**Trade-offs:** Losing some native column constraints (`NOT NULL`, `CHECK`). These are replaced by JSON Schema validation and partial unique indexes on JSONB fields. Recursive CTEs for graph traversal can be slower than simple JOINs at extreme depths, but more than sufficient for business data graphs (org structures, task trees).

### Pattern 3: Type-Registry Validation

**Problem:** Moving to a schema-less JSONB model can lead to data corruption if not enforced.

**Solution:** The application layer validates every write against the schema defined in `entity_types`. This registry is live metadata that the validation layer and the `FieldEncryptor` use to decide how to handle each property.

**Trade-offs:** Validation logic moves from the database engine to the application tier. This increases application complexity but allows for more expressive validation and versioned schema evolution without DDL.

### Pattern 4: Key-Per-Type Encryption

**Problem:** A single encryption key for all data means that compromise of one key exposes everything.

**Solution:** The `entity_types` registry declares which properties are `sensitive` and which `kms_key_id` protects them. The `FieldEncryptor` reads this registry to encrypt specific keys within the JSONB blob before storage.

**Trade-offs:** Requires the registry to be consistent across all application instances. Rotation is handled per entity type.

### Pattern 3: Aggregation-Tier Separation

**Problem:** Running analytics queries against the transactional customer database creates a direct path from the analytics layer to raw customer records. A single query mistake, a permissive access grant, or a compromised analytics tool exposes individual customer data.

**Solution:** Analytics operates on a separate data store populated by pseudonymous, aggregated events. The transactional store emits events (not row copies) that are stripped of direct identifiers and attributed to rotating session pseudonyms. The analytics store has no foreign keys to the transactional store and no mechanism to reverse the pseudonymization.

**Trade-offs:** Two stores means two schemas, two backup strategies, and an event pipeline that must be monitored for lag and data loss. Debugging analytics anomalies is harder when you cannot trace an event back to a specific customer record — this is the intended behavior, but it frustrates ad-hoc investigation.

**Event pipeline required properties:** The pipeline must provide at-least-once delivery — events must not be silently dropped. Duplicate events are acceptable and must be handled by the analytics store (idempotent writes keyed on event ID). The pipeline does not guarantee ordering within a session; the analytics store must tolerate out-of-order event arrival and must not depend on insertion order for correctness. Pipeline lag must be monitored: a lag alert threshold (recommended: 60 seconds for real-time analytics, 5 minutes for batch) must be configured before real customer data enters the system. Events that fail pseudonymization must be dropped with a structured log entry, never written to the analytics store with a raw identifier as a fallback.

### Pattern 4: Session Pseudonymization

**Problem:** Attributing analytics events to permanent user identifiers (user IDs, emails) enables re-identification even when events are stored in a separate analytics tier.

**Solution:** Analytics events are attributed to session-scoped pseudonyms that rotate on a defined schedule (e.g., per session, per day). The mapping between permanent identifiers and session pseudonyms is held only in the transactional tier and is never exported to the analytics store. Longitudinal analysis uses cohort-level aggregation, not individual tracking across sessions.

**Trade-offs:** Rotating pseudonyms make it impossible to build per-user behavioral timelines in the analytics tier — this limits certain product analytics use cases (e.g., funnel analysis per user). Teams that need user-level analytics must access the transactional tier through the audit-controlled, access-restricted path, not the analytics tier.

### Pattern 5: Differential Privacy on Export

**Problem:** Aggregated query results can leak information about individuals, especially in small groups. A query that returns "average salary for the 3-person engineering team" reveals meaningful information about each individual.

**Solution:** Aggregated data exported from the analytics tier has calibrated noise added according to a differential privacy mechanism. The privacy budget (epsilon) is configured per query class and enforced by the analytics export layer. Queries that would exceed the privacy budget for a given dataset are rejected.

**Trade-offs:** Noise reduces the precision of analytics results. Low-cardinality groups (small teams, rare events) produce noisy results that may not be actionable. The privacy budget must be managed — too generous and privacy degrades; too restrictive and analytics becomes useless. Differential privacy is a mathematical guarantee, not an approximation, but it requires careful calibration.

**Budget management:** The privacy budget (epsilon) is a shared resource across the dataset. Each query against a dataset consumes a portion of the budget proportional to the sensitivity of the query and the noise level applied. Budget state is tracked in a persistent store keyed on dataset identifier; queries check and decrement the remaining budget atomically before executing. A query that would exceed the remaining budget is rejected with an explicit error — it does not execute with reduced noise. Budget ownership is a product-level decision: who sets the initial epsilon per dataset, who can reset it (and under what audit controls), and what the policy is for datasets that exhaust their budget before the reset interval. These decisions must be made before the DP mechanism is implemented, not after. For agent-initiated queries, budget exhaustion must return a structured error that the agent can surface to its operator — it must not cause the agent to retry with broader queries in an attempt to circumvent the limit.

### Pattern 6: Signed Analytics at the Edge

**Problem:** Analytics events generated client-side can be tampered with in transit — injecting false events, modifying values, or replaying old events to skew analytics.

**Solution:** The client signs each analytics event with a session-scoped key before transmission. The server validates the signature, checks for replay (via nonce or timestamp window), and rejects events that fail validation. Signed events are stored with their signatures for downstream audit.

**Trade-offs:** Client-side signing requires key distribution and increases event payload size. A compromised client can still generate valid but misleading events (the signature proves origin, not truthfulness). This pattern protects against network-level tampering, not against a malicious client.

### Pattern 7: Audit-Log-First

**Problem:** Audit logs written after data access are vulnerable to suppression — an attacker who can read data can also prevent the log entry from being written. Asynchronous logging has the same failure mode under load shedding or crash.

**Solution:** The access path writes the audit entry to an append-only log before executing the data read. If the audit write fails, the data read is denied. The audit log uses a separate encryption key and a separate storage backend from the data it covers. Log entries include the identity of the accessor, the timestamp, the data accessed, and the access justification (where applicable).

**Trade-offs:** Synchronous audit logging adds latency to every sensitive read — typically one additional write operation. The append-only log grows without bound and requires a retention and archival strategy. The separate storage backend is an additional infrastructure dependency. For high-throughput systems, the audit write can become a bottleneck; batching is not permitted because it breaks the "log before read" guarantee.

### Pattern 8: PII-Scrubbing Log Pipeline

**Problem:** Application logs record errors, request traces, and system events. Under normal operation, logs may appear clean. Under error conditions — exceptions, serialization failures, validation errors — the full object that caused the error is frequently logged, and that object may contain decrypted PII, database rows, or request payloads with sensitive fields.

**Solution:** All log output passes through a scrubbing layer before persistence. The scrubber maintains a deny-list of field names (e.g., `email`, `name`, `address`, `token`, `password`, `ssn`, `dob`, `phone`) and a pattern list (e.g., email address regex, credit card number pattern). Fields matching the deny-list are replaced with a fixed redaction marker. Error handlers are explicitly prohibited from logging raw objects, database rows, or request bodies — they log IDs and error codes only. The scrubber is applied at the log sink, not at each call site, so it catches accidental log calls that bypassed the convention.

**Verification:** PII-free status is not self-certifying. Before each gate (Alpha, Beta, V1), the log pipeline is verified under adversarial conditions: validation errors are deliberately triggered against payloads containing known PII patterns; exception handlers are exercised with objects containing sensitive fields; serialization of database row types is tested. Automated tests assert that log output matching known PII patterns is absent from log fixtures generated by these scenarios. "No plaintext PII in logs" on the checklist means this test suite exists and passes — not that someone reviewed the logs manually.

**Trade-offs:** A deny-list scrubber is not a perfect control — novel field names and deeply nested structures can escape it. It is a defense-in-depth measure that catches the most common accidental leaks. The definitive control is the discipline of never logging structured objects in error paths, which the scrubber reinforces but does not replace.

---

## Plausible Architectures

### Architecture A: Three-Database Single-Instance (scaffold through beta)

The baseline architecture from the first commit. A single PostgreSQL instance hosts three databases: `calypso_app` (transactional), `calypso_analytics` (analytics events), `calypso_audit` (audit log). The application server holds three separate connection pools with three separate database roles — the transactional role cannot write to the analytics or audit databases. The KMS runs as a separate process (local Vault in development via Docker Compose, cloud KMS in production). Analytics events flow from the application through an in-process event pipeline that pseudonymizes before writing to `calypso_analytics`.

```
┌──────────────────────────────────────────────────┐
│                Application Server                │
│                                                  │
│  ┌─────────────┐   ┌──────────────────────────┐  │
│  │  Business   │   │    Encryption Layer       │  │
│  │  Logic      │──►│ (per-type, registry-sync)  │  │
│  └──────┬──────┘   └────────────┬─────────────┘  │
│         │                       │                │
│  ┌──────▼──────┐   ┌────────────▼─────────────┐  │
│  │  Event      │   │    KMS Client             │  │
│  │  Pipeline   │   │    (Vault / AWS / GCP)    │  │
│  │  (pseudonym)│   └──────────────────────────┘  │
│  └──────┬──────┘                                 │
└─────────┼────────────────────────────────────────┘
          │
          │  app_rw          analytics_w     audit_w
          ▼                  ▼               ▼
┌──────────────────┐  ┌──────────────┐  ┌──────────────┐
│  calypso_app     │  │calypso_      │  │calypso_audit │
│ (entities,       │  │analytics     │  │(INSERT-only  │
│  relations,      │  │(pseudonymous │  │ role,        │
│  entity_types)   │  │ events,      │  │ append-only  │
└──────────────────┘  │ DP export)   │  │ table)       │
          │           └──────────────┘  └──────────────┘
          ▼
┌──────────────────┐
│  KMS             │
│  (Vault local /  │
│   cloud KMS)     │
└──────────────────┘
```

**Database roles:**
- `app_rw` — read/write on `calypso_app`; no access to `calypso_analytics` or `calypso_audit`
- `analytics_w` — insert-only on `calypso_analytics`; no read, no access to other databases
- `audit_w` — insert-only on `calypso_audit`; no `UPDATE`, no `DELETE`, no `TRUNCATE`, no access to other databases

**Development setup:** Docker Compose starts PostgreSQL and Vault. `bun run dev` expects both services running. A single `init.sql` script creates the three databases and three roles on first start.

**Trade-offs:** All three databases share one PostgreSQL instance — an instance-level failure takes down all three simultaneously. This is acceptable for pre-production and early production; the single instance is replaced by independent managed databases when availability SLAs require it. The event pipeline is in-process, so analytics event delivery is synchronous with request handling — lag is zero but pipeline failures surface as request errors. An async pipeline (separate worker, queue) is a production upgrade, not a day-one requirement.

### Architecture B: Multi-Tenant Encrypted Platform (production, regulated industries)

Per-tenant key hierarchies isolate encryption so that compromise of one tenant's keys does not expose another tenant's data. The key management service is backed by a hardware security module. The analytics tier applies differential privacy on all exports. The audit log is replicated to immutable cold storage.

```
┌─────────────────────────────────────────────────────┐
│                  Application Tier                   │
│  ┌─────────┐  ┌─────────┐  ┌─────────────────────┐ │
│  │Tenant A │  │Tenant B │  │  Encryption Layer    │ │
│  │ Context │  │ Context │  │  (per-tenant keys)   │ │
│  └────┬────┘  └────┬────┘  └──────────┬──────────┘ │
│       └─────┬──────┘               │               │
└─────────────┼──────────────────────┼───────────────┘
              │                      │
              ▼                      ▼
┌──────────────────┐    ┌──────────────────────────┐
│   Relational     │    │   Key Management Service │
│   Database       │    │   (HSM-backed)           │
│   (row-level     │    │                          │
│    security,     │    │   ┌────────┐ ┌────────┐  │
│    encrypted     │    │   │Tenant A│ │Tenant B│  │
│    fields)       │    │   │ Keys   │ │ Keys   │  │
│                  │    │   └────────┘ └────────┘  │
└──────────────────┘    └──────────────────────────┘
              │
              ▼
┌──────────────────┐    ┌──────────────────────────┐
│  Event Pipeline  │───►│  Analytics Event Store    │
│  (pseudonymize,  │    │  (tenant-partitioned,     │
│   strip PII)     │    │   DP on export)           │
└──────────────────┘    └──────────┬───────────────┘
                                   │
              ┌────────────────────┼──────────────┐
              ▼                    ▼              ▼
┌──────────────────┐  ┌────────────────┐  ┌────────────┐
│  Agent Consumer  │  │  Admin Report  │  │  Audit Log │
│  (aggregated,    │  │  Dashboard     │  │  (immutable│
│   DP-filtered)   │  │  (DP-filtered) │  │   cold     │
└──────────────────┘  └────────────────┘  │   storage) │
                                          └────────────┘
```

**From Architecture A:** The transition from Architecture A to Architecture B replaces the single shared PostgreSQL instance with independent managed databases per tier; adds per-tenant key hierarchies in the KMS; moves the event pipeline to an async worker with a durable queue; and adds HSM backing for the KMS. Schema, query patterns, and encryption envelope format are unchanged — the migration is an infrastructure and configuration change, not a code rewrite.

**Trade-offs:** Significant operational complexity. Per-tenant key hierarchies require tenant lifecycle management (onboarding creates keys, offboarding destroys them). HSM-backed key management adds latency and cost. Differential privacy on every export requires privacy budget tracking and may render small-tenant analytics unusable. Row-level security adds query planning overhead. This architecture is justified when regulatory requirements (SOC 2, GDPR, HIPAA) demand provable tenant isolation and mathematical privacy guarantees.

---

## Alternative Considered: Apache AGE

**Apache AGE** is a PostgreSQL extension that adds native openCypher (graph query language) support. It stores data in regular PostgreSQL tables using table inheritance, which means WAL replication, pg_dump, PITR, and standard backups all work.

**Assessment:** AGE is architecturally sound and openCypher is categorically better than recursive CTEs for graph traversal. The blocker is managed service availability — AGE is not currently available on AWS RDS or GCP Cloud SQL.

**Recommendation:** Track AGE for future adoption. Build on the DIY Property Graph model now. The transition path later is straightforward: dump entity boundaries and relations, bulk load into AGE graph structures, and rewrite recursive CTEs to openCypher.

---

## Implementation Checklist

### Alpha Gate

- [ ] Three PostgreSQL databases provisioned: `calypso_app`, `calypso_analytics`, `calypso_audit`
- [ ] Three database roles created with correct privilege scope: `app_rw`, `analytics_w`, `audit_w`
- [ ] Core property graph tables initialized: `entities`, `relations`, `entity_types`
- [ ] Docker Compose dev environment starts PostgreSQL and Vault
- [ ] All database queries use parameterized statements; no string concatenation
- [ ] Application-layer encryption active for sensitive properties in JSONB
- [ ] Key management service integrated; no encryption keys in config files
- [ ] Audit logging operational for all auth events and entity reads
- [ ] Audit log entries written before data access is granted (log-first ordering)
- [ ] Separate analytics event store operational; analytics queries never touch `calypso_app`
- [ ] Analytics events pseudonymized with session-scoped identifiers
- [ ] No plaintext PII present in application logs (verified by adversarial test suite)
- [ ] Single core migration applied to establish graph tables
- [ ] Point-in-time recovery configured and tested

### Beta Gate

- [ ] Differential privacy mechanism active on analytics exports; privacy budget (epsilon) configured per query class; budget exhaustion rejects queries, does not silently reduce noise
- [ ] Key rotation procedure tested end-to-end: rotate a table key, verify old rows remain readable via `keyVersion` lookup, verify new writes use the new key
- [ ] Per-tenant key isolation implemented (if multi-tenant); one tenant's key compromise verified not to expose another's data
- [ ] Backup restoration tested: restore from encrypted backup to a clean environment, verify data integrity and that decryption succeeds
- [ ] Rate limiting enforced on data-access endpoints
- [ ] Schema migration rollback tested: apply migration, roll back, verify no data loss or corruption
- [ ] Session pseudonym rotation verified: pseudonyms change per session (or per configured interval)
- [ ] Audit log tamper resistance verified: `audit_w` role cannot modify existing entries; confirmed by attempting `UPDATE` and `DELETE` with the audit role

### V1 Gate

- [ ] KMS backed by hardware security module (HSM); no software-only key storage in production
- [ ] Automated key rotation operational on schedule; zero-downtime rekeying verified under load
- [ ] Immutable audit log replicated to cold storage; retention policy enforced
- [ ] Full differential privacy pipeline active: noise calibration, budget tracking, budget exhaustion rejection
- [ ] Agent access restricted to analytics tier; no code path from agent processes to `calypso_app` (verified by architecture review and integration test)
- [ ] Penetration test completed against data layer; findings remediated
- [ ] Data retention automation operational: expired data deleted on schedule, deletion verified
- [ ] Log pipeline verified PII-free under adversarial conditions (error paths, stack traces, serialization edge cases)
- [ ] Cross-tenant data isolation verified by automated test suite (if multi-tenant)

---

## Antipatterns

- **Privacy policy as technical control.** Promising users their data is safe in a privacy policy while storing it in plaintext behind a single firewall. A policy is a legal obligation, not an enforcement mechanism. The gap between the promise and the implementation is the breach surface.

- **Disk encryption as data privacy.** Enabling transparent disk encryption and treating the data layer as "encrypted." Disk encryption protects against physical theft of the storage medium. It does nothing against a compromised database user, a leaked connection string, or an exfiltrated backup taken through the database's own export tools.

- **Analytics on the transactional store.** Running analytics queries directly against the production customer database because it is faster to build and avoids the complexity of a second store. Every analytics query becomes a potential privacy violation, and every analyst becomes a potential threat actor. The structural separation exists precisely to make this impossible, not merely forbidden.

- **Frontier crypto as default architecture.** Reaching for fully homomorphic encryption, zero-knowledge proofs, or secure multi-party computation before implementing aggregation-tier separation, differential privacy, or basic field-level encryption. These are powerful tools with legitimate use cases, but they are research-grade in most deployment contexts. Simpler, well-understood patterns solve the same problems with lower operational risk. Adopt frontier techniques deliberately, with a specific threat model row they address that simpler patterns cannot.

- **Collecting data "just in case."** Storing fields that no current product function requires, on the theory that they might be useful for future analytics or features. Every field is a liability — it increases breach exposure, complicates compliance, and creates retention obligations. Collect what you need, delete what you no longer need, and justify the difference.

- **Keys alongside data.** Storing encryption keys in environment variables on the database host, in the application's configuration file next to the database connection string, or in the same backup archive as the encrypted database. A single host compromise yields both ciphertext and keys. Key management exists as a separate service specifically to prevent this.

- **Single flat key for all tables.** Using one encryption key for the entire database because it is simpler to manage. One key compromise exposes every table. Key-per-table limits the blast radius to one data domain and enables independent rotation schedules.

- **PII in error logs.** Catching exceptions and logging the full request object, database row, or decrypted field value in the error handler. Under normal operation the logs look clean; under error conditions they become a secondary copy of the data they were supposed to protect. Log pipelines must redact before writing, not after.

- **Pseudonymization without rotation.** Assigning each user a permanent pseudonym in the analytics store and treating it as anonymization. A permanent pseudonym is a second identifier — it can be correlated across sessions, linked to external data, and re-identified with modest effort. Session-scoped, rotating pseudonyms resist longitudinal re-identification.

---

## Relationship to the Auth Blueprint

The data layer and the auth layer share one threat: a rogue agent accessing customer records it should not reach. The [Auth Blueprint](./auth-blueprint.md) addresses this at the HTTP layer through scoped agent tokens and per-route scope enforcement. This blueprint addresses it at the data layer through tier separation — no code path connects an agent process to the transactional store. These are two independent controls for the same threat. Scope enforcement can be bypassed by a middleware bug or a misconfigured route; tier separation cannot, because the agent process has no database credentials for the transactional store and no network path to it. Both controls must be present. Neither is sufficient alone.

---

## Incident Response: Data Compromise

The V1 Gate requires a runbook. At minimum it must cover:

- **Field-level key compromise:** identify which table's key was compromised; rotate that key immediately (not the full database key); re-encrypt affected rows using the new key in a background job; audit all reads of that table for the preceding key lifetime to assess the exfiltration window; other tables' keys are unaffected.
- **Database backup exfiltration:** determine whether the backup was encrypted at the field level (it must be); if field encryption was active, the attacker holds ciphertext and needs the KMS keys to decrypt — immediately audit KMS access logs for unauthorized key usage; if field encryption was not active for any reason, treat the exfiltrated data as plaintext and notify affected users.
- **PII discovered in log pipeline:** immediately halt log export to downstream consumers; identify the error path that produced the leak; deploy a scrubber fix; audit exported logs for the period the leak was active; notify affected users if the logs were accessible to unauthorized parties.
- **Agent process accessing transactional data:** this should be architecturally impossible — if it occurs it indicates a misconfiguration of the tier separation boundary. Immediately revoke the agent's credentials; determine how the boundary was breached (network policy, missing credential restriction, code path); treat all data the agent could have accessed as potentially exfiltrated pending log review.
