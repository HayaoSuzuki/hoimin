# Exception scope and outcome audit

Issue #692 follow-up; working branch `investigate/user-defined-exception-mutations`.

## Claim and model boundary

Attribute writes follow may-alias edges between lexical identities. Equal spellings
in different scopes do not denote the same vertex. A local parameter rename must
not change a candidate in an unrelated module. Reachable imported providers remain
invalidated. No-related-class and constructor-policy exclusions are distinct from
incomplete analysis, and only incomplete analysis produces a bounded diagnostic.

The lexical helper precollects declarations, resolves global/nonlocal/free names,
skips class namespaces for methods, preserves class-body fallback, and separates
headers, lambda bodies and comprehension scopes. Comprehension walruses bind in
the containing scope and first iterables run outside the implicit scope.

The model starts with extracted lexical keys; it does not prove Python parsing or
Rust extraction. Call argument/result aliases, containers, arbitrary dynamic Python,
flow-sensitive class namespace ordering, and type-parameter annotation scopes are
not modeled. Class reads conservatively include both class and outer bindings.
The pre-existing conservative global-declaration invalidation is retained.

## Correspondence worksheet

| Premise / observation | Lean representation | Production / adapter | Mode |
| --- | --- | --- | --- |
| Lexical identity | `ScopedName = Nat × String` | `BindingKey {scope, name}`, serialized as the full pair | internal-fixture |
| Import owner, assignment, direct write | `AliasCase` facts | Parse `patcher.py`; inspect retained facts and closure | internal-fixture |
| Provider write reachability | `WriteReach`, `writeClosure`, `aliasTrusted` | Reverse worklist and provider invalidation | internal-fixture |
| Candidate after unrelated rename or actual escape | `eligible` using `aliasTrusted` | Real `plan::create`, complete candidate vectors | strict |
| Snapshot timing | Existing four-event model | Existing snapshot adapter | internal-fixture |
| Diagnostic reason, count and location | Not formalized | Public tests for explicit outcomes | Rust-only verification |
| Actual resource caps | Not reduced into a Lean bound | Parser/public tests at production boundaries | Rust-only verification |

Corpus schema 2 intentionally changes internal alias keys from strings to pairs.
The user-facing candidate and diagnostic schemas remain unchanged. All previous
125 strict cases retain their expected candidates; 22 scope cases and four implicit-scope candidate controls are added.
The corpus now has 151 strict and 209 internal-fixture rows (39 extraction, 170
snapshot). No expected result is supplied by the Rust adapter.

## Proofs and sensitivity

The existing path-coverage and provider-invalidation proofs now quantify over
scoped keys. Two additional theorems establish that distinct scope numbers imply
distinct identities, and that an isolated write in one scope preserves a provider
imported under the same spelling in another scope. These are model statements;
the finite corpus provides exercised correspondence with Rust, not a Rust proof.

The existing 17 broken variants plus a flattened-scope variant are detected.
Snapshot exploration retains alphabet 4 and maximum depth 3 (85 traces at depth 3).
Alias fixtures comprise 17 existing plus 22 scope cases, with at most three edges.
No larger exhaustive search was introduced.

## Reviews and rulings

Design reviews: (1) declaration identity precedes propagation, (2) class-body and
method lookup must differ, (3) excluded sites must not be called failed analysis.
Plan reviews: (1) public RED cases precede implementation, (2) adapter preserves the
entire key, (3) production resource limits require actual boundary tests.
Implementation reviews: (1) lexical declarations and late nonlocals, (2) escaping
imports and headers, (3) retained storage and failure before publication.
Test reviews: (1) positive shadowing versus real escaping writes, (2) class/private,
lambda/comprehension and all local-binding forms, (3) old corpus preservation,
new boundary tests, full workspace and lint integration.

Resource ruling: retain 65536 entries per declaration set and 131072 total across
scopes, because 65536 alias assignments inside one function must remain supported
alongside the function declaration itself. Cost is the documented larger bounded
declaration budget. No process peak-memory guarantee is inferred from these caps.

Disk ruling: use `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0
CARGO_PROFILE_TEST_DEBUG=0` for builds and commit hooks. Rebuild/debugger convenience
is traded for lower disk use; generated artifacts may be cleaned after validation.

## Verification record

The initial scope regression failed for eight unrelated-shadowing cases and the
module parameter rename. All passed after scoped resolution. The initial Lean
build encountered a reserved-word collision in a local variable; it was corrected.
The first adapter run rejected the obsolete hardcoded corpus count before semantic
comparison; schema/count guards were updated to the generated case set.

Commands and final results are recorded below when the integration gates finish.
All Lean commands use `tools/lean_resource_guard.py --timeout-seconds 20
--rss-limit-mib 2048 --sample-ms 250`; proofs retain 50000 heartbeats. Lean jobs
run serially, and the aggregate check uses `-j1 -DElab.async=false`.

Additional original-design review: the older `Bindings` scan still treated
comprehension targets and lambda-local walruses as enclosing-scope declarations.
A new public regression reproduced the missing candidate (expected 1, actual 0).
The scan now skips comprehension targets and lambda bodies while retaining header
and outer-walrus effects. Four module/function controls pass and are retained in
the Lean-generated strict corpus as well.

Focused verification: `cargo test -p hoimin-cli --lib --test exception_hierarchy
--test exception_hierarchy_scopes --test lean_exception_hierarchy_oracle` passed
(765 library tests, 12 ignored; 22 hierarchy, four scope and one 151-case public
oracle test). Extraction and snapshot adapters matched all 209 internal rows.
`cargo clippy --workspace --all-targets --all-features -- -D warnings` passed.
Lean aggregate, corpus `--check` and `--sensitivity` passed. The aggregate took
10.047 seconds and peaked at 1260000 KiB RSS under the 20-second / 2-GiB guard.
