# Authentication & Authorization Blueprint

> [!IMPORTANT]
> This blueprint defines Calypso's authentication and authorization posture: how users and agents prove identity, how sessions are managed, and how access is scoped and governed. Read this before the [Data Blueprint](./data-blueprint.md) which covers persistence, encryption, and privacy.

---

## Vision

Every application that handles user data faces an authentication problem, and most solve it by inheriting someone else's: a managed identity provider, a password-plus-MFA flow copied from a tutorial, or an OAuth integration treated as a black box. These approaches share a structural flaw — they place the most security-critical surface of the application outside the team's understanding and control. When the provider has an outage, logins stop. When the provider changes its token format, sessions break. When a vulnerability is disclosed, the team waits for a patch it cannot inspect.

Calypso treats authentication as owned infrastructure. Identity verification, token issuance, and session lifecycle are first-class components of the application, not rented services. The default authentication mechanism is passkey-based (FIDO2 WebAuthn), which eliminates the largest class of credential attacks — phishing, credential stuffing, and password reuse — at the protocol level rather than through policy enforcement. There are no passwords to rotate, no password hashes to exfiltrate, and no reset flows that reduce security to the strength of an email inbox.

Agents — AI systems that act on behalf of the platform — are first-class participants in the authorization model, but they are not peers of human users. An agent receives scoped, short-lived credentials that grant access only to the specific resources its task requires. No agent holds a master key, and no agent credential outlives a single working day without explicit renewal. This constraint is not a limitation to be relaxed as the system matures; it is a permanent architectural boundary.

The cost of ignoring this blueprint is familiar and predictable: a single compromised credential grants unlimited lateral movement, a token algorithm mismatch enables forgery, a long-lived API key leaks into a log file and is not rotated for months, and an external provider outage locks every user out of a system that is otherwise fully operational. These are not exotic failure modes. They are the default outcome of treating authentication as a peripheral concern.

---

## Threat Model

Every design choice in this blueprint addresses at least one of these scenarios. A control that cannot be traced back to a row in this table is decoration.

| Scenario | What must be protected |
|---|---|
| Phished or stolen user credentials | User sessions, personal data, account integrity |
| Algorithm confusion in token verification (e.g., `alg: none` or HS256 substitution) | Token integrity; forged tokens must never be accepted |
| Compromised admin account | All user data, system configuration, encryption keys |
| Rogue AI agent exceeding its authorized scope | Customer records, write access to production entity types, other agents' credentials |
| Replay of intercepted authentication tokens | Session integrity; replayed tokens must not grant access |
| Credential stuffing or brute-force attacks against login endpoints | Account availability and integrity |
| Single insider unilaterally approving a privileged operation | Root key material, bulk data exports, key rotation |
| External authentication provider outage | Login availability; users must not be locked out of an operational system |
| Session token exposed via client-side script (XSS exfiltration) | Session material; tokens must not be readable by JavaScript |
| Agent credential leaked in logs or error output | Scoped access the agent held; blast radius must be bounded by scope and TTL |

---

## Core Principles

### Passkey-first, password-never

The system uses passkey-based authentication as the sole primary credential mechanism — not as an optional upgrade alongside passwords. Passwords are the root cause of phishing, credential stuffing, and reuse attacks. Eliminating them is not a user-experience preference; it is a structural security decision. When the only credential is a cryptographic key pair where the private key never leaves the user's device, the entire class of server-side credential theft becomes impossible. There is no password hash to exfiltrate because no password exists.

### Tokens are opaque to browsers

Session tokens are stored in HTTP-only, secure, same-site-strict cookies and are never accessible to client-side JavaScript. The browser is an untrusted execution environment. Any token that JavaScript can read, an XSS payload can exfiltrate. Moving tokens out of JavaScript's reach eliminates the highest-impact consequence of cross-site scripting — session hijacking — without requiring the application to be XSS-free (which it should be, but defense in depth does not depend on that assumption).

### Agent credentials are scoped and short-lived

