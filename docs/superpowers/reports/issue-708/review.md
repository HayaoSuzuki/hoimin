# Optional keyword deletion review and evidence

## Implementation self-review

1. Identity: index only supported top-level functions, requiring one unconditional module
   binding and matching lexical resolution. Shared enum resolver extraction preserves its
   existing semantics; missing NameScopeKind import was caught and corrected by compilation.
2. Signature: validate positional capacity, unique known keywords, no positional-only
   keyword binding or double assignment, and all required parameters present. Only explicit
   defaulted parameters enter removal; default values themselves are not re-evaluated.
3. Source: independent review found compact return(f)(...) becoming returnf(); added a
   failing regression and wrapped the complete reconstructed call. Preserve Unicode source
   callee/keyword spelling and retained expression order, normalize inter-argument trivia.
4. Hazards/roles: function escapes, defaults/attribute mutation, relevant globals and dynamic
   namespace access exclude candidates. Pattern/type/target guards are shared. Document
   forward/recursive source, class namespace, type-parameter and __-name exclusions.
5. Resources: index only when selected, poll scans and replacement loops, bound per-call
   emission to max+1; shared retention/IDs/selectors remain authoritative. No Python import
   or runtime tracing in candidate generation; external reflection is not guaranteed.

## Test self-review

1. RED: five public tests failed for missing operator; the later compact-return regression
   separately failed with the lexical defect before its fix.
2. Syntax/binding: first/middle/last/sole optional keywords, positional and keyword-only
   defaults, required and positional-only validation, multiline comments, Unicode and
   parenthesized callees compile. Invalid original argument bindings produce no candidates.
3. Semantics: saved escaping fixture survives result-type-only check and is killed by exact
   output. Default factory runs once; removed expression effects vanish; remaining positional
   and keyword expressions execute once in source order.
4. Scope/integration: alias/rebinding/local/global/unsafe/type/target exclusions plus selector,
   bounds/cancel tests pass. Enum regressions validate factored shared identity resolution.
   Independent probes additionally checked method/class lookup and annotated/walrus escapes.
5. Real code: iniconfig __init__.py and packaging/version.py have no eligible candidates.
   packaging/tags.py hits the existing duplicate unary-not fact-index panic, recorded as an
   infrastructure failure rather than zero candidates. CPython 3.14.7 difflib has two valid
   candidates: import-only probes miss both; HtmlDiff callback-use assertions kill both.
   These are authored probes, not upstream suites; no equivalence conclusion is claimed.

## Formal audit

OptionalKeywordDelete.lean passed lake env lean (exit 0, 2.115 seconds, 20-second limit).
Six theorems prove target absence, other-parameter preservation, existing default selection,
no deleted-argument state effects, and required/positional-only exclusions. Two witnesses
show explicit False versus default True. Model-only: not a proof of signature extraction,
name resolution, whole-call source reconstruction or the Rust implementation.

## Verification

Focused tests pass: 6 public keyword tests, 5 enum tests and 293 analyzer tests (3 existing
ignored). Independent follow-up verified the compact-call fix and found no remaining
correctness defect. Full workspace and final lint results are recorded below.

Final results: corrected `cargo test --workspace --offline` exited 0. Final all-target/
all-feature workspace clippy with -D warnings, fmt/diff checks and OKF metadata checks
passed. A static final audit found all 13 × 4 × 5 numbered self-review passes present,
and no sorry/axiom/native_decide in the thirteen new Lean files.
