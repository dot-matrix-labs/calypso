# Calypso FAQ

## Development Environment

### "Why not on my Mac?"

Because you don't deploy on a Mac server. Trying to build code that works for development and testing on a Mac, only to later deploy it on Ubuntu, is an anti-pattern. 

The Calypso Blueprint mandates that all development occurs natively on a bare-metal Linux host in the cloud (using `tmux` and an AI agent like Claude/Gemini/Codex). This guarantees that the execution context exactly mirrors the deployment and testing environments.

### "Why not just use Docker for consistency?"

Docker adds an unnecessary layer of networking, volume management, and build-time complexity for a single-stack application. By developing directly on the Linux target host, we eliminate the "works on my container but not in production" class of bugs. A single `systemd` service and `.env` file run natively is fundamentally easier for AI agents to write, debug, and maintain than troubleshooting broken Dockerfile layers.

## Architecture & Testing

### "Why Bun instead of Node or Deno?"

Bun is chosen for its significantly faster start times, built-in TypeScript execution (no `ts-node` or compilation steps required for server code), and built-in testing (`bun test`). It drastically reduces the number of toolchain dependencies needed to get a project running.

### "Why never mock APIs? Isn't that standard practice?"

Mocking is a leading cause of false confidence. You end up testing that your code works against your *imagination* of how an external API behaves, not how it *actually* behaves. The Calypso Blueprint requires generating "golden fixtures" via actual network requests. While harder to set up initially, it ensures your code survives real-world API drift and eliminates a massive source of production bugs.

### "Why no heavy state-management libraries (Redux, MobX, etc.)?"

Heavy state libraries encourage putting everything into global state, leading to tight coupling and complex rendering cycles. For 90% of web web applications, React hooks (`useState`, `useContext`) combined with simple prop-drilling or a data-fetching library (like React Query or SWR) are more than sufficient and much easier for AI agents to reason about without hallucinating massive boilerplate.

### "Why is dependency 'cloning' (DIY) encouraged over just doing `npm install`?"

Every `npm` dependency is a liability—it's code you don't control, bringing its own transitive dependencies, potential security flaws, and breaking changes. For trivial utilities (like date formatting or tiny UI components), having an AI agent generate a clean, tree-shaken, tested implementation directly in your codebase takes seconds and removes a permanent supply-chain risk. We explicitly reserve "Buy" (`npm install`) for complex, high-liability features like payment processing (Stripe) or dense specifications (PDF generation).
