
# Auth — Calypso TypeScript Implementation

> This document is the Calypso TypeScript reference implementation for the [Auth Blueprint](../blueprints/auth-blueprint.md). The principles, threat model, and patterns in that document apply equally to other stacks. This document covers the concrete realization using TypeScript, Bun, PostgreSQL, and Web Crypto.

---

## Database placement

All auth data lives in `calypso_app`, accessed via the `app_rw` role. This includes: passkey credentials, token revocation list, agent registry, recovery shards, and the DP budget table. Auth data is never written to `calypso_analytics` or `calypso_audit` (except that auth events — login, logout, failed attempt — are written to `calypso_audit` via the `audit_w` role, the same as any other audit log entry).

---

## Authentication

- **Passkey-first via WebAuthn (FIDO2).** Server stores the public key and credential ID only — never a password hash.
- **Challenge-response flow:** server generates a random challenge, client signs with the passkey, server verifies the signature against the stored public key.
- **Key recovery:** BIP-39 mnemonic encrypts a recovery shard stored server-side. Recovery requires the mnemonic plus a second factor.

## Token Management

- **Signing:** ES256 (ECDSA P-256) via Web Crypto API (`crypto.subtle.sign` / `crypto.subtle.verify`)
- **Algorithm pinning:** JWT header `alg` is verified to be `ES256` before validation; any other value is rejected
- **Token storage:** HTTP-only, Secure, SameSite=Strict cookie
- **Expiry:** Default 1 hour; refresh via rotation (new token issued on each refresh, old token invalidated immediately)
- **Revocation:** `jti` claim on every token; revocation list is a table in `calypso_app` queried on every authenticated request. In-memory caching of revoked `jti` values is permitted with a TTL ≤ 60 seconds for standard revocations (logout). Security-critical revocations (account compromise, passkey change) bypass the cache: the middleware performs a direct DB read on the next request after revocation. Revocation entries expire from the table after the token's own `exp` timestamp — entries for expired tokens have no value and are cleaned up on a scheduled job.

## Agent Authentication

- Agents receive scoped tokens with an explicit `scopes` claim (e.g., `['analytics:read', 'schema:read']`)
- **Max TTL:** 24 hours; agents must re-authenticate daily
- Agent tokens are issued by a dedicated endpoint, not the user login flow
- **Shard keys:** agents get KMS-scoped keys that can only decrypt data within their authorized scope

## Session Management

- **Auth middleware:** single function extracts the cookie, verifies the JWT (pinned algorithm + expiry + `jti` revocation check), and attaches the user or agent to the request context
- Applied to all protected routes
- **Rate limiting:** login and register endpoints are rate-limited (configurable; default 10 req/min per IP — treat this as a development default; production values require deliberate review). IP-based rate limiting degrades behind shared NAT (offices, mobile carriers); before V1, add per-username rate limiting in addition to per-IP, and implement progressive delay (exponential backoff) rather than hard cutoff.

## Agent Scope Enforcement

Token issuance alone does not enforce scope. Every protected route that requires a specific scope must check the token's `scopes` claim against the required scope. This check happens in the auth middleware, not in the route handler — route handlers must not contain scope logic.

```typescript
// Required scope declared at route registration
function requireScope(required: string) {
  return (req: Request, ctx: RequestContext): Response | null => {
    const token = ctx.token; // attached by auth middleware
    if (!token.scopes.includes(required)) {
      return new Response('Forbidden', { status: 403 });
    }
    return null; // continue
  };
}

// Usage: scope check runs before the handler
router.get('/analytics/summary',
  requireScope('analytics:read'),
  handleAnalyticsSummary
);
```

A route without a `requireScope` call is accessible to any valid token regardless of `kind`. This must be treated as a misconfiguration, not a valid open route. Routes that are intentionally unauthenticated are explicitly marked with a `public()` decorator so the absence of `requireScope` is visible and deliberate.

---

## Package Structure

