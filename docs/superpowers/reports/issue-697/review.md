# Issue 697 self-review and evidence

## Implementation self-review: five passes

1. Arithmetic: decimal parser returns u64 magnitude in i128, leaving ample headroom
   for +/-1. Explicit symmetric bound filters endpoints; no unchecked narrow casts.
2. Ownership: signed numeric AST owns its operand, including unsupported spellings,
   so an out-of-range negative cannot accidentally produce a positive child mutant.
3. Context: inspected suppression save/restore across siblings, nested subscripts,
   Store/Del attribute/list/tuple targets; structural operators still visit slices.
4. Syntax: every replacement parenthesized; power and numeric-dot binding preserved.
   Reuse original byte spans, shared selection and candidate prefix storage.
5. Integration: enum appended, defaults untouched, Arithmetic ranking and inventory
   extended. Existing operator traversal continues in source order.

## Test self-review: five passes

1. RED: three tests fail for missing selector before production changes.
2. Membership/order: first GREEN attempt exposed a test assumption that CLI ranking
   was numerical. Fixed to compare mathematical membership independently, then
   assert identical ordered candidates across repeated plans. No production change.
3. Bounds: tests cover both +/-u64::MAX, out-of-range sources, zero to negative,
   signed child suppression and excluded decimal separators/bases/types.
4. Context/syntax: added nested subscript calls, Store and Del attributes, CRLF with
   Unicode and comments inside a signed literal, plus numeric-dot syntax.
5. Observation/resources: paired saved-plan probes assert passing baseline and both
   killed/survived counts; added opt-in/exclusion/limit/cancel analyzer regression.
   Tests use existing Python 3.14 symlink; debug/incremental off, clean per issue.

## Evidence

Initial new suite GREEN: 3 passed; existing analyzer: 279 passed, 3 ignored.
Lean: 3 kernel-checked model theorems, 3 boundary examples, 1 broken-rule witness,
exit 0 in 2.492s, external 20s deadline. Model-only, not Rust implementation proof.
Full suite, final focused run and lint results will be recorded after completion.

Final focused checks: new public tests 3 passed; analyzer 280 passed, 3 ignored.
All-target/all-feature clippy passed. OKF YAML/reserved-file checks: 29 passed.
Independent reviewer found no defects; probes additionally checked signed zero,
f-/t-strings, pattern guards, default-vs-annotation distinction and mixed-context
suppression restoration. Optional suggestion: retain the latter as a fixture.

## Real-package trials

See project-trials.json for versions, exact commands, outcomes and elapsed times.
Iniconfig 2.3.0 exceptions.py: 2 candidates, both survive import-only; both killed by
asserting ParseError's line-number string. Packaging 26.3 version.py: 114 candidates,
all compile under CPython 3.14; only the ranked top 10 were executed, and all survive
both small probes. All baselines passed; no timeout/error/inconclusive outcomes.
These are authored probes, not upstream suites, and equivalence remains unclassified.
The packaging top-10 probe largely exercises version-dependent branches; survival
alone does not establish equivalence or a missing upstream test.

Reproduction: copy installed package into a disposable directory. Run `hoimin plan
--root ROOT --file FILE --operators integer_literal_neighbor --max-candidates 1000
--min-free-space 1B --allow-best-effort-memory --baseline-timeout 10s
--mutant-timeout 10s --total-timeout 60s -- PYTHON314 -B -c TEST_COMMAND`; save plan
JSON, compile each applied mutation, then `hoimin verify PLAN --top 10 --format json`.
FILE/TEST_COMMAND are in the JSON. Temporary package copies were removed.

Rust verification uses `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
CARGO_INCREMENTAL=0` and the existing Python 3.14 .venv symlink:
`cargo test --workspace --offline`; `cargo test -p hoimin-cli --test
integer_literal_neighbor --test rust_analyzer --offline`; `cargo clippy --workspace
--all-targets --all-features --offline -- -D warnings`; `cargo fmt --all -- --check`.
Formal verification: `cd formal/HoiminOracle && lake env lean IntegerLiteralNeighbor.lean`
under a 20s process deadline.

Final full workspace run: **2,613 passed, 0 failed, 22 ignored**, 131 suite summaries.