Every agent credential carries explicit scope claims and expires within 24 hours. An agent that needs access to analytics data receives a token that grants read access to the analytics schema and nothing else. An agent that needs to write transformation code receives a token scoped to that operation. Broad, long-lived agent tokens are not issued, regardless of convenience. The blast radius of a compromised agent credential is bounded by both scope and time: a leaked token grants narrow access for hours, not broad access forever.

### No single actor authorizes privileged operations

Operations that touch root key material, trigger bulk data exports, or modify the authentication infrastructure itself require M-of-N approval from distinct human operators. No administrator account, no matter how trusted, can unilaterally perform these operations. This is not a workflow preference — it is a cryptographic constraint enforced through secret sharing. A single compromised admin account is a serious incident; it must not be a total compromise.

### Authentication is self-hosted

The critical login path — credential verification, token issuance, session creation — runs on infrastructure the team owns and operates. External identity providers may participate in federated flows (enterprise SSO, for example), but they are never the sole authentication path. If the external provider goes down, users authenticate via the self-hosted passkey flow. This principle does not prohibit integration with external systems; it prohibits dependence on them for the ability to log in.

### Algorithm is pinned, not negotiated

Token verification accepts exactly one signing algorithm. The algorithm is configured at deployment time and is not read from the token header. Algorithm negotiation — where the server accepts whatever algorithm the token claims to use — is the root cause of algorithm confusion attacks, including the `alg: none` bypass and symmetric/asymmetric key substitution. Pinning the algorithm makes these attacks structurally impossible rather than relying on validation logic to catch them.

---

## Design Patterns

### Pattern 1: Passkey Authentication

**Problem:** Password-based authentication is vulnerable to phishing, credential stuffing, replay attacks, and server-side credential theft (hash exfiltration).

**Solution:** Use the WebAuthn protocol for all user authentication. During registration, the user's device generates an asymmetric key pair; the private key remains on the device (protected by biometrics or a hardware token), and the server stores only the public key and a credential identifier. During login, the server issues a random challenge; the device signs it with the private key; the server verifies the signature against the stored public key. No shared secret ever crosses the network. Replay is prevented because each challenge is unique and time-bound.

**Trade-offs:** Passkey support depends on the user's device and platform. Older browsers and operating systems may not support WebAuthn, which limits the addressable user base. Key recovery is more complex than password reset — there is no "forgot password" flow, only a key recovery operation (see Pattern 5). For platforms that must support legacy devices, a fallback mechanism (hardware security keys, for example) is necessary, and that fallback becomes the weakest link.

### Pattern 2: Pinned-Algorithm Token Verification

**Problem:** Token verification that reads the signing algorithm from the token header is vulnerable to algorithm confusion attacks — an attacker can force the server to verify a token using an unintended algorithm (e.g., switching from asymmetric to symmetric verification using the public key as the HMAC secret).

**Solution:** The server is configured with exactly one acceptable signing algorithm at deployment time. During verification, the server ignores the `alg` field in the token header and uses only its configured algorithm. Tokens bearing any other algorithm are rejected without further inspection. The signing key type and the verification algorithm are a matched pair; they are never derived from the token itself.

**Trade-offs:** Pinning a single algorithm means algorithm migration requires a coordinated deployment — both the issuer and all verifiers must be updated together, with a brief window where both old and new algorithms are accepted. This is operationally more complex than free negotiation, but the security benefit is categorical: an entire class of forgery attacks becomes impossible.

### Pattern 3: Scoped Agent Tokens

**Problem:** AI agents need programmatic access to platform resources, but granting them the same credentials as human users creates an unacceptable blast radius — a compromised agent credential would have the same access as a compromised user account, without the behavioral signals that help detect human account compromise.

**Solution:** Agents authenticate through a dedicated issuance path that produces tokens with explicit scope claims. Each scope claim names a resource and an operation (e.g., `analytics:read`, `transformations:write`). The token has a maximum TTL of 24 hours. Server-side middleware validates scope claims on every request, not just at issuance. An agent requesting access outside its declared scope receives a 403 regardless of token validity. Agent registration is a human-operator action; agents cannot self-register or request scope escalation.

