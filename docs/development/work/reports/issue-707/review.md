# Conversion removal review and evidence

## Implementation self-review

1. Eligibility: explicit eleven-name allowlist, exact one positional argument and no
   keywords/stars; only ExprName callee whose occurrence resolves to builtin.
2. Resolution: complex joins the shared tracked builtin set, allowing existing local,
   parameter, global/nonlocal and uncertainty handling. No runtime import/type tracing.
3. Source/effects: replacing the whole call with parenthesized argument text preserves
   precedence and one argument evaluation. It intentionally skips hooks, validation,
   copying and iterator consumption; no equivalence inference from observed types.
4. Context: RuntimeRole, pattern guard and annotation/PEP613 alias ranges exclude target
   and type positions. Generator/binding/suspension scan applies recursively to arguments.
5. Integration: original deletion safety defaults are unchanged for prior operators;
   conversion removal and legacy callee-name swaps retain separate IDs/spans.
   Shared prefix, selector and cancellation behavior remains in the common collector.

## Test self-review

1. RED: all four public tests failed for unknown operator before implementation.
2. Source: all eleven builtin names, Unicode/CRLF/comments, lambda, conditional and tuple
   expressions compile; 2*int(1+2) mutant evaluates to 6, preserving precedence.
3. Binding/roles: parameter/local/global/nonlocal/wildcard/exec uncertainty, direct/expanded
   argument exclusions, recursive generators, annotations/aliases and targets checked.
   Added explicit complex assignment/parameter/global regressions.
4. Behavior: argument called once with __int__ skipped; dict copy removal aliases the
   original. Saved numeric-only plan survives; string input with exact type/value kills.
5. Probes: iniconfig has no candidates. packaging has 11 syntactically valid candidates;
   import probe misses sampled 10, basic Version checks kill 3, expanded representation
   and component type/value checks kill all 10. One candidate unexecuted; no equivalence
   claim and no baseline failures/timeouts/errors. These are authored, not upstream tests.

## Formal audit

ConversionCallRemove.lean checked with lake env lean (exit 0, 2.205 seconds, 20-second
limit). Four theorems cover argument action/value/state preservation and identity
conversion correspondence. Three event-count witnesses distinguish one retained argument
event from an extra conversion event. Model-only; this does not prove source parsing,
name resolution, hook semantics or Rust implementation correctness.

## Verification

Four public tests and 292 analyzer tests passed (3 existing ignored). Full workspace
run and final lint checks recorded below; additional complex regressions rerun separately.

Final results: workspace command exited 0, additional complex public regressions passed,
and final all-target/all-feature workspace clippy (-D warnings), fmt, diff and OKF
metadata checks passed. Independent review found no concrete defect in source, binding,
context or semantics probes. External monkey patching remains outside the guarantee.
