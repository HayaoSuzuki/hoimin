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
125 strict cases retain their expected candidates; 22 scope cases, four implicit-scope controls and nine class-cell controls are added.
The corpus now has 160 strict and 215 internal-fixture rows (45 extraction, 170
snapshot). No expected result is supplied by the Rust adapter.

## Proofs and sensitivity

The existing path-coverage and provider-invalidation proofs now quantify over
scoped keys. Two additional theorems establish that distinct scope numbers imply
distinct identities, and that an isolated write in one scope preserves a provider
imported under the same spelling in another scope. These are model statements;
the finite corpus provides exercised correspondence with Rust, not a Rust proof.

The existing 17 broken variants plus a flattened-scope variant are detected.
Snapshot exploration retains alphabet 4 and maximum depth 3 (85 traces at depth 3).
Alias fixtures comprise 17 existing plus 22 scope and six class-cell cases, with at most three edges.
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

Commands and final results are recorded below.
All Lean commands use `tools/lean_resource_guard.py --timeout-seconds 20
--rss-limit-mib 2048 --sample-ms 250`; proofs retain 50000 heartbeats. Lean jobs
run serially, and the aggregate check uses `-j1 -DElab.async=false`.

Additional original-design review: the older `Bindings` scan still treated
comprehension targets and lambda-local walruses as enclosing-scope declarations.
A new public regression reproduced the missing candidate (expected 1, actual 0).
The scan now skips comprehension targets and lambda bodies while retaining header
and outer-walrus effects. Four module/function controls pass and are retained in
the Lean-generated strict corpus as well.

Pre-review focused verification: `cargo test -p hoimin-cli --lib --test exception_hierarchy
--test exception_hierarchy_scopes --test lean_exception_hierarchy_oracle` passed
(765 library tests, 12 ignored; 22 hierarchy, four scope and one 151-case public
oracle test). Extraction and snapshot adapters matched all 209 internal rows.
`cargo clippy --workspace --all-targets --all-features -- -D warnings` passed.
Lean aggregate, corpus `--check` and `--sensitivity` passed. The aggregate took
10.047 seconds and peaked at 1260000 KiB RSS under the 20-second / 2-GiB guard.

## Independent review and fix

The independent read-only reviewer found one Important issue, no Critical or Minor
issues: free `__class__` receiver writes were resolved to a module name rather than
the owning class. For `Child.patch` assigning `__class__.__init__` to a required-arg
lambda, the planner still emitted `Root -> Child`. The regression returned one
candidate where zero was expected (RED). The lexical helper now retains each class
definition's binding for implicit cells; ordinary locals/globals still take priority,
methods and nested closures reach the owner, and headers stay outside that cell.
Six public controls pass, including direct, alias, nested, parameter, global and
header cases. Six extraction/public pairs plus three constructor-policy public
cases are generated by Lean; the latter model Child's changed constructor property.

Reviewer boundary rulings:
- Keep call argument/result and container aliases deferred: this increment improves
  direct lexical aliases; arbitrary Python effects can still escape the model.
- Keep type-parameter annotation scopes outside this increment: their full lexical
  behavior is not claimed; supporting them needs separate extraction tests.
- Keep both class-body lookup possibilities: sacrificing precision avoids relying
  on exact execution order.
- Keep model proofs separate from Rust refinement and process memory guarantees:
  finite correspondence and table caps are the available evidence.
- Fresh validation belongs to the parent run and is recorded below, rather than
  inferred from the reviewer's read-only assessment.

Fix review 1: local/global `__class__` bindings override the implicit cell.
Fix review 2: nested closures use the class definition binding; definition headers
use the containing scope, and alias edges preserve the resolved owning identity.
Fix review 3: one optional binding per bounded class scope preserves storage limits;
rebound definition names remain conservatively invalidated.

## Final verification

At `c5f288c`, `cargo test --workspace` passed: 2601 tests, zero failures,
22 ignored, across 129 binary/doc-test summaries. The five scope tests
include the six class-cell controls. All 160 strict and 215 internal-fixture corpus
rows match; all original 125 strict inputs/expectations remain unchanged except
for the internal corpus schema version. No new `sorry` or axioms occur in the model
or proofs. All 18 model theorems check; 18 broken variants are detected.

Commands (Rust commands used the disk-saving environment above):

```console
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

The guarded Lean commands, from `formal/HoiminOracle`, were:

```console
lake build +HoiminOracle.ExceptionHierarchyModel:o
lake build +HoiminOracle.ExceptionHierarchyProofs:o
lake exe generate_exception_hierarchy --output corpus/exception-hierarchy.jsonl
lake exe generate_exception_hierarchy --check corpus/exception-hierarchy.jsonl
lake exe generate_exception_hierarchy --sensitivity
lake env lean -j1 -DElab.async=false HoiminOracle.lean
```

Each command ran through the 20-second / 2048-MiB resource guard. The final reviewer
finding is fixed and tested; no Minor findings were deferred. Changes stay on the
requested branch, without pushing or merging.
