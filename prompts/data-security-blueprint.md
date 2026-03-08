# Data Security Blueprint

> [!IMPORTANT]
> This blueprint defines the security posture for all Calypso applications. It is not a hardening checklist — it is the foundational architecture. Security is not added after the fact; it is designed in from the first schema, the first API route, and the first agent key.

---

## Vision

Every Calypso application defaults to the security posture of:
- **Banking-grade authorization**: no data is accessible without explicit, auditable credential chains
- **HIPAA-grade privacy**: customer data is never visible in plaintext outside its originating session
- **Blockchain-grade adversarial hardening**: assume the database is public; assume the disk is stolen; assume admin credentials are burned

This is achievable in greenfield deployments because there are no brownfield trade-offs to honor. PassKey is the implementation baseline. Everything else is key recovery.

The threat model is not "prevent breaches." The threat model is "make a breach useless."

---

## Threat Model

Every security design choice is made against explicit adversary scenarios. A control that does not address a scenario below is decoration.

| Scenario | What must NOT be exposed |
|---|---|
| Admin account fully compromised | Customer data, analytical data, encryption keys |
| Server root access obtained | Admin credentials, database plaintexts, customer identities |
| Disk image or backup exfiltrated | Database contents, user records, session material |
| Ransomware encrypts the server | Historical analytical state (cold-path backups must survive) |
| Rogue agent gains execution | Direct customer data, other agents' key shards |
| Network traffic intercepted | Any non-public data in transit |
| Log files exfiltrated | PII, secrets, query contents, session tokens |

If a proposed design cannot demonstrate containment of every row above, it is incomplete.

---

## The Onion: Encryption Layers

Data is encrypted at every layer. Each layer is independently keyed. Compromise of one layer key does not yield plaintext at any other layer.

```
┌───────────────────────────────────────────┐
│  Layer 5: Disk Encryption (LUKS / BitLocker)  │  KMS key or TPM-sealed
│  ┌─────────────────────────────────────┐   │
│  │  Layer 4: Database Encryption       │   │  DB-level key, rotated monthly
│  │  ┌───────────────────────────────┐  │   │
│  │  │  Layer 3: Row/Field Encrypt.  │  │   │  Per-table AES-256-GCM keys
│  │  │  ┌─────────────────────────┐  │  │   │
│  │  │  │  Layer 2: User Data     │  │  │   │  User-session-derived key
│  │  │  │  ┌─────────────────┐    │  │  │   │
│  │  │  │  │  Layer 1:       │    │  │  │   │  User-signed, user-keyed
│  │  │  │  │  Analytical     │    │  │  │   │
│  │  │  │  │  Data           │    │  │  │   │
│  │  │  │  └─────────────────┘    │  │  │   │
│  │  │  └─────────────────────────┘  │  │   │
│  │  └───────────────────────────────┘  │   │
│  └─────────────────────────────────────┘   │
└───────────────────────────────────────────┘
```

### Layer 5 — Disk Encryption

- All disks encrypted at rest (LUKS on Linux, TPM-sealed keys for unattended boot where required)
- Key management via a hardware-backed KMS (cloud HSM, or open-source Vault equivalent)
- No unencrypted disks in any environment, including dev

### Layer 4 — Database Encryption

- Transparent Database Encryption (TDE) or equivalent at the engine level
- Database-level key is rotated on a defined schedule (default: 30 days)
- Database key is stored in KMS, not on the database host
- Backup files are encrypted with a separate backup key; backup key rotation is independent of live key

### Layer 3 — Row / Field Encryption

- Sensitive columns (PII, financial, health) are encrypted at the application layer before insert, using AES-256-GCM
- Each table class has its own key (user table key ≠ order table key ≠ audit log key)
- The application holds no keys in memory longer than the request lifetime
- Key derivation uses HKDF from a root secret stored in KMS; the root secret never touches the application process

### Layer 2 — User Data

- Customer records are encrypted with a key derived from the user's own authentication material
- The server cannot decrypt user data without an active, authenticated user session
- Session keys are ephemeral: they are derived on login and discarded on logout or expiry
- Admin roles can access user records only via a logged, audited re-encryption path using their own delegated key — not a master decrypt