```
/packages/auth
  /passkey         # WebAuthn credential registration and assertion
  /jwt             # Token issuance, validation, revocation list
  /agent-auth      # Agent token issuance and scope validation
  /middleware       # Auth middleware for route protection
/apps/server
  /routes/auth.ts  # Login, logout, register, token refresh, agent token endpoints
/packages/core
  /types/auth.ts   # Auth-related types (token payload, scopes, passkey credential)
  /types/user.ts   # User type shared between server and client
```

## Core Interfaces

```typescript
// Passkey credential stored server-side
interface PasskeyCredential {
  credentialId: string;
  publicKey: Uint8Array;
  userId: string;
  createdAt: number;
}

// JWT token payload
interface TokenPayload {
  sub: string;        // user or agent ID
  jti: string;        // unique token ID for revocation
  scopes: string[];   // access scopes
  exp: number;        // expiry timestamp
  iat: number;        // issued-at timestamp
  kind: 'user' | 'agent';
}

// Agent token request
interface AgentTokenRequest {
  agentId: string;
  requestedScopes: string[];
}

// Recovery shard (encrypted, stored server-side)
interface RecoveryShard {
  userId: string;
  encryptedShard: Uint8Array;  // AES-256-GCM encrypted under key derived from BIP-39 mnemonic via HKDF
  // secondFactorKind distinguishes what secondFactorVerifier represents:
  // 'backup-code' → Argon2id hash of a one-time printed backup code (32 random bytes, base32 encoded)
  // 'hardware-key' → SHA-256 of the hardware key's credential ID (public, used for lookup only)
  secondFactorKind: 'backup-code' | 'hardware-key';
  // For 'backup-code': Argon2id hash of the printed code (never store the code itself)
  // For 'hardware-key': SHA-256 of the credential ID (used to locate the key, not for auth alone)
  secondFactorVerifier: string;
}
```

## Dependency Justification

| Package | Reason | Buy or DIY |
|---|---|---|
| `@simplewebauthn/server` | FIDO2 protocol is complex and security-critical; `@simplewebauthn/server` is the standard Node/Bun library — actively maintained, full FIDO2 conformance, no native dependencies | Buy |
| JWT sign/verify | ~50 lines with Web Crypto (`crypto.subtle.sign` with ECDSA) | DIY |
| BIP-39 mnemonic generation | BIP-39 is the cryptocurrency wallet standard for high-entropy human-readable mnemonics. It is used here not for its blockchain origins but because it provides a standardized 2048-word list, well-specified entropy-to-mnemonic encoding, and audited library implementations (`@scure/bip39` is recommended). The mnemonic is the recovery passphrase — entropy and auditability matter more than origin. | Buy |
| Auth SaaS (Auth0, Clerk) | Adds latency, cost, vendor lock-in; agent builds auth in minutes | Do not buy (unless mandated) |
| Rate limiting library | Simple token bucket is ~30 lines | DIY |

---

## Antipatterns (TypeScript/Web-Specific)

- **Tokens in localStorage.** Every script on the page can read `localStorage` — including injected scripts from XSS vulnerabilities. HTTP-only cookies are invisible to JavaScript by browser specification. Use them.

- **Auth SaaS by default.** For a standard passkey + JWT flow, authentication is roughly 300 lines of code with Web Crypto. The SaaS provider charges per user, adds an external dependency to the critical login path, and makes debugging auth failures require reading a third-party dashboard.

- **Importing auth modules in browser code.** Server-side auth primitives must never resolve in the browser bundle. Even "just for types" creates an import path that agents or developers may follow to add runtime auth calls client-side.

- **Symmetric signing (HS256) for production tokens.** HMAC requires sharing the secret with every service that verifies tokens. ES256 asymmetric signing means only the issuer holds the private key; verifiers use the public key.

- **In-memory-only revocation store.** An in-memory `Set` of revoked `jti` values is not durable across restarts and not shared across processes. The revocation table in `calypso_app` is the source of truth. In-memory caching is a read-performance optimization on top of the DB store, not a replacement for it.
