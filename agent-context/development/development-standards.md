# New Module Development Workflow

## Overview

The **New Module Development** workflow is a structured process for building new features or modules from scratch. It follows a **Plan → Stub → Implement** cycle designed to:

- Maximize context for agents
- Minimize hallucinations
- Ensure quality through incremental verification

## When to Use This Workflow

Use this workflow when:

- Starting a new module or package
- Building a significant new feature
- Implementing a complex architectural change
- The task requires multiple sessions/agents to complete

**Don't use for**: Small bug fixes, minor refactors, or simple one-file changes.

## The Three Phases

### Phase 1: Planning (High-Capability Agent)

**Agent Profile**: Claude Opus, GPT-4o, Gemini Pro (high context window, reasoning-heavy)

**Goal**: Create a comprehensive plan before writing any code.

#### Create Plan Document

Location: `docs/plan/<module_name>_plan.md`

Contents:

- **Product Features**: What are we building?
- **Technical Implementation**: Libraries, patterns, architecture
- **Prioritized Task List**:
    - Incremental (buildable pieces)
    - **Risk-First**: Tackle biggest unknowns early

#### Feasibility Review

The same or similar agent reviews the plan for:

- Technical correctness
- Viability (can this actually be built?)
- Context sufficiency (can a lower-tier agent execute?)

**Output**: Edit the plan directly. Don't create a separate review doc.

**Example plan structure:**

```markdown
# Payment Module Plan

## Product Features

- Accept credit card payments via Stripe
- Support refunds
- Store transaction history

## Technical Implementation

- Library: stripe-node v14.x
- Pattern: Repository pattern for data access
- Architecture: Layered (Controller → Service → Repository)

## Task List (Risk-First)

- [x] Stripe API key integration (HIGH RISK - external dependency)
- [ ] Payment processing endpoint
- [ ] Refund processing endpoint
- [ ] Transaction history storage
- [ ] Unit tests
- [ ] Integration tests
```

---

### Phase 2: Stubbing (High-Capability Agent)

**Agent Profile**: Claude Opus, GPT-4o, Gemini Pro

**Goal**: Structure the codebase before implementing logic.

#### Generate Stubs

1. **Create file structures**: All classes, functions, signatures
2. **Add documentation**:
    - Module `README.md`
    - Extensive inline comments (JSDoc, docstrings)
3. **Ensure compilability**: Use `throw new Error("Not implemented")` or similar
4. **No logic yet**: Just structure

**Example stub:**

```typescript
/**
 * Payment Service
 *
 * Handles payment processing via Stripe API.
 * See docs/plan/payment_plan.md for architecture.
 */
export class PaymentService {
    /**
     * Process a payment
     *
     * @param amount - Amount in cents
     * @param currency - ISO currency code (e.g., "usd")
     * @param paymentMethodId - Stripe payment method ID
     * @returns Payment confirmation object
     * @throws PaymentError if payment fails
     */
    async processPayment(
        amount: number,
        currency: string,
        paymentMethodId: string
    ): Promise<PaymentConfirmation> {
        throw new Error("Not implemented");
    }
}
```

#### Stub Tests

Create test files with test case signatures:

```typescript
describe("PaymentService", () => {
    describe("processPayment", () => {
        it("should process valid payment", async () => {
            // TODO: implement
        });

        it("should throw PaymentError on invalid card", async () => {
            // TODO: implement
        });

        it("should handle network failures gracefully", async () => {
            // TODO: implement
        });
    });
});
```

#### Commit Stubs

```bash
git commit -m "feat: stub payment module structure

- Add PaymentService with method signatures
- Add PaymentRepository interface
- Add test file structure
- See docs/plan/payment_plan.md for details"
```

#### Update Plan

Mark stubbing phase complete:

```markdown
## Task List

- [x] **STUBBING COMPLETE**
- [x] Stripe API key integration
- [ ] Payment processing endpoint (IN PROGRESS)
      ...
```

---

### Phase 3: TDD Implementation (Standard Agent)

**Agent Profile**: Claude Sonnet, GPT-4o-mini, Gemini Flash (cost-effective, fast)

**Goal**: Fill in the blanks using Test-Driven Development.

#### Write Tests First

Before implementing a function:

1. Write specific test case
2. Run test (should fail - RED)
3. Implement minimum code to pass (GREEN)
4. Refactor if needed

**Example TDD cycle:**