### Layer 1 — Analytical Data

- Analytical data (usage events, metrics, behavioral data) can only be *created* inside an authenticated user session
- Each analytical record is signed with the user's session key before transmission to the server
- The server accepts analytical writes only with a valid signature; unsigned analytics are dropped, not accepted
- Aggregated analytical state is stored in a separate encrypted store with its own key chain

---

## Authentication: PassKey-First

PassKey (FIDO2 WebAuthn) is the default and only primary authentication mechanism. There are no passwords.

### PassKey Baseline

- User registration issues a FIDO2 credential backed by a platform authenticator (Touch ID, Face ID, Windows Hello) or a hardware key (YubiKey)
- The server stores only the public key and credential ID — never a password hash, never a secret
- Authentication requires a signed challenge; replay attacks are impossible by construction
- All session tokens are short-lived (default: 1 hour), non-renewable without re-authentication

### Key Recovery (Not Password Reset)

Because there are no passwords, recovery is a key recovery operation:

1. At enrollment, the user generates a recovery passphrase (BIP-39 mnemonic or equivalent) that encrypts a recovery shard
2. The recovery shard is stored server-side, encrypted under the passphrase — the server cannot decrypt it
3. Recovery requires: passphrase + a second factor (backup device, recovery code) — not just email link
4. Recovery events are logged and trigger out-of-band notification to all enrolled devices

### Session Tokens

- JWTs issued on successful WebAuthn assertion
- Algorithm pinned to ES256 (ECDSA P-256); RS256 and HS256 are rejected
- Claims: `sub` (user ID), `iat`, `exp` (1 hour), `jti` (unique nonce — logged for replay detection)
- Stored in HttpOnly, Secure, SameSite=Strict cookies only; never `localStorage`
- Server maintains a `jti` revocation list; logout is immediate and verifiable

---

## Key Management

### Key Hierarchy

```
Root KMS Key (HSM-backed, never exported)
    └── Application Master Key (rotated quarterly)
            ├── Database Encryption Key (rotated monthly)
            ├── Backup Encryption Key (rotated monthly, independent)
            ├── Table Keys (per-table, rotated on schema change or incident)
            │       ├── users_key
            │       ├── orders_key
            │       └── events_key
            └── Agent Shard Keys (per-agent, rotated on agent deregistration)
                    ├── agent:analytics-001_key
                    └── agent:reporting-002_key
```

### Key Rotation

- All key rotations are zero-downtime: new key encrypts new writes; old key decrypts existing reads until re-encryption is complete
- Re-encryption jobs run as background workers; progress is logged; they are idempotent and resumable
- Rotation events are written to an append-only audit log that is itself encrypted under a dedicated audit key

### Rekeying Procedure

1. Generate new key in KMS
2. Re-encrypt all affected rows in batches (never in a single transaction that could lock the table)
3. Flip the `active_key_id` pointer in the key registry
4. Archive the old key (retained for 90 days for forensic access, then destroyed)
5. Write rotation event to audit log

---

## Agent Authorization Model

Agents are not administrators. Agents are constrained participants with narrow, auditable access.

### Principles

- An agent never holds a master key or a user session key
- An agent holds a shard key: a key that decrypts only the aggregated, anonymized view it needs to do its job
- An agent cannot access raw customer records; it can only read schema definitions and pre-aggregated analytical tables
- Agents can write new analytical processing code (transformations, new aggregations); they cannot write to customer-facing tables directly

### Agent Key Lifecycle

1. Agent is registered by a human operator; operator signs the agent's public key
2. KMS issues a shard key scoped to the agent's declared access set (table list + operation list)
3. Shard key has a TTL (default: 24 hours); agents must re-authenticate daily
4. Agent key revocation is immediate: shard key is added to the revocation list; no grace period

### Shared Secrets and Shamir's Secret Sharing

For operations requiring elevated access (e.g., emergency data export, root-level rekeying), a Shamir Secret Sharing scheme is used:

- The sensitive operation key is split into N shards; M-of-N shards are required to reconstruct it (e.g., 3-of-5)
- Each shard is held by a distinct human operator (or hardware device)
- No single person or agent can unilaterally authorize a privileged operation
- Shard assembly is logged and triggers out-of-band notification to all shard holders

### What Agents Can Do

| Operation | Allowed |
|---|---|
| Read aggregated analytics table | Yes (via shard key) |
| Read schema definitions | Yes |
| Write new transformation code | Yes (reviewed and deployed via CI) |
| Read raw customer records | No |
| Write to customer-facing tables | No |
| Access session tokens or session keys | No |
| Request new shard keys | No (operator-only) |
| Invoke re-encryption jobs | No (operator-only) |

---

## Analytical Data Architecture

Analytics is a separate data tier. It is physically and cryptographically isolated from transactional customer data.

### Data Flow

```
User Session
    │  (user-signed analytical event)
    ▼
/api/events  [validates signature, drops unsigned]
    │
    ▼
Events Store  [Layer 1 encrypted, append-only]
    │
    ▼  (background aggregation workers — no user keys held)
Aggregated Analytics Store  [Layer 1 encrypted, agent-readable via shard key]
    │
    ├──▶ Real-time dashboard queries (agent reads aggregated view)
    └──▶ Strategic/agentic analytics (agent reads aggregated view, writes new code)
```

### Guarantees

- Aggregation workers run without user session keys; they operate only on pre-existing aggregated tables
- A worker compromise exposes only aggregated statistics — no raw user data, no customer PII
- New aggregation logic proposed by agents is committed to the repository and deployed through CI; it does not execute ad-hoc in production
- Analytical stores are backed up independently of transactional stores, with independent encryption keys

---

## Design Patterns

### Pattern 1: Encrypt-Before-Insert

Application code never stores plaintext sensitive fields. The encryption step is in the domain service, not the database driver.

```typescript
// Correct: encrypt at domain layer
const encryptedEmail = await kms.encrypt(user.email, tableKey('users'));
await db.insert({ ...user, email: encryptedEmail });

// Wrong: store plaintext and rely on DB-level encryption alone
await db.insert({ ...user, email: user.email });
```

### Pattern 2: Key-Per-Table, Not Key-Per-Row

Deriving a unique key per row creates re-keying surface proportional to the table size. Use per-table keys unless regulatory requirements mandate per-row (e.g., HIPAA-covered entities with multi-tenant row isolation).

### Pattern 3: Signed Analytics at the Edge

Analytical events are signed client-side using the session-derived key before transmission. The server validates the signature before accepting the write.

```typescript
// Client: sign the event inside the session
const signature = await sessionKey.sign(JSON.stringify(event));
await fetch('/api/events', { body: JSON.stringify({ event, signature }) });

// Server: verify before accepting
const valid = await verifySignature(event, signature, userPublicKey);
if (!valid) return Response.json({ error: 'invalid signature' }, { status: 400 });
```

### Pattern 4: Audit-Log-First for Privileged Operations

Any operation that touches a key, elevates a role, or accesses a customer record must write to the audit log *before* executing. The audit log entry is committed in the same transaction as the operation, or the operation does not proceed.

### Pattern 5: Short-Lived Agent Tokens with Scope Claims

Agent JWTs carry explicit scope claims. Server middleware validates scope on every request — not just on login.

```typescript
// Middleware validates scope on every route
if (!agentToken.scopes.includes('analytics:read')) {
  return Response.json({ error: 'forbidden' }, { status: 403 });
}
```

### Pattern 6: Homomorphic Computation for Aggregations

Where aggregation over encrypted data is needed without decryption (e.g., summing encrypted balances), use partial homomorphic encryption (PHE). PHE allows addition over ciphertexts; the server never holds plaintext values.

- Use CKKS or BFV scheme for numerical aggregations
- PHE operations are expensive: batch them; do not use them for per-request logic
- PHE is a Buy, not DIY: use an audited library (e.g., `node-seal`, `tfhe-rs` via WASM binding)
- Document every PHE callsite in `docs/dependencies.md` with threat model justification