**Trade-offs:** Short-lived, narrowly scoped tokens require agents to re-authenticate frequently and may need multiple tokens for multi-resource workflows. This adds latency and operational complexity to agent orchestration. For workflows that span many resources, the temptation is to issue a broader token — but that temptation is precisely what this pattern exists to resist.

### Pattern 4: M-of-N Privileged Operations

**Problem:** Privileged operations — root key access, bulk data exports, authentication configuration changes — are catastrophic if performed by a single compromised or malicious actor.

**Solution:** The key or credential required to perform a privileged operation is split into N shards using a secret sharing scheme (e.g., Shamir's Secret Sharing). M of N shards are required to reconstruct the key, where M > 1 and the shards are held by distinct human operators on separate devices. Shard assembly is logged, triggers out-of-band notifications to all shard holders, and is time-bounded — an assembled key is valid for a single operation and is discarded immediately after use.

**Trade-offs:** M-of-N introduces coordination overhead. Privileged operations cannot be performed by a single person, even in emergencies. This is by design, but it means the organization must maintain a sufficient pool of shard holders and have a process for shard holder unavailability. If M shard holders are not reachable, the operation cannot proceed. The values of M and N must balance security against operational availability.

**Recommended starting values:** For a small engineering team, 3-of-5 is a practical default — it tolerates two simultaneous unavailabilities while requiring three active participants for any privileged operation. 2-of-3 is the minimum acceptable; 2-of-2 provides no redundancy and should be avoided. Larger organizations with operational staff across time zones may use 3-of-7 to improve availability without weakening the threshold.

**Shard holder operational requirements:** Each shard must be held on a device that is not accessible to other shard holders — the security property is lost if all shards are on the same machine or the same shared drive. Hardware security keys (YubiKey or equivalent) are the recommended shard storage medium for production; they prevent shard extraction even if the holder's computer is compromised. Shards must never be stored in shared password managers, email, or cloud sync services. Shard holder onboarding and offboarding (rekeying the entire secret, issuing new shards to the new set of holders) must be a documented procedure tested at least annually.

### Pattern 5: Key Recovery Without Passwords

**Problem:** In a passkey-only system, losing access to all registered devices means losing access to the account. There is no password to fall back on, and email-based reset flows reintroduce the credential weakness that passkeys eliminate.

**Solution:** At enrollment, the user generates a recovery passphrase (a high-entropy mnemonic, for example). This passphrase encrypts a recovery shard that is stored server-side — the server holds the encrypted shard but cannot decrypt it. Recovery requires the passphrase plus a second factor (a backup device, a printed recovery code, or a hardware key). The recovery flow re-enrolls a new passkey rather than restoring the old one. Recovery events are logged and trigger notifications to all previously enrolled devices.

**Trade-offs:** Recovery passphrases can be lost or forgotten, and unlike passwords, there is no fallback behind the fallback. Users must be clearly informed that the recovery passphrase is a key, not a convenience — losing it and all registered devices means permanent account loss. This is the honest trade-off of eliminating passwords: recovery becomes harder, but the day-to-day attack surface shrinks dramatically.

**Operational requirements for recovery at scale:** The recovery flow is a high-value social engineering target — an attacker who can convince support staff to initiate recovery on their behalf bypasses the entire passkey model. Three controls are required: (1) recovery can only be self-initiated by the account holder, never initiated by support staff on a user's behalf; (2) recovery attempts are rate-limited per account (no more than three attempts per 24-hour window) and trigger immediate out-of-band notification to all enrolled devices and the account's verified contact; (3) if a support-assisted path ever becomes necessary, it requires M-of-N approval from operators under the same constraint as privileged operations (Pattern 4), with a mandatory waiting period and notification to the account holder before execution.

### Pattern 6: HTTP-Only Session Cookies

**Problem:** Session tokens stored in browser-accessible locations (localStorage, sessionStorage, JavaScript variables) can be exfiltrated by cross-site scripting attacks.

**Solution:** After successful authentication, the server issues the session token as an HTTP-only cookie with the Secure flag (HTTPS only) and SameSite=Strict (no cross-origin transmission). The token never appears in a response body, URL parameter, or any location accessible to client-side scripts. The browser sends the cookie automatically with same-origin requests; the application JavaScript never handles the token directly.

**Trade-offs:** HTTP-only cookies require the authentication server and the resource server to share an origin (or a carefully configured domain hierarchy). Cross-origin API architectures — where the frontend and backend are on different domains — must use a same-origin proxy or a token relay, adding deployment complexity. SameSite=Strict also prevents the cookie from being sent on cross-site navigation, which can break flows that start from external links.

**Cross-origin deployment pattern:** When the frontend (`app.example.com`) and backend (`api.example.com`) are on separate subdomains, configure cookies with `Domain=example.com` and `SameSite=Strict`. Both origins share the same registrable domain, so the browser sends the cookie to the API. Do not use `SameSite=None` to work around this — it requires `Secure` and re-enables cross-site transmission. If the frontend and backend cannot share a registrable domain, deploy a same-origin token relay: a thin server-side route at `app.example.com/api` that proxies authenticated requests to the backend, holds the session cookie itself, and forwards a short-lived internal token to the backend in a request header. The client never touches a token in either case.

### Pattern 7: Token Revocation Store

**Problem:** JWTs are stateless by design — a token that has been issued is valid until expiry regardless of what happens after issuance. A user who logs out, changes their passkey, or whose account is suspended cannot have their existing tokens invalidated without a server-side revocation mechanism.

**Solution:** Every issued token carries a `jti` (JWT ID) claim — a unique identifier generated at issuance. The server maintains a revocation store keyed on `jti`. Auth middleware checks the revocation store on every request before accepting a token as valid. A revoked `jti` returns 401 regardless of token signature and expiry. Revocation entries expire from the store after the token's own `exp` timestamp passes — there is no value in retaining a revocation entry for a token that would be expired anyway.

**Consistency model:** The revocation store must be consistent across all server instances. An in-memory `Set` is not acceptable for any deployment with more than one process — a token revoked against instance A is still valid against instance B. The revocation store must be a shared, durable backing store (durable graph entities, Redis with persistence) that all instances query. For a single-process deployment, in-memory with synchronous persistence on every write is acceptable; the constraint must be documented and enforced as a deployment invariant.

**Unavailability:** If the revocation store is unreachable, the auth middleware must fail closed — it must deny requests rather than allow them through on the assumption that the token is not revoked. An unreachable revocation store is an operational incident, not a graceful degradation scenario.

**Trade-offs:** A synchronous revocation check on every request adds a database round-trip to every authenticated call. For high-throughput services, a short-lived local cache (TTL ≤ 60 seconds) with a cache-miss fallback to the store is acceptable, with the understanding that a revoked token may remain valid for up to one cache TTL. This window must be weighed against the threat model — for most session revocations (logout) a 60-second window is acceptable; for security-critical revocations (account compromise, passkey change) the cache should be bypassed or the TTL set to zero.

---

## Plausible Architectures

### Architecture A: Single-Application Passkey Auth (small team, single product)

```
┌──────────────────────────────────────────────────┐
│                  Application Server               │
│                                                    │
│  ┌──────────┐  ┌──────────────┐  ┌─────────────┐ │
│  │ Passkey   │  │ Token        │  │ Auth        │ │
│  │ Store     │  │ Issuer       │  │ Middleware   │ │
│  │ (typed    │  │ (pinned alg, │  │ (validates  │ │
│  │  entities)│  │  scoped      │  │  token +    │ │
│  └─────┬─────┘  │  claims)     │  │  scope on   │ │
│        │        └──────┬───────┘  │  every req) │ │
│        │               │          └──────┬──────┘ │
│        └───────┬───────┘                 │        │
│                │                         │        │
│         ┌──────▼──────┐          ┌───────▼──────┐ │
│         │ Property    │          │ Resource     │ │
│         │ Graph (PG)  │          │ Handlers     │ │
│         └─────────────┘          └──────────────┘ │
└──────────────────────────────────────────────────┘
           │
    ┌──────▼──────┐
    │ Key Store   │
    │ (signing    │
    │  keys, HSM) │
    └─────────────┘
```

All authentication components — passkey verification, token issuance, session management — run within the same application process. The Passkey Store holds registered public keys as `passkey_credential` entities in the property graph. The Token Issuer creates tokens with a signing key held in an external Key Store (hardware-backed in production). Auth Middleware runs on every protected route.

**Trade-offs vs. other architectures:** Simplest to deploy and reason about. No network boundary between auth and application logic, which reduces latency but means a vulnerability in the application layer has direct access to the auth components. Does not support federated identity or multi-product SSO without significant rework.

### Architecture B: Federated Identity with Self-Hosted Fallback (enterprise, multi-tenant)

```
┌─────────────────────┐     ┌──────────────────────┐
│  External Identity  │     │  Self-Hosted Passkey  │
│  Provider           │     │  Auth Service         │
│  (enterprise SSO)   │     │  (fallback path)      │
└────────┬────────────┘     └──────────┬────────────┘
         │  identity assertion          │  passkey assertion
         └──────────┬──────────────────┘
                    ▼
         ┌──────────────────┐
         │  Token Gateway   │
         │  (normalizes     │
         │   assertions,    │
         │   issues scoped  │
         │   session tokens)│
         └────────┬─────────┘
                  │  uniform session token
                  ▼
         ┌──────────────────┐
         │  Resource Server │
         │  (Auth Middleware │
         │   validates      │
         │   token + scope) │
         └──────────────────┘
```

Enterprise tenants authenticate through their own identity provider (SAML, OIDC). The Token Gateway normalizes the external assertion into the platform's internal token format. If the external provider is unavailable, users fall back to the self-hosted passkey path — the same passkey flow as Architecture A, running as an independent service. The Resource Server sees only the platform's internal token and does not know or care which path produced it.

**Trade-offs vs. other architectures:** Supports enterprise SSO requirements without surrendering control of the authentication surface. More complex to operate: two authentication paths must be maintained, tested, and monitored. The Token Gateway is a critical single point — it must be highly available and its normalization logic must be rigorously tested to prevent assertion injection.

### Architecture C: Agent-Aware Auth Gateway (agentic platform)

```
                    ┌──────────────────┐
                    │   Auth Gateway   │
                    │                  │
         ┌─────────┤   Routes by      ├──────────┐
         │         │   credential     │          │
         │         │   type           │          │
         │         └──────────────────┘          │
         ▼                                       ▼
┌─────────────────┐                  ┌────────────────────┐
│  Human Auth     │                  │  Agent Auth        │
│  Path           │                  │  Path              │
│                 │                  │                    │
│  Passkey Store  │                  │  Agent Registry    │
│  Token Issuer   │                  │  (public keys,     │
│  Session Mgmt   │                  │   declared scopes) │
│  (cookies,      │                  │  Scope Validator   │
│   refresh)      │                  │  Token Issuer      │
└────────┬────────┘                  │  (24h max TTL,     │
         │                           │   explicit claims) │
         │                           └─────────┬──────────┘
         │                                     │
         └──────────────┬──────────────────────┘
                        ▼
              ┌──────────────────┐
              │  Resource Server │
              │  (Auth Middleware │
              │   validates      │
              │   token + scope  │
              │   uniformly)     │
              └────────┬─────────┘
                       │
              ┌────────▼─────────┐
              │  Key Store       │
              │  (signing keys,  │
              │   agent shards,  │
              │   M-of-N keys)   │
              └──────────────────┘
```

The Auth Gateway is the single entry point for all authentication. It inspects the credential type and routes to the appropriate path: human users go through the passkey flow; agents go through the agent registry and scope validation. Both paths produce tokens in the same format — the Resource Server does not distinguish between human and agent tokens except through scope claims. The Agent Registry is a separate component where human operators register agents and declare their allowed scopes.

**Trade-offs vs. other architectures:** Purpose-built for platforms where AI agents are first-class participants. The Auth Gateway adds a network hop and a component to maintain, but it cleanly separates the human and agent authentication concerns. The agent path can be scaled, rate-limited, and monitored independently. Risk: if the gateway is compromised, both paths are affected — the gateway must be hardened as a critical component.

**Agent Registry access controls:** The Agent Registry holds the source of truth for which agents exist and what scopes they are authorized to hold. Write access to the registry is restricted to human operators authenticated through the human auth path — no agent or service account may create, modify, or delete registry entries. The registry's write endpoint requires a user token with an explicit `agent-registry:write` scope that is not included in any default operator role; it must be granted deliberately. All registry mutations are audit-logged with the operator identity, the previous state, and the new state. An agent whose registry entry is deleted or whose scopes are reduced must have its outstanding tokens immediately revoked — the Auth Gateway must check the registry on token issuance, not only at agent registration time.

---

## Implementation Checklist

### Alpha Gate

- [ ] Passkey registration flow implemented: users can enroll a platform authenticator or hardware key
- [ ] Passkey assertion flow implemented: users can log in with an enrolled passkey
- [ ] Token signing algorithm pinned at deployment configuration; tokens with unexpected algorithm rejected
- [ ] Session tokens issued as HTTP-only, Secure, SameSite=Strict cookies
- [ ] Token expiry enforced on every request; expired tokens return 401
- [ ] Token includes a unique identifier (nonce); revocation list checked on every request
- [ ] Auth middleware present on all protected routes; no unprotected route serves user data
- [ ] Agent authentication implemented with scoped tokens; agent tokens carry explicit scope claims
- [ ] Agent scope validated on every request by middleware, not just at token issuance
- [ ] Agent token TTL enforced at a maximum of 24 hours
- [ ] Rate limiting active on authentication endpoints (registration, assertion, token refresh)
- [ ] Authentication events (login, logout, failed attempt, registration) written to audit log

### Beta Gate

- [ ] Key recovery flow implemented and tested end-to-end: passphrase + second factor re-enrolls a new passkey
- [ ] Recovery events trigger out-of-band notification to all enrolled devices
- [ ] Token refresh rotation implemented: each refresh produces a new token and invalidates the old one
- [ ] M-of-N approval flow documented and tested for at least one privileged operation (e.g., signing key rotation)
- [ ] Shard assembly logged and time-bounded; assembled keys valid for single operation only
- [ ] Penetration test or structured security review completed on the authentication surface
- [ ] Agent re-registration flow tested: revoking and re-issuing agent credentials with new scopes
- [ ] Authentication configuration (algorithm, TTL, scope definitions) managed via deployment config, not runtime API
- [ ] Failed authentication attempts trigger progressive delays or temporary lockout

### V1 Gate

- [ ] Automated credential rotation operational for signing keys; no manual key rotation required
- [ ] Session revocation tested: revoking a session immediately prevents further access (no grace period)
- [ ] Agent re-authentication enforced daily in production; verified in monitoring
- [ ] Immutable authentication audit log operational: log entries cannot be modified or deleted
- [ ] Audit log exported to append-only cold storage on a defined schedule
- [ ] Incident response runbook written and tested for authentication compromise scenarios
- [ ] Federated identity flow tested if applicable (enterprise SSO with self-hosted fallback)
- [ ] All authentication error messages are generic; no information leakage about account existence or credential validity

---

## Antipatterns

- **Password as default with passkey as optional upgrade.** Offering passwords alongside passkeys means the system's security is bounded by the password path, not the passkey path. Attackers will always target the weakest credential type. If passwords exist, phishing works. Passkey-first means passkey-only for the primary credential.

- **Algorithm negotiation in token verification.** Reading the `alg` header from the token and using it to select the verification algorithm is the root cause of algorithm confusion attacks. The server must be told which algorithm to use at deployment time; it must never ask the token. This applies even when "only one algorithm is expected" — the attack works precisely because the expectation is not enforced.

- **Long-lived agent tokens with broad scopes.** Issuing an agent a token that lasts for weeks and grants access to multiple resource types because "it's easier to manage" eliminates the containment boundary that makes agent credentials safe. A leaked broad, long-lived token is functionally equivalent to a leaked admin credential.

- **Authentication SaaS as sole login path.** Depending entirely on an external provider for login means the provider's outage is your outage, the provider's vulnerability is your vulnerability, and the provider's token format change is your breaking change. External providers are acceptable as one path in a federated model; they are not acceptable as the only path.

- **Storing tokens in localStorage or sessionStorage.** Browser storage accessible to JavaScript is exfiltrable by any XSS payload. This is not a theoretical concern — it is the first thing an attacker's script does after achieving code execution in the browser. HTTP-only cookies exist to solve this exact problem.

- **Single-person approval for privileged operations.** Any system where one administrator can rotate root keys, export user data, or modify authentication configuration is one compromised account away from total breach. M-of-N is not bureaucracy; it is a cryptographic constraint that bounds the damage of insider threats.

- **Shared credentials between agents.** Multiple agents sharing a single token or API key means revoking one agent's access revokes all of them, and a compromise of one agent is a compromise of all. Each agent receives its own credential with its own scope and its own lifecycle. The operational cost of per-agent credentials is the price of per-agent containment.

- **Falling back to email-based password reset.** If the recovery flow sends a magic link to an email address, the system's security is bounded by email security — which is to say, it is not bounded at all. Email is not an authenticated channel. Recovery must require proof of possession (the recovery passphrase) plus a second factor, not proof of email access.

- **In-memory revocation list in a multi-process deployment.** An in-memory `Set` of revoked token IDs is only coherent within a single process. In any deployment with more than one server instance — load balanced, rolling deploy, horizontal scale — each process has a different view of revoked tokens. A token revoked against one instance is still valid against all others until restart. The revocation store must be a shared, durable, consistent backing store.

---

## Relationship to the Data Blueprint

The scope enforcement in this blueprint (agent tokens carry explicit `scopes` claims, middleware validates scope on every request) is a necessary but not sufficient control for agent data access. Scope enforcement prevents an agent from requesting a resource outside its declared scope — but it operates at the HTTP layer, not the data layer. A bug in the middleware, a misconfigured route, or a missing middleware attachment can bypass it.

The [Data Blueprint](./data-blueprint.md) provides the second layer: agents are architecturally restricted to the analytics tier and have no code path to the transactional store. The two controls address the same threat row ("rogue AI agent exceeding its authorized scope" / "agent process accesses raw transactional data") from different angles. Both must be present. Scope enforcement limits what an agent can request; tier separation limits what an agent can reach even if scope enforcement fails.

---

## Incident Response: Authentication Compromise

The V1 Gate requires a written and tested runbook. At minimum it must cover:

- **Signing key compromise:** immediately rotate the signing key (requires M-of-N); invalidate all outstanding tokens by advancing a key version counter that all verification paths check; force re-authentication for all users and agents; preserve the old key read-only for forensics until all pre-rotation tokens have expired.
- **Agent credential compromise:** revoke the affected agent's token via the revocation store; remove the agent from the registry; audit all requests made by the agent's `sub` claim for the preceding 24 hours; re-register the agent with new credentials and, if the scope was broad, a narrowed scope.
- **Admin account compromise:** immediately require M-of-N to freeze the account; audit all privileged operations performed by the account since last known-good authentication; treat all shard operations that account participated in as potentially compromised and initiate a full shard rotation.
- **Mass session invalidation:** if a systemic credential leak is suspected, the fastest path is to rotate the signing key — this invalidates every outstanding token immediately without needing to enumerate individual `jti` values. All users and agents must re-authenticate after key rotation.
