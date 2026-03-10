# UX Blueprint

> [!IMPORTANT]
> This blueprint defines the UX posture for all Calypso applications. UX is not a visual design concern alone — it is the contract between a service and every type of user that interacts with it, including administrators and AI agents.

---

## Vision

Beauty is a gate condition, not a preference. A Calypso application that is not visually polished and immediately comprehensible will not survive management review — regardless of its technical correctness. The interface is the product to every non-technical stakeholder. UX quality must be established from the first screen, the first prototype, and the first demo. An ugly early version sets an anchor that is nearly impossible to reverse. Applications that are not beautiful from the start do not ship.

Service delivery is designed before interfaces. The question "how do we deliver this capability to a user?" precedes "what does the screen look like?" The channel — visual browser interface, API, command-line, voice, structured text — is a consequence of the service design, not its premise. A service designed around a visual browser interface produces a service that only works in a visual browser. A service designed around the delivery of a capability to a user produces something that can be expressed in any appropriate medium.

Every user type receives an interface appropriate to their medium. A human end-user navigates a visual interface. An administrator manages operations through a purpose-built interface — never through raw database tooling or developer consoles. An AI agent interacts through structured, machine-readable interfaces — not through screen scraping, browser automation, or interfaces designed for human perception. Designing only for the human end-user produces a system where administration is painful and agent integration is fragile.

The AI agent is a first-class user with a defined presence on the end-user account. It is not a background process operating invisibly. Its participation in the account is declared, visible to the human account holder, and governed by a clearly specified scope. The end-user knows the agent is there. The service is designed to make that shared-account relationship explicit and auditable, not to obscure it.

Applications that neglect UX as a design discipline — that defer it, treat it as a visual afterthought, assume it applies only to end-users, or fail to specify agent interaction as a first-class UX surface — produce systems that are operationally brittle, inaccessible to automation, and abandoned by users.

---

## Problem Space

| Failure Mode | What is degraded or lost |
|---|---|
| Interface ships before service is designed | UX is forced to compensate for service design gaps; users encounter dead ends, confusing flows, and tasks that cannot be completed without workarounds |
| Multiple paths to the same action exist | Users cannot build reliable mental models; onboarding takes longer; support costs increase; automation fails when path selection is non-deterministic |
| Admin UI is an afterthought | Operational tasks require developer intervention or direct database access; configuration errors are not caught before execution; operations are irreversible without a safety net |
| AI agent has no specified UX | Agents resort to browser automation or screen scraping, which breaks on any visual change and is undetectable when it fails silently |
| AI agent's presence on the account is invisible | End-users do not know what the agent can see or do; trust is violated when the agent acts on data the user did not understand was shared |
| Beautiful prototype replaced by utilitarian production code | Management anchor on the prototype quality is broken; stakeholders lose confidence; remediation requires a design retrofit that is more expensive than building correctly from the start |
| Design is coupled to a front-end framework | Redesigning or porting the UX requires rewriting both design and implementation; design decisions are constrained by framework capabilities rather than user needs |
| Complexity is exposed by default | Users encounter features they do not need before they understand the ones they do; first-use abandonment increases; the product appears more complex than its core value warrants |
| Agent scope on the account is undefined | Agent writes, reads, or triggers actions beyond the user's expectation; the system has no mechanism to audit or limit what the agent did on the user's behalf |

---

## Core Principles

### Service delivery precedes surface design.

The UX of a capability is defined as the sequence of states a user moves through to accomplish a goal — not as the visual or interactive elements that render those states. Interfaces are outputs of service design, not inputs. A team that designs the screen before designing the service will produce a screen that cannot change without breaking the service.

### There is one obvious way to accomplish any task.

For every user goal, there is exactly one path through the system that is clearly correct and clearly available. Alternative paths either do not exist or are hidden behind an explicit escalation (such as "advanced options"). Ambiguity about which path to take is a design defect. This applies equally to human interfaces and machine interfaces: an agent calling an API endpoint must have one correct way to perform a given operation.

### Every user type has a medium-appropriate interface.

Human end-users interact through visual, touch, or voice surfaces. Administrators interact through operational interfaces that surface system state and protect against dangerous operations. AI agents interact through structured, machine-readable interfaces — APIs with typed schemas, command-line tools with deterministic output, or structured text protocols. Exposing an agent to a human interface, or exposing an administrator to a raw API, is a design failure.

