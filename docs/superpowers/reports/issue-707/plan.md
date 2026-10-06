# Conversion removal implementation plan

Parent 38a15dd; previous cargo clean removed 4.4 GiB.

1. RED tests: all names/source forms, shadowing/exclusions and saved numeric fixture.
2. Add selected collector, recursive conversion-argument safety and complex tracking.
3. Test side effects, copy aliasing, selectors/bounds/cancel and legacy coexistence.
4. Lean, workspace/clippy, real-project probes, five implementation/test reviews and
   independent review; commit docs/PR, link stack and clean before #708.

## Plan self-review

1. Verify CPython mutant compilation for every conversion spelling and multiline input.
2. Execute precedence and exactly-once argument tests, including skipped __int__.
3. Exercise parameter/local/global/nonlocal/wildcard uncertainty separately.
4. Preserve eligible calls under normal runtime contexts while excluding target/type roles.
5. Pair numeric-only surviving verification with string-input type/value assertions.
