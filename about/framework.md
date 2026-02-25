# Framework Philosophy

## Core Principles

### 1. Avoid Human-Only Abstractions

Don't use abstractions that exist solely to make development easier for humans. Frameworks, macros, pragmas, and code generators add dependencies and complexity that agents don't need.

- It's acceptable for top-level code to be verbose
- Prefer explicit over implicit
- Reduce dependencies at the cost of developer convenience
- Agents understand the underlying primitives; they don't need abstractions to work at a higher level of abstraction

### 2. Minimize Build Steps

Limit transpiling, compilation, and transformation pipelines to the minimum necessary.

- Web pages should be written in pure HTML and JavaScript
- No bundlers, compilers, or build tools unless genuinely required
- Each build step is a potential point of failure and increases the distance between source and execution

### 3. Prefer Pure, Native Solutions

When a tool or language provides a capability natively, use it directly rather than adding a layer of abstraction.

- Prefer pure SQL over query builders or ORMs
- Use native file formats over custom parsers
- Leverage built-in language features before reaching for libraries

### 4. Extensive Code Comments

Code must be extensively commented in English to help agents orient themselves and minimize context requirements.

- Write comments as if explaining the code to an intelligent but unfamiliar reader
- Include high-level purpose at the top of files and functions
- Document the "why" not just the "what"
- Agents can reference comments directly rather than requiring additional context or documentation lookup
- Verbose comments reduce the need for external context and help agents locate relevant sections quickly

AI agents operate most effectively when working with code that maps directly to execution. Human-centric abstractions exist to reduce boilerplate, improve readability, or enforce patterns—all things agents handle differently. By minimizing layers between intention and execution, we reduce complexity and increase reliability.

The goal is not developer ergonomics, but operational simplicity and maintainability by non-human contributors.
