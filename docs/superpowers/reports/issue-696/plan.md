# Issue 696 implementation plan

Goal: expose opt-in statement_delete through the existing analyzer and CLI.
Spec: design.md. Stack parent: origin/main at 4cc710d.

- [x] Add public CLI plan tests for eligible and excluded syntax; run RED.
- [x] Add enum/name registry and a cancellable AST eligibility visitor in a focused
  analyzer module; call from visit_stmt before normal recursion; run GREEN.
- [x] Exercise CPython 3.14 compilation, encoding/newline boundaries, selector/limit
  behavior, default exclusion, stable plans and saved-plan weak/strong verification.
- [x] Prove model eligibility/single replacement properties with Lean 4.32.2; check
  deliberate missing-exclusion witness under a 20-second deadline.
- [x] Review implementation five times and tests five times; run format, clippy and
  applicable regression suites. Commit code, design, plan, proof and evidence.
- [ ] Create issue PR, clean issue target and temporary artifacts before Issue 697.

## Plan self-review (five passes, before implementation)

1. Coverage: paired weak/strong save fixture exercises the observable missing effect.
2. Order: tests use selector strings so RED compiles before the enum exists.
3. Interfaces: public run_with_io uses JSON; no analyzer implementation duplicated.
4. Boundary coverage: include sole suite, semicolon, multiline, nested yield, walrus,
   BOM/Latin-1/newlines and exclusions; compile actual source bytes with CPython.
5. Resource/commit audit: debug info and incremental disabled; worktree has its own
   target; cargo clean before next issue; no global cache removal.
