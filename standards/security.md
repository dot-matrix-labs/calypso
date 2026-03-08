# Security

## 1. Secrets
- **Dev:** `.env` (never committed). `.env.example` committed with placeholders. `.gitignore`: `.env`, `.env.local`, `.env.production`.
- **Prod:** `systemd` `EnvironmentFile=` (root-owned, 0600, outside app dir). At V1: evaluate Vault/Infisical/KMS for regulated data.
- **Rotation:** JWT keys support array (current+previous). All secrets rotatable via env update + `SIGHUP`/reload.

## 2. Auth
- **Default:** Self-hosted JWT via `crypto.subtle`, HTTP-only cookies.
- **Enterprise (Beta+):** OIDC (Auth Code + PKCE) — justified Buy. SAML via OIDC bridge. Internal JWT remains session token.
- Roles/permissions in `auth/permissions.ts` as TypeScript constants. Middleware enforces per-route.
- Multi-tenant: tenant scope on every query. Enforce at query layer.

## 3. HTTP Headers
Apply as middleware on every response:
```typescript
{
  "Strict-Transport-Security": "max-age=63072000; includeSubDomains",
  "X-Content-Type-Options": "nosniff",
  "X-Frame-Options": "DENY",
  "Referrer-Policy": "strict-origin-when-cross-origin",
  "Permissions-Policy": "camera=(), microphone=(), geolocation=()",
  "Content-Security-Policy": "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'"
}
```
CORS: explicit origin allowlist. Never `*` in production.

## 4. Rate Limiting
- Auth endpoints: 10 req/min/IP. API: 100 req/min/user.
- DIY sliding window counter (~30 lines). Shared store at V1 with multiple instances.

## 5. Input Validation
- Validate at API boundary only. TypeScript type guards for runtime validation.
- SQL: parameterized always. Never interpolate user input.
- File uploads: validate MIME, enforce size, store outside web root.

## 6. CI Security
```yaml
name: Security Audit
on: [pull_request]
jobs:
  audit:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: oven-sh/setup-bun@v2
      - run: bun install --frozen-lockfile
      - run: bun pm audit
```
- Pin bootstrap script to commit hash. Local `docs/standards/` is source of truth after bootstrap.
- Never echo secrets in CI. Use `::add-mask::`. Prefer OIDC over long-lived API keys.

## 7. Audit Logging
- Log: auth events, authorization failures, all data mutations (with user ID + timestamp).
- Append-only `audit_log` table in telemetry DB. Retention: 90d min, 1y for regulated.

## 8. Agent Constraints
- Run as `calypso` user, no root. Project directory + systemd user services only.
- No secrets in commits, code, comments, or Git-Brain metadata.
- Pre-commit scans for `sk_live_`, `AKIA`, high-entropy strings. Track costs via `session` field.