### Pattern 7: Zero-Knowledge Proofs for Authorization Without Disclosure

For authorization assertions that must not reveal the underlying credential (e.g., "prove you are over 18 without revealing your birthdate"), use a ZK proof.

- ZKP is a last-resort primitive; most authorization can be done with signed claims and scoped tokens
- When ZKP is used, document the circuit and its trust assumptions in `docs/security/zkp-circuits.md`
- Never roll a ZK circuit from scratch; use an audited framework (e.g., `snarkjs`, `circom`)

---

## Plausible Architectures

### Architecture A: Single-Node Greenfield (Default for Alpha/Beta)

```
Internet
   │ TLS 1.3
   ▼
Bun Server (systemd, bare metal)
   ├── PassKey auth (WebAuthn)
   ├── JWT issuance (ES256, 1hr TTL)
   ├── KMS client (Vault or cloud KMS)
   └── Encrypted SQLite (sqlcipher) [Alpha]
       → Encrypted PostgreSQL [Beta/V1]

Backups: encrypted nightly, uploaded to cold storage (S3 / B2)
Audit log: append-only table, separate encryption key
Agent access: read-only shard key to analytics schema only
```

### Architecture B: Multi-Tenant with Row-Level Isolation (V1+)

```
Internet
   │ TLS 1.3
   ▼
Bun API Gateway
   ├── Tenant isolation middleware (validates tenant claim in JWT)
   ├── Per-tenant row encryption keys (fetched from KMS per request)
   └── PostgreSQL with row security policies (RLS)

Analytical tier: separate read replica, aggregated tables only
Agent tier: scoped to aggregated tier; tenant data never visible
KMS: HSM-backed, key per tenant
```

### Architecture C: Agentic Analytics Pipeline (V1+)

```
Transactional DB (customer data, fully encrypted)
   │ (aggregation worker — no session keys, read from event store only)
   ▼
Event Store (append-only, signed events, Layer 1 encrypted)
   │ (background aggregation workers)
   ▼
Aggregated Analytics DB (Layer 1 encrypted, agent shard key access)
   │
   ├── Real-time queries (agent reads via shard key)
   └── Strategic analysis (agent proposes new transformations → CI deploy)
```

---

## Calypso TS Implementation

### Package Structure

```
/packages/security
  /kms           # KMS client abstraction (Vault, AWS KMS, GCP KMS)
  /crypto        # Encrypt/decrypt, key derivation, signature primitives
  /passkey       # WebAuthn credential registration and assertion
  /jwt           # Token issuance, validation, revocation list
  /audit         # Audit log writer (append-only, signed entries)
  /agent-auth    # Agent shard key issuance and validation
  /phe           # Partial homomorphic encryption wrappers (Buy: node-seal)
```

### Core Interfaces

```typescript
// Key lifecycle
interface KMSClient {
  encrypt(plaintext: Uint8Array, keyId: string): Promise<Uint8Array>;
  decrypt(ciphertext: Uint8Array, keyId: string): Promise<Uint8Array>;
  rotateKey(keyId: string): Promise<{ newKeyId: string }>;
}

// Session key — ephemeral, never serialized
interface SessionKey {
  sign(data: string): Promise<Uint8Array>;
  verify(data: string, signature: Uint8Array): Promise<boolean>;
  destroy(): void; // zeroes key material from memory
}

// Agent shard token
interface AgentToken {
  agentId: string;
  scopes: string[];       // e.g. ['analytics:read', 'schema:read']
  exp: number;            // unix timestamp, max 24h from issuance
  shardKeyId: string;     // KMS key ID scoped to this agent's access set
}

// Analytical event — must be signed before server accepts
interface AnalyticalEvent {
  type: string;
  payload: Record<string, unknown>;
  userId: string;
  timestamp: number;
  signature: string;      // base64url(sessionKey.sign(type + payload + timestamp))
}
```

### Dependency Justification

