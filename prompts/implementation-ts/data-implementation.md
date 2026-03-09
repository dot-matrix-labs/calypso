# Data — Calypso TypeScript Implementation

> This document is the Calypso TypeScript reference implementation for the [Data Blueprint](../blueprints/data-blueprint.md). The principles, threat model, and patterns in that document apply equally to other stacks. This document covers the concrete realization using TypeScript, Bun, PostgreSQL, and Web Crypto.

---

## Database

PostgreSQL from the first commit. Three core tables in `calypso_app` provide a flexible property graph model.

| Table | Purpose |
|---|---|
| `entities` | The nodes: `id`, `type`, `properties` (JSONB), `tenant_id`, `version`. |
| `relations` | The edges: `id`, `source_id`, `target_id`, `type`, `properties` (JSONB). |
| `entity_types` | The registry: `type` (PK), `schema` (JSONB), `sensitive` (text[]), `kms_key_id`. |

### Roles and Privileges

| Database | Role | Privileges |
|---|---|---|
| `calypso_app` | `app_rw` | Read + write on `entities`, `relations`, `entity_types`. |
| `calypso_analytics` | `analytics_w` | `INSERT` only on analytics entities/relations. |
| `calypso_audit` | `audit_w` | `INSERT` only on the audit log table. |

The application server holds three separate connection pools. `app_rw` credentials are used for the transactional graph; `analytics_w` for the analytics tier; `audit_w` for the append-only audit log.

### Dev environment

Docker Compose starts PostgreSQL and Vault. An `init.sql` script (run once on first container start) creates the three databases and three roles. `bun run dev` requires both services running.

```
# docker-compose.yml (relevant services)
# postgres: standard postgres image, port 5432
# vault: hashicorp/vault image in dev mode, port 8200
```

### Migrations

Core graph tables are established in the first migration (`0001_initial_graph.sql`). Because the schema is flexible (JSONB properties), business evolution rarely requires further DDL migrations.

- **Schema Evolution**: Adding a new entity type is `INSERT INTO entity_types`. Adding a property is `UPDATE entity_types SET schema = ...`.
- **Validation**: Performed in the application layer against the JSON Schema in the registry before SQL execution.
- **Rollback**: Registry changes are audited and can be reverted by updating the type registry back to a previous state.

### Queries

Parameterized SQL via the `postgres` client (`sql\`SELECT ... WHERE id = ${id}\``). No ORM. No string concatenation in query construction.

---

## Encryption

- Application-layer field encryption: AES-256-GCM via Web Crypto API.
- **Key-per-type**: The `entity_types` registry declares which properties are `sensitive` and which `kms_key_id` protects them.
- Encrypt before insert, decrypt after read: The `FieldEncryptor` intercepts writes, checks the registry, and encrypts sensitive keys within the JSONB property blob.

### Ciphertext envelope format

Every encrypted column stores a base64url-encoded envelope:

```
base64url( keyVersion (4 bytes) || iv (12 bytes) || ciphertext + auth tag (n+16 bytes) )
```

- **`keyVersion`**: 4-byte big-endian uint32. Identifies the exact key version used for encryption. Required for key rotation — the decryption path reads `keyVersion` to retrieve the correct key from the KMS, so old rows remain readable after a rotation without any cutover.
- **`iv`**: 12 bytes, randomly generated per encryption call via `crypto.getRandomValues`. A static or reused IV breaks AES-GCM's authentication guarantee entirely. Never derive the IV from any deterministic input.
- **`ciphertext + auth tag`**: AES-256-GCM output. The 16-byte authentication tag is appended by the Web Crypto API and must be present for decryption.

### Key rotation

The KMS issues a new key version for the table's key ID. New writes immediately use the new version. Old rows are re-encrypted in a background job: read each row, parse `keyVersion` from the envelope, decrypt with the old key via KMS, re-encrypt with the new key, write back. The old key is retained read-only in the KMS until the background job confirms zero rows with the old `keyVersion`. Only then is the old key destroyed. No application downtime, no single large transaction.

---

## Analytics Tier

- Events written to `calypso_analytics` via the `analytics_w` role — this role has no read path back to `calypso_app`
- Events attributed to a rotating session pseudonym, not a user ID
- The mapping between session pseudonym and user identity exists only in `calypso_app` and is never exported
- Differential privacy: Laplace noise applied on all aggregation exports. Privacy budget (epsilon) tracked per dataset in `calypso_app`; the analytics export layer checks and decrements atomically before executing the query. Budget exhaustion returns a structured error — it does not silently reduce noise
- Events signed using a session-derived HMAC key (see `AnalyticsEvent` interface below), validated server-side before storage
- Analytics store uses `INSERT ... ON CONFLICT (event_id) DO NOTHING` for idempotent writes — the event pipeline is at-least-once; duplicates are discarded at the store

---

## Audit Logging

- Every sensitive data access logged before the access executes — if the audit write fails, the data read is denied
- Written via the `audit_w` role: `INSERT`-only, no `UPDATE`, no `DELETE`, no `TRUNCATE`
- Separate encryption key from the data it audits
- `audit_w` credentials are held only by the audit writer module — no other code path in the application can obtain them
- Log entries include: `action`, `actorId`, `actorKind`, `resourceType`, `resourceId`, `timestamp`, `result`

### Backup

Audit log backed up independently from `calypso_app`. A backup of `calypso_app` must never include the audit log — they are separate databases. Audit log replication to append-only cold storage is required before V1.

---

## Package Structure

