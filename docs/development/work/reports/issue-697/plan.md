# Integer literal neighbor implementation plan

Goal/spec: implement design.md using existing AST collector and decimal parser.
Parent: codex/analyzer-696 (3438b3a); #696 cargo clean removed 3.9 GiB before setup.

- [x] Public CLI tests: exact signed replacements, 0 power/dot syntax, exclusions,
  u64 boundaries and weak/strong saved-plan verification. Observe unknown-ID RED.
- [x] Register enum/name/Arithmetic rank/README count/operator inventory, and add
  context-aware collection without changing structural neighbor behavior.
- [x] CPython compile candidates; targeted analyzer/core/integration regressions,
  whole workspace suite, fmt/clippy; prove Lean model with a 20s deadline.
- [ ] Five implementation and five test review passes, independent review; commit
  all artifacts, create PR based on #696, link stack, cargo clean and delete scratch.

## Plan self-review

1. Assertions derive from mathematical examples, not the production decimal helper.
2. RED uses string selector so missing operator fails at runtime rather than compile.
3. Negative child and subscript nesting need explicit zero-candidate assertions.
4. Bound tests require both u64::MAX neighbors and unsupported u64::MAX+1 source.
5. Preserve inherited #696 tests; own target, no shared build cache that defeats clean.