| Package | Reason to Buy | Justified |
|---|---|---|
| `node-seal` (PHE) | PHE implementation is 10k+ lines of cryptographic C++; DIY is catastrophically error-prone | Yes |
| `snarkjs` (ZKP) | ZK circuit compilation and proof generation; irreplaceable | Yes, when ZKP is required |
| WebAuthn server lib | FIDO2 protocol is complex and security-critical; no DIY | Yes |
| JWT parsing | < 50 lines DIY using `crypto.subtle`; no external dep needed | No — DIY |
| AES-GCM encrypt/decrypt | Web Crypto API (`crypto.subtle`) covers this natively | No — DIY |
| Key derivation (HKDF) | Web Crypto API covers this natively | No — DIY |

All security dependencies are documented in `docs/dependencies.md` with CVE monitoring enabled.

---

## Implementation Checklist

### Alpha Gate (no customer data in production without all items checked)

- [ ] Disk encryption verified on deployment target
- [ ] Database engine-level encryption configured and tested
- [ ] PassKey registration and assertion flow implemented and E2E tested
- [ ] JWT issued with ES256, 1hr TTL, `jti` revocation list operational
- [ ] Session tokens in HttpOnly, Secure, SameSite=Strict cookies only
- [ ] All sensitive columns encrypted at application layer before insert
- [ ] KMS client integrated; no keys hardcoded or in `.env`
- [ ] Audit log operational for all auth events and key operations
- [ ] Agent authentication implemented; agents have no access to customer tables
- [ ] `/api/events` validates signatures; unsigned events are dropped with 400
- [ ] `bun pm audit` shows zero high/critical findings
- [ ] Secret scan on git history returns no matches

### Beta Gate (external users, real data)

- [ ] Key rotation procedure documented and tested end-to-end
- [ ] Backup encryption verified: restoring from backup requires backup key, not live key
- [ ] Shamir Secret Sharing implemented for privileged operations
- [ ] Rate limiting on all auth endpoints (WebAuthn assertion, token refresh)
- [ ] Out-of-band notification on recovery events and shard assembly
- [ ] PHE wrappers tested for all numerical aggregations
- [ ] Penetration test or structured security review completed
- [ ] Incident response runbook written to `docs/security/incident-response.md`

### V1 Gate

- [ ] HSM-backed KMS in production (not software KMS)
- [ ] Automated key rotation operational (no manual rotation required)
- [ ] Zero-downtime rekeying tested and verified
- [ ] Audit log exported to immutable cold storage (write-once S3 or equivalent)
- [ ] Agent key TTL enforced; daily re-authentication verified in staging
- [ ] ZKP circuits (if used) audited by external reviewer

---

## Antipatterns

- **Encrypting at the database layer only and calling it "encrypted."** A database-level key on the same host as the database process provides no protection against root-level compromise. Each layer must be independently keyed.
- **Storing session keys in `localStorage`.** `localStorage` is readable by any JavaScript on the page. XSS = full session capture. HttpOnly cookies only.
- **Admin bypass routes.** There must be no endpoint where an admin credential grants direct plaintext access to customer data. Admins access customer data through the same delegated re-encryption path with the same audit trail.
- **Logging plaintext in error handlers.** A `catch (e) { log(user) }` that dumps the user object will log decrypted PII. Log IDs and error codes only; never log objects containing user fields.
- **Rotating keys without re-encrypting old data.** A new key that only applies to new writes is not rotation. Re-encryption of existing data is mandatory.
- **Using symmetric shared secrets for agent auth.** Shared secrets cannot be revoked per-agent. Use asymmetric keys; the agent holds the private key, the server validates against the registered public key.
- **PHE or ZKP as the first tool.** These are expensive and complex. Exhaust simpler designs (scoped tokens, aggregation tiers, data minimization) before reaching for homomorphic encryption or zero-knowledge proofs.
- **Treating dev and production key material as interchangeable.** Dev environments use dev keys generated locally. Production keys live in KMS and never touch a developer machine.
- **Accepting analytical events without signature validation.** An unsigned event endpoint is an open write path that can inject false data into the analytics tier. Validate or drop; never store unsigned.
- **Single-person approval for privileged operations.** Any operation that touches root keys or customer data exports requires M-of-N sign-off. There are no superman admin accounts.