```
/packages/data
  /db              # Three connection pools (app, analytics, audit), migration runner
  /crypto          # AES-GCM encrypt/decrypt, HKDF key derivation
  /kms             # KMS client abstraction (Vault, AWS KMS, GCP KMS)
  /analytics       # Analytics event writer, pseudonymization, DP noise + budget
  /audit           # Audit log writer (append-only, audit_w credentials only)
/apps/server
  /migrations/     # Numbered SQL migration files
  /db.ts           # Connection pool initialization and export
/infra
  /docker-compose.yml   # PostgreSQL + Vault for local dev
  /init.sql             # Database + role creation (run once on first container start)
/packages/core
  /types/data.ts   # Database record types, event types
```

---

## Core Interfaces

```typescript
// KMS client abstraction
interface KMSClient {
  encrypt(plaintext: Uint8Array, keyId: string): Promise<Uint8Array>;
  decrypt(ciphertext: Uint8Array, keyId: string): Promise<Uint8Array>;
  rotateKey(keyId: string): Promise<{ newKeyId: string }>;
}

// Encrypted field envelope — what is stored in the database column.
// Format: base64url(keyVersion || iv || ciphertext)
// See "Ciphertext envelope format" section above for byte layout.
interface FieldEncryptor {
  // Generates a fresh random IV, fetches the current key version from KMS,
  // encrypts, and returns the full base64url envelope.
  encryptField(value: string, table: string): Promise<string>;
  // Parses keyVersion from the envelope, fetches the matching key from KMS,
  // decrypts. Works on both current-key and old-key ciphertext — rotation
  // does not require all rows to be re-encrypted before the old key is retired.
  decryptField(ciphertext: string, table: string): Promise<string>;
}

// Analytics event (pseudonymized, signed)
interface AnalyticsEvent {
  type: string;
  payload: Record<string, unknown>;
  sessionPseudonym: string;  // rotating session pseudonym, never a user ID
  eventId: string;           // UUIDv4 — used for idempotent INSERT ON CONFLICT
  timestamp: number;
  // signature: base64url(HMAC-SHA256(sessionSigningKey, type + eventId + timestamp + payload))
  //
  // sessionSigningKey: 32-byte key derived per session via HKDF:
  //   HKDF(salt=serverHmacSecret, ikm=jti, info="analytics-signing", length=32)
  // Computed server-side at session creation and sent to the client as a
  // separate HTTP-only cookie scoped to the analytics endpoint only.
  // Expires when the session expires — no separate key lifecycle.
  signature: string;
}

// Audit log entry
interface AuditEntry {
  action: string;
  actorId: string;
  actorKind: 'user' | 'agent' | 'system';
  entityType: string;
  entityId: string;
  timestamp: number;
  result: 'allowed' | 'denied';
}

// Typed query function pattern
type QueryFn<TParams, TResult> = (params: TParams) => Promise<TResult[]>;
```

---

## Dependency Justification

| Package | Reason | Buy or DIY |
|---|---|---|
| `postgres` (npm) | PostgreSQL wire protocol client for Node/Bun — tagged template literal API produces parameterized queries by default, making string-concatenation injection structurally difficult | Buy |
| AES-256-GCM encrypt/decrypt | Web Crypto API (`crypto.subtle`) covers this natively | DIY |
| Key derivation (HKDF) | Web Crypto API covers this natively | DIY |
| ORM (Prisma, TypeORM, Drizzle) | Adds schema file, generation step, runtime abstraction; agents write SQL directly | Do not buy |
| Differential privacy library | Laplace noise addition is ~20 lines. **Caveat:** privacy budget tracking (epsilon accounting per dataset, atomic decrement, exhaustion enforcement) is an additional ~100 lines of DB-backed state in `calypso_app` — not a library concern, but it must be built before DP is considered active | DIY |
| KMS SDK (AWS/GCP/Vault) | Required for cloud KMS and Vault integration; use the official thin client for the target provider | Buy (when needed) |
| `bun:sqlite` | No longer used | Do not use |

---

## Antipatterns (TypeScript/PostgreSQL-Specific)

- **ORM as safety blanket.** Prisma/Drizzle "prevent SQL injection" — parameterized queries do that. The `postgres` client's tagged template syntax enforces parameterization at the call site. The ORM adds a generation step and a runtime abstraction layer between the engineer and the database.

- **Shared database module in the browser.** Importing a DB utility into the browser bundle "just for types" creates an import path to runtime database calls. The bundler may not tree-shake it cleanly.

- **Logging decrypted objects in error handlers.** `catch (e) { log(user) }` dumps decrypted PII. Log IDs and error codes only.

- **Environment variables for encryption keys.** Keys in `.env` are readable by any process on the host and often committed accidentally. Use a KMS client; never hold key material in application config.

- **Single database role for app and audit.** If `app_rw` can write to the audit table, a compromised application can cover its tracks. The `audit_w` role exists precisely to prevent this — its credentials are held only by the audit writer module.

- **Analytics and transactional tables in the same database.** Even in separate schemas on the same PostgreSQL database, a single compromised role or a missing `REVOKE` can bridge the boundary. The separation is enforced at the database level, not the schema level.

- **Static or missing IV in AES-GCM.** AES-256-GCM requires a unique 12-byte IV per encryption call. A static or deterministically derived IV breaks GCM's authentication guarantee. Always generate `crypto.getRandomValues(new Uint8Array(12))` per call. See the envelope format above.

- **No key version in stored ciphertext.** Storing raw ciphertext without a key version makes rotation impossible without a full table re-encryption cutover. The `keyVersion` prefix in the envelope format makes rotation a background migration.