### Beauty is a functional requirement, not a preference.

Visual quality determines whether a product survives stakeholder review, earns user trust, and sets the baseline for future development. A polished interface signals that the product is under control. An unpolished interface signals that the product is not ready — regardless of what the backend does. Beauty is evaluated at every milestone, not deferred to V1.

### The agent's presence is explicit and bounded.

An AI agent operating on a user's account is a shared participant, not an invisible background process. The system declares the agent's presence, surfaces its scope, and logs its actions in a form the account holder can review. The agent's UX — the structured interfaces it uses — is scoped to match what the account holder has authorized. Unbounded, invisible agent access is a UX failure.

### Designers specify needs, not implementations.

A UX specification describes what a user needs to accomplish, the states they move through, and the feedback they receive — not the components, frameworks, or libraries used to render those states. A specification that references specific front-end technologies has conflated design with implementation. Any compliant implementation of the specification is valid.

---

## Design Patterns

### Pattern 1: Service Flow Mapping

**Problem:** Teams design interfaces before understanding the sequence of states a user must traverse to accomplish a goal, producing interfaces that cannot be consistently navigated.

**Solution:** Before any visual design begins, map the service flow as a state machine. Define: the entry state, the terminal state (goal achieved), every intermediate state, and every transition. Annotate each transition with what triggers it (user action, system event, agent action) and what feedback the system provides. The resulting map is the authoritative UX specification from which interfaces in any medium are derived.

**Trade-offs:** Service flow mapping adds time before design begins. It is the wrong pattern when the service is exploratory or rapidly evolving — in those cases, prototype first and formalize the flow once the happy path is stable.

---

### Pattern 2: Single-Path Navigation

**Problem:** Multiple routes to the same destination cause users to question whether they are in the right place, whether the routes produce different outcomes, and which one to use next time.

**Solution:** For every user goal, design exactly one primary path. If a secondary path exists for power users or edge cases, it is explicitly labeled as an alternative and is not surfaced in the default flow. Navigation is structured so that at every point, the next correct action is either the only visible action or the most visually prominent one. This applies to visual interfaces (primary CTA), CLI interfaces (canonical command with no aliases required for default use), and API interfaces (one endpoint per operation with no ambiguous overlaps).

**Trade-offs:** Single-path navigation requires deliberate choices about what is primary. This creates friction for teams that want to surface multiple options simultaneously. The pattern is wrong when the user population is expert and expects direct access to multiple routes — in those cases, expose alternatives explicitly rather than hiding them.

---

### Pattern 3: Agent-Native Interface

**Problem:** AI agents are forced to interact with interfaces designed for human perception, producing integrations that are fragile, opaque, and break silently when visual layouts change.

**Solution:** Every capability that an agent must access is exposed through a structured, machine-readable interface with a stable schema. This interface is a first-class product surface — versioned, documented, and tested to the same standard as the human-facing interface. The agent's interface is not a workaround for the absence of a visual interface; it is the canonical interface for that user type. The human-facing interface is built on top of the same underlying capability, not in place of it.

**Trade-offs:** Maintaining two interface surfaces (human and agent) increases design and implementation scope. This cost is lower than the cost of maintaining fragile browser automation against a changing visual interface. The pattern is wrong when agent access is genuinely never required — do not pre-build an agent interface for capabilities that have no agent use case.

---

### Pattern 4: Unified Design System Across User Types

**Problem:** Admin interfaces and end-user interfaces are designed independently, producing systems where administrators are unable to form accurate mental models of what users experience, and where users see inconsistency if they access admin-adjacent views.

**Solution:** A single design system governs all visual interfaces in the application, including administrative interfaces. Interaction patterns, information hierarchy, form behavior, feedback, and error handling are specified once and applied everywhere. The admin interface is not exempt from visual and UX standards. Role-specific elements (destructive actions, bulk operations, system configuration) are expressed within the design system rather than outside it.

**Trade-offs:** Applying the design system to admin interfaces requires more design investment upfront. The pattern is wrong when admin and end-user workflows are so different in nature that a shared system produces confusing ambiguity — in those cases, maintain shared foundations (color, typography, component library) while allowing layout and navigation to diverge.

