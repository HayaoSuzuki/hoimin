# Function body erasure implementation plan

Spec: design.md. Parent e4a359e; previous issue cargo clean removed 3.9 GiB.

- [x] RED: public CLI fixture for preserved docstring/header, eligible/no-op scope
  boundaries, weak/strong saved-plan outcome and Python compilation.
- [x] Implement focused function-body module and collector hook; register enum,
  Behavioral ranking, inventories and opt-in documentation.
- [x] Add nested-default yield, line anchor, size-limit and cancellation regressions;
  Lean model, full workspace tests, fmt/clippy and real-package probes.
- [ ] Five implementation/test self-reviews, independent review, commit artifacts,
  stacked PR and cargo clean before next issue.

## Plan self-review

1. Use source strings and literal expected original/replacement, not AST helpers in tests.
2. Nested generator's body must not exclude outer function; its own candidate absent.
3. Include yield in nested default (outer generator) to expose a naive scope-skip bug.
4. Weak/strong fixture asserts baseline and killed/survived, not invocation alone.
5. Resource check uses one >2 MiB comment span rather than millions of AST nodes.
