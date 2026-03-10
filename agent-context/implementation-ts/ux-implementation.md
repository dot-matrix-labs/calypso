# UX — Calypso TypeScript Implementation

<!-- last-edited: 2026-03-10 -->

CONTEXT MAP
  this ──implements──▶ blueprints/ux-blueprint.md
  this ◀──referenced by── index.md

> Implements: UX Blueprint (`agent-context/blueprints/ux-blueprint.md`)

The principles, threat model, and patterns in that document apply equally to other stacks. This document covers the concrete realization using TypeScript, Bun, React, and the Calypso monorepo layout.

---

## Package Structure

```
/packages/ui
  /design-system       # Shared component library: tokens, primitives, patterns
  /end-user            # End-user interface components and flows
  /admin               # Admin interface components and flows

/packages/services
  /capability-api      # Typed service capability definitions (used by all surfaces)

/apps/web              # End-user React application (consumes design-system, end-user)
/apps/admin            # Admin React application (consumes design-system, admin)
/apps/agent-sdk        # Agent TypeScript SDK (consumes capability-api, no React)
/apps/server
  /api                 # REST API routes backing all surfaces
  /agent-router        # Agent-specific API routes with structured schema responses
```

## Core Interfaces

```typescript
// Service capability — the atomic unit of UX design
interface Capability {
  id: string;                     // e.g. 'invoice.create'
  allowedActors: ActorType[];     // ['end-user', 'admin', 'agent']
  requiredScopes: string[];       // e.g. ['invoices:write']
}

// Actor types — determines which interface surface is appropriate
type ActorType = 'end-user' | 'admin' | 'agent';

// Agent presence declaration on a user account
interface AgentPresence {
  agentId: string;
  accountId: string;
  declaredScopes: string[];       // what the agent is authorized to do
  visibleToAccountHolder: true;   // invariant: always true
  grantedAt: number;
  lastActiveAt: number;
}

// Agent action log entry — every agent operation on the account is recorded
interface AgentActionRecord {
  agentId: string;
  accountId: string;
  capabilityId: string;
  inputSummary: string;           // human-readable, not raw payload
  outcome: 'success' | 'rejected' | 'failed';
  timestamp: number;
}

// Service flow state — the authoritative UX specification unit
interface ServiceFlowState {
  id: string;
  label: string;
  availableTransitions: ServiceFlowTransition[];
}

interface ServiceFlowTransition {
  id: string;
  trigger: 'user-action' | 'system-event' | 'agent-action';
  targetStateId: string;
  requiredCapability: string;     // maps to Capability.id
}
```

## Dependency Justification

| Package | Decision | Reason |
|---|---|---|
| React | Buy | UI component model; no viable DIY for the browser rendering layer |
| Tailwind CSS | Buy | Design token system and utility classes; DIY CSS at scale is unmaintainable without a preprocessor or framework |
| Design token generator | DIY | Token definitions are a JSON file; no package needed |
| Agent SDK HTTP client | DIY | `fetch` is native to Bun; a thin typed wrapper over `fetch` is < 50 lines |
| Component documentation tool | DIY | Design system documentation is a static markdown or auto-generated HTML artifact produced by the build pipeline; no runtime dev server tool (e.g. Storybook) is warranted |
| Form state management | DIY | React `useState` + controlled inputs handle all form cases; no external library needed at Calypso scale |

---

## Antipatterns (TypeScript/Calypso-Specific)

- **Technology-specific UX specifications.** A design document that specifies "use a React modal" or "apply a Tailwind flex container" has conflated design and implementation. When the implementation changes, the specification is wrong. Specifications describe user states and transitions; implementations are chosen to satisfy them.

- **GUI-dependent development tooling.** Calypso runs on hosted Linux with no display server. Any tool that requires a live browser window, a native GUI, or a local dev server to be useful to the developer (e.g. Storybook, Figma desktop agents, visual diff tools requiring a display) cannot be used in the agent workflow. Visual output is evaluated via headless Playwright screenshots. Design system documentation is a static build artifact. If a tool cannot run headlessly and produce its output to stdout or a file, it does not belong in the Calypso toolchain.