---

### Pattern 5: Progressive Disclosure

**Problem:** Full system complexity shown at first contact causes abandonment and slows onboarding; but hiding capabilities from experienced users creates frustration.

**Solution:** The default state of any view shows only what is needed for the primary task at that moment. Secondary capabilities, advanced options, and edge-case controls are accessible but not immediately visible. Disclosure is triggered by explicit user intent (a labeled "advanced" toggle, a secondary navigation level) not by scrolling past primary content. Each disclosure level is stable — once revealed, the advanced state persists for that user session or preference.

**Trade-offs:** Deciding what belongs at each disclosure level is a judgment call that requires user research or iteration. The pattern can be used to hide capabilities that should be primary; audit disclosure levels regularly against actual usage patterns.

---

## Plausible Architectures

### Architecture A: Unified Service Layer, Multiple Rendering Surfaces

Appropriate for: products with heterogeneous user types (human end-users, human admins, AI agents) requiring access to the same service capabilities. Small to mid-scale teams.

```
                    ┌───────────────────────────────────┐
                    │           Service Layer            │
                    │  (capabilities, state, business   │
                    │         logic, access control)     │
                    └──────┬───────────┬──────────┬─────┘
                           │           │          │
               ┌───────────▼─┐  ┌──────▼───┐  ┌──▼──────────────┐
               │  End-User   │  │  Admin   │  │  Agent Interface │
               │  Web/Mobile │  │  Web UI  │  │  (Structured API │
               │  Interface  │  │          │  │   / CLI / SDK)   │
               └─────────────┘  └──────────┘  └─────────────────┘
```

**Trade-offs vs. other architectures:** The service layer is a single point of design governance — all surfaces inherit the same capability definitions. Adds abstraction between service and surface that teams unfamiliar with service-first design find counterintuitive.

---

### Architecture B: API-First, Client-Agnostic

Appropriate for: products where the set of interface surfaces is unknown at design time, or where third-party clients are expected. Also appropriate when the agent interface is the primary interface and the human interface is secondary.

```
┌───────────────────────────────────────────────────────────┐
│                    Capability API                          │
│         (versioned, typed schema, access-scoped)          │
└──┬──────────────┬──────────────┬──────────────────────────┘
   │              │              │
┌──▼──────────┐  ┌▼──────────┐  ┌▼────────────────────────┐
│ First-Party │  │ First-Party│  │  Agent SDK / CLI Tool   │
│  Web Client │  │ Admin CLI  │  │  (machine-readable      │
│             │  │            │  │   output, typed schema) │
└─────────────┘  └───────────┘  └─────────────────────────┘
                                          │
                               ┌──────────▼──────────┐
                               │  Third-Party Clients │
                               │  (partner apps,      │
                               │   integrations)      │
                               └──────────────────────┘
```

**Trade-offs vs. other architectures:** Maximum flexibility for surface diversity. Requires disciplined API design — the API is the product, and breaking changes affect all surfaces simultaneously. Client teams must build against a contract rather than a shared codebase.

---

### Architecture C: Agent-Mediated Administration

Appropriate for: products where administrative operations are predominantly performed by AI agents on behalf of human principals, with humans approving rather than executing.

```
Human Admin
    │  (reviews, approves, or vetoes)
    ▼
┌───────────────────────────────┐
│   Approval Interface          │
│   (human-readable action log, │
│    approval/rejection UI)     │
└──────────────┬────────────────┘
               │ approved actions
               ▼
┌───────────────────────────────┐
│   Agent Execution Layer       │
│   (agent operates via         │
│    structured capability API) │
└──────────────┬────────────────┘
               │
               ▼
┌───────────────────────────────┐
│   Service Layer               │
│   (same capabilities as       │
│    human-facing interface)    │
└───────────────────────────────┘
```

**Trade-offs vs. other architectures:** Reduces human operational burden at the cost of requiring a robust approval and audit surface. Wrong when agents do not yet have sufficient reliability to be trusted with initiating actions — in those cases, agents should propose, and humans should execute.

---

---

> For the Calypso TypeScript implementation of these patterns, see [ux-implementation.md](../implementation-ts/ux-implementation.md).

## Implementation Checklist

### Alpha Gate

- [ ] Service flow maps written and reviewed for all primary user goals before any interface implementation begins
- [ ] Design system initialized: color tokens, typography scale, spacing scale, and at least one primitive component (button) defined
- [ ] End-user interface implements all primary-path flows without dead ends
- [ ] Admin interface exists as a distinct surface and uses the shared design system; no raw database UI in use
- [ ] Agent API routes exist for every capability declared as `allowedActors: ['agent']`; routes return typed JSON, not HTML
- [ ] `AgentPresence` record created and readable by the account holder for every agent granted account access
- [ ] `AgentActionRecord` written for every agent operation against the account; records are accessible from the account holder's interface
- [ ] Single-path navigation verified: no user flow has two equally prominent routes to the same destination
- [ ] Visual quality verified via headless Playwright screenshot capture; screenshots reviewed by a vision-capable model or human stakeholder — no live browser session required or permitted
- [ ] Designers have signed off that no specification references a specific front-end framework, component, or CSS property

### Beta Gate

- [ ] Progressive disclosure implemented: advanced options are present but not surfaced in the default view; verified with a first-use walkthrough
- [ ] Agent scope displayed in the account settings UI: account holder can see what the agent is authorized to do and when it last acted
- [ ] Agent action log is paginated, searchable, and accessible without developer tooling
- [ ] Admin interface covers all operational tasks without requiring database access, terminal access, or API calls outside the admin UI
- [ ] Agent SDK published with typed method signatures matching every `agent`-authorized capability; no method requires HTML parsing or visual interaction
- [ ] Usability review completed with at least one human end-user, one administrator, and one agent integration test
- [ ] All interface surfaces tested in headless Chromium via Playwright; no surface requires a GUI, display server, or live browser window

### V1 Gate

- [ ] Full service flow documentation published and version-controlled alongside the implementation
- [ ] Design system documented in a static HTML or markdown catalogue generated by the build pipeline; no dev server or browser-only tooling required to read it
- [ ] Agent capability surface versioned; breaking changes require a new version, not an in-place modification
- [ ] Agent presence and action log exported on account data export request
- [ ] UX review conducted at each milestone gate with explicit sign-off from a non-technical stakeholder

---

## Antipatterns

- **Screen-first design.** Designing the interface before mapping the service flow produces interfaces that expose gaps in the service design as confusing dead ends or workaround paths. A screen that cannot be explained by a service flow state does not belong in the product.

- **The developer console as admin UI.** Shipping without an admin interface and substituting direct database access, terminal commands, or API clients for administrative tasks means the admin UX is unspecified, untested, inconsistent, and inaccessible to non-developers. Administration is a product surface with real users.

- **Browser automation as agent integration.** Pointing an agent at a human-facing browser interface — whether through a headless browser, DOM scraping, or visual element parsing — produces an integration that breaks silently on any layout change and cannot be tested deterministically. The agent interface is a product surface; it must be designed and maintained as one.

- **Invisible agent participation.** An AI agent operating on a user account that the user cannot see, scope-limit, or audit is a trust violation regardless of what the agent does. The end-user's mental model of their account does not include the agent; that gap is a UX defect that becomes a support and trust problem.

- **Beauty deferred to V1.** Treating visual quality as a polish pass at the end of development anchors stakeholders and users on an unpolished baseline that is expensive to correct. The first demo sets the quality anchor. Ship the first demo at the intended quality level or do not demo.

- **One interface for all actor types.** Building a single interface that attempts to serve end-users, administrators, and agents simultaneously produces an interface that serves none of them well. Each actor type has distinct goals, available actions, and appropriate interaction patterns. Shared foundations (design system, capability API) do not imply a shared surface.

- **Multiple equally prominent paths to the same action.** Offering two buttons that appear to do the same thing, two navigation entries for the same destination, or two API endpoints for the same operation forces the user to reason about whether the paths are truly equivalent. They will often be wrong. One path is a design decision; two paths are an unresolved design conflict.

- **Complexity surfaced by default.** Showing all capabilities, settings, and options on first use causes cognitive overload and slows users to the speed of their least-needed feature. Default views must be designed for the primary task; everything else requires deliberate disclosure.

