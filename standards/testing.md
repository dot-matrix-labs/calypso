# Testing & CI/CD

## Rules
- **Never fabricate** test data. No invented API responses/DB states/DOM structures.
- **Golden fixtures only** — recorded from real service requests. Agent must build the fixture generator.
- **No mocking libraries** (jest.mock, msw). No `.skip()`. No headed browser tests.
- Test on Linux only. Headless Chromium via Playwright + Vitest.
- CI mirrors production environment exactly.

## Test Categories
1. **Unit** — pure logic, no API server needed
2. **API Integration** — validate against recorded fixtures, type-check schemas
3. **React Component** — headless browser, isolated rendering/interaction
4. **Full-Page E2E** — click-through flows, navigation, forms

## CI/CD
- GitHub Actions. One workflow per test suite.
- Lint + format enforced before tests. Failures block merge.
- Linux runners. Command-line test output only (no browser reporters).
- Separate `.github/workflows/*.yml` per suite. Security audit (`bun pm audit`) on every PR.
- Deploy per milestone (Alpha/Beta/V1). Logging + monitoring enforced at Beta.