```typescript
// 1. RED - Write failing test
it('should process valid payment', async () => {
  const service = new PaymentService();
  const result = await service.processPayment(1000, 'usd', 'pm_test_123');

  expect(result.status).toBe('succeeded');
  expect(result.amount).toBe(1000);
});

// Run: ❌ FAIL - "Not implemented"

// 2. GREEN - Minimum code to pass
async processPayment(amount, currency, paymentMethodId) {
  const paymentIntent = await stripe.paymentIntents.create({
    amount,
    currency,
    payment_method: paymentMethodId,
    confirm: true,
  });

  return {
    status: paymentIntent.status,
    amount: paymentIntent.amount,
  };
}

// Run: ✅ PASS

// 3. REFACTOR - Improve code quality
async processPayment(amount, currency, paymentMethodId) {
  this.validateAmount(amount);
  this.validateCurrency(currency);

  try {
    const paymentIntent = await this.createPaymentIntent(
      amount,
      currency,
      paymentMethodId
    );
    return this.mapToConfirmation(paymentIntent);
  } catch (error) {
    throw new PaymentError('Payment processing failed', error);
  }
}
```

#### Continuous Plan Updates

After completing a logical unit, update the plan:

```markdown
## Task List

- [x] Stripe API key integration
- [x] Payment processing endpoint ← JUST COMPLETED
- [ ] Refund processing endpoint (NEXT)
      ...
```

---

## Iteration Loop

Repeat:

1. **Check Plan**: What's next priority?
2. **Pick Task**: High-priority/high-risk
3. **Implement** (TDD):
    - Write test
    - Implement
    - Refactor
4. **Update Plan**: Mark complete
5. **Commit**: Git-brain commit with reasoning

---

## Agent Commands

### OpenCode

```bash
/new-module payment-processing
```

Agent will:

1. Read `.nightshift/commands/new-module-development.md`
2. Create `docs/plan/payment-processing_plan.md`
3. Guide through Plan → Stub → Implement

### Other Vendors

```
I need to build a new payment processing module.
Follow the new-module-development workflow from
.nightshift/commands/new-module-development.md
```

---

## Living Plan Protocol

**The plan is NOT static** - it's the source of truth.

### Update Triggers

- Task completed
- Blocker found
- New information learned
- Implementation reveals plan was wrong

### Update Actions

- Mark items: `[x] Complete`, `[ ] Blocked (reason)`
- Remove/descope items no longer needed
- **Refactor plan**: If implementation reveals the plan was wrong, rewrite relevant sections

**Example refactor:**

```markdown
## Original Plan

- Use REST API for payment webhooks

## After Implementation (Refactored)

- ~~Use REST API for payment webhooks~~ (CHANGED)
- Use Stripe webhooks with signature verification
    - Reason: Stripe provides built-in retry logic
    - See stripe-webhooks branch for implementation
```

---

## Checklist

Before considering a module "done":

- [ ] **Plan**: Created with risk-prioritized tasks
- [ ] **Review**: Plan validated for feasibility
- [ ] **Stub**: Structure with verbose comments & compiling code
- [ ] **Implement**: TDD approach (Tests → Code → Refactor)
- [ ] **Maintain**: Plan updated after every significant step
- [ ] **Tests**: All tests passing
- [ ] **Nags**: All quality checks (build, test, lint) passing
- [ ] **Documentation**: Module README.md complete

---

## Philosophy

> "Plan with Opus, Stub with Opus, Implement with Sonnet."

Use expensive, high-capability agents for planning and structuring. Use cheaper, fast agents for the mechanical work of filling in TDD tests.

This workflow maximizes quality while minimizing cost.

---

## Benefits

✅ **Reduces hallucinations**: Stubs provide concrete structure
✅ **Enables handoff**: Another agent can pick up from plan
✅ **Incremental verification**: TDD catches issues early
✅ **Risk management**: High-risk items tackled first
✅ **Context preservation**: Living plan maintains state
✅ **Cost optimization**: Right agent for right task

---

## Common Pitfalls

❌ **Skipping planning**: Results in rework and confusion
❌ **Implementing before stubbing**: Hard to see overall structure
❌ **Ignoring the plan**: Plan becomes stale and useless
❌ **Not updating plan**: Next agent has no context
❌ **Testing after implementation**: Missed edge cases

✅ **Follow the process**: Plan → Stub → Implement → Update