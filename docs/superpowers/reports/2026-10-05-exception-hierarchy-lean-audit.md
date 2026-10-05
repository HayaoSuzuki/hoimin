# Exception hierarchy: formal audit and implementation correspondence

Branch: `investigate/user-defined-exception-mutations`. Implementation baseline:
`ab0356a`. This audit revises the earlier decision to omit Lean. A graph-only
proof would have been insufficient, but that did not justify omitting models of
visibility, identity, index bounds and prepared-input consistency.

## Claim and correspondence worksheet

A hierarchy candidate must name distinct, related, trusted exception identities
visible at its use site. Building an index must not authorize classes from changed
prepared inputs or from a lower-priority module exposed by a transient deletion.
Retained index entries must respect their configured bound.

The following worksheet is fixed before choosing the exploration domain.

| Premise / observation | Lean representation | Production configuration / observation | Evidence and mode |
| --- | --- | --- | --- |
| Exception ancestry and direct parent/sibling relation | Module-qualified class identifiers and parent lookup; bounded ancestry walk | Owned Python class definitions; public plan replacement pairs | `plan::create`, `ExceptionIndex::resolve_class/replacements`; `strict` for generated fixtures |
| Constructor restrictions | Constructor-transparent path | Plain/custom source or inherited constructor; public raise/handler pairs | `resolve_class`, Python fixtures; `strict` |
| Lexical shadowing, including private names | Canonical binding keys and local-before-global lookup | Parameter/private parameter fixtures; public plan pairs | `Bindings`, `mangled_name`, `Collector`; `strict` for fixtures; arbitrary Python canonicalization excluded from the theorem |
| Import position and module identity | Natural-number load/use order and module-qualified identifiers | Earlier/later import, relative module context, reserved module origin; public plan pairs | `Module.loaded`, `resolve_relative_imports`, `module_name`; `strict` for fixtures |
| Index entry limit | Guarded insertion counter, arbitrary natural-number bound | Actual 65536-entry visibility boundary; public plan success/error and pairs | `build_visible`; `strict` at the production bound; reduced-bound sensitivity is `model-only` |
| Change/delete/build/restore ordering | Prepared/current input state and immutable successful cache | Owned fixture invokes `ExceptionProject::load` between filesystem events; observes load result, candidate pairs and fingerprint recheck | `exception_project`; `internal-fixture`, not public concurrency correspondence |
| Parser, OS, custom import hooks, hash collisions | Outside model | Existing parser/CPython/filesystem tests remain necessary | Not claimed as proved |

## Design and implementation plan

1. Define a small executable model and kernel-checked theorems for ancestry,
   shadowing, visibility, bounded insertion and snapshot/cache preservation.
2. Enumerate snapshot traces shortest-first and retain witnesses for deliberately
   broken validation, origin selection, shadowing, visibility and limit rules.
3. Generate all expected observations in Lean. Compare public plan fixtures in
   `strict` mode and snapshot schedules in `internal-fixture` mode. Adapters may
   normalize observations but must not compute expected values.
4. Register model, generator, freshness and sensitivity gates in the existing serial
   Lean CI lane. Run correspondence tests and repository checks; commit the artifacts.

Design reviews: (1) narrowed the claim to model properties plus exercised Rust
correspondence, (2) separated internal snapshot scheduling from public plan cases,
(3) kept parser/import-runtime facts outside the proof instead of assuming they
were verified. Plan reviews: (1) Lean owns source fixtures and expected values,
(2) broken variants must be detected before trusting green checks, (3) every Lean
command uses a wall-time/RSS guard and CI checks corpus freshness.

## Evaluation placement and limits

Imported modules contain definitions and kernel-checked proofs only. Enumeration,
serialization and broken-model checks live in the generator executable. Start the
snapshot alphabet at change/delete/restore/build, depth zero, and measure each
increment through depth three; do not increase after unexplained cost growth.
Use a 20-second interactive deadline, 2 GiB aggregate RSS cap and 250 ms sampling.
The repository CI lane retains its 30-second deadline. Theorems use explicit local
heartbeat limits, never unlimited search.

## Results and counterexample ledger

### Kernel-checked properties

`ExceptionHierarchyModel.lean` and `ExceptionHierarchyProofs.lean` contain 13
checked theorems, with no `sorry`, added axioms, `native_decide`, or unlimited
heartbeats. They establish the following inside the model:

- A successful bounded ancestry walk has a finite derivation to an accepted builtin
  root. Both endpoints of an emitted candidate have such a derivation.
- An emitted candidate is not used before its modeled load position; an opaque
  source binding prevents fallback to a global class. Module owners distinguish
  otherwise equal class names.
- Successful insertion, retained counts and arbitrarily many insertions respect
  the bound, provided the initial count is within it.
- Every finite event trace preserves the prepared-origin/cache invariant. Starting
  with an unrelated hierarchy cannot invent a candidate, even after restoration.
  Once built successfully, the cache is immutable under arbitrarily many events.

These are inductive theorems, not conclusions from the depth-three search. The
model represents only two builtin roots (`Exception`, `ValueError`), single-parent
classes, canonical binding keys, source positions and trusted-origin flags. It
assumes the supplied abstraction of Python identities/bindings/origins; the Rust
fixtures exercise particular translations into that abstraction. The missing-input
cache contains no eligible classes: its origin/digest labels denote the prepared
namespace, not a claim that missing bytes were read or hashed.

### Implementation observations

| Mode | Domain | Result |
| --- | --- | --- |
| `strict` | 72 cases: related/unrelated × three constructor policies × raise/except/except-star × four local/private binding contexts | All matched |
| `strict` | Two import-order, two relative-module-origin, three reserved-module cases | All matched |
| `strict` | Visibility tables of 65280, 65536 and 65792 entries at the real 65536 bound; public success/error, bounded candidate and truncation observations | All matched |
| `internal-fixture` | Two initial hierarchies × all 85 traces of length zero through three over change/delete/restore/build | All 170 matched |
| `model-only` | Broken-policy sensitivity and unbounded inductive theorems | 11 deliberately broken variants detected; 13 theorems checked |

The 252 corpus rows are generated exclusively by Lean. The public adapter calls
`plan::create`; the internal adapter uses real configuration preparation,
`ExceptionProject::load`, Ruff parsing, index collection and fingerprint rechecks.
Each row has an isolated temporary filesystem. Adapters collect every semantic
mismatch and fail, without rewriting expectations or reports. A case filter permits
single-case reproduction; no filter runs the entire mode.

The observation is a sorted **multiset** of original/replacement pairs. Public
candidate ranking is outside this audit; duplicates remain observable. The first
adapter run exposed this normalization omission in `candidate-true-1-1-0`:
actual order was `Child→Sibling, Child→Root`, while Lean supplied
`Child→Root, Child→Sibling`. A single-case run confirmed that the contents matched.
Sorting actual observations fixed the adapter; no expected value or production
rule was changed. Every generated source is also parsed before execution, so a
broken fixture cannot pass a negative case by being silently ignored.

Expected summary-limit failures and prepared-input changes are semantic outcomes.
Unexpected plan/load errors, parse failures, filesystem failures or fingerprint
resolution failures fail separately as `infrastructure-error`; fingerprint mismatch
is accepted only for the typed `RecordsChanged` result.

### Counterexample ledger

| Deliberately broken policy | Retained witness / observation | Status |
| --- | --- | --- |
| Validate only the final current fingerprint; accept changed bytes while indexing | Unrelated initial classes; `change, build, restore` yields a false candidate with a matching final fingerprint. Correct row `snapshot-false-35`: no pairs, load error, fingerprint matches | `--assert-old-safe` failed with exit 1 as intended; sensitivity detects it |
| On deletion, expose a lower-priority same-name module | `delete, build, restore` accepts a different origin. Correct row `snapshot-false-51`: successful opaque index, no pairs, fingerprint matches | Detected |
| Rebuild a successful cache from later inputs | Related initial classes; `build, delete, build` loses the existing candidate | Detected |
| Ignore ordinary/private local binding | Source parameter or class-private parameter versus an eligible global reference | Both detected |
| Ignore load position | Submodule imported after the use site | Detected |
| Conflate module identity | Relative imports identify different `Root` classes despite equal short names | Detected |
| Treat reserved interpreter module as a project origin | Untrusted origin versus the same eligible graph with trusted origin | Detected |
| Ignore constructor restrictions | Custom constructor in a raised exception versus a transparent chain | Detected |
| Permit insertion when count equals limit | Reduced bound 2, count 2; boundary corpus separately exercises actual production bound | Detected |
| Reject every candidate | Healthy direct-parent candidate | Detected |

No unresolved semantic mismatch or ownership decision remains in the exercised
domain. The broken variants live only in the generator executable; they are not
production changes. These controls establish that the specified rule distinctions
are observable, not that all possible incorrect implementations are detectable.

### Exploration and resource record

The explorer increases one depth at a time in a single guarded process. Both
snapshot restoration witnesses first appear at depth three.

| Maximum depth | Event alphabet | Traces per initial hierarchy | Total events per hierarchy | Explorer elapsed time |
| --- | --- | --- | --- | --- |
| 0 | 4 | 1 | 0 | < 1 ms |
| 1 | 4 | 5 | 4 | < 1 ms |
| 2 | 4 | 21 | 36 | < 1 ms |
| 3 | 4 | 85 | 228 | < 1 ms |

Every Lean command used a 20-second deadline and 2048 MiB aggregate RSS limit,
with 250 ms sampling. The model build took about 6.0 seconds / 663 MiB peak RSS;
proof compilation 3.6 seconds / 641 MiB; the final executable build 4.9 seconds /
770 MiB. Individual corpus/sensitivity commands completed below two seconds.

Infrastructure findings were kept separate from semantic results:

1. The sandbox initially denied the guard's process monitoring (exit 126).
   Running the same guard with process-monitoring access resolved it.
2. An attempted aggregate `lake build +HoiminOracle:o` scheduled unrelated missing
   dependencies together and was stopped by the RSS guard after 2.08 seconds
   (observed aggregate peak 2369 MiB; exit 125). The limit was **not increased**.
3. Direct aggregate checking then identified a missing existing `.olean`. Missing
   dependencies were checked individually in import order, followed by the
   aggregate library, each under the same guard. All 18 checks passed; the maximum
   was 6.9 seconds / 1270 MiB. No simultaneous Lean builds were retried.

The existing CI lane already compiles modules serially under individual guards;
this change registers the new model, proofs, executable, freshness and sensitivity
checks in that lane. Imported new modules contain only definitions and proofs;
all enumeration, source rendering and serialization remain in the executable.

## Follow-up self-review record

The design and plan each received the three passes recorded above, before the
model was implemented. Implementation and test reviews added the following:

| Stage / pass | Focus | Finding / action |
| --- | --- | --- |
| Implementation 1 | Does the model describe actual ownership and cache boundaries? | Kept private-name canonicalization and trusted-origin classification as explicit abstraction boundaries; strengthened identity sensitivity to compare graph eligibility, not merely identifier inequality |
| Implementation 2 | Can the adapter hide a failure? | Added fixture parsing, typed fingerprint-error handling, full mismatch collection and a single-case filter; retained duplicates during pair normalization |
| Implementation 3 | Can the proof/CI path become unchecked or unbounded? | Checked imports, absence of proof holes, local heartbeat limits, module/generator registration and serial guards; replaced aggregate dependency compilation with individual checks after the RSS stop |
| Tests 1 | Positive/negative coverage and real resource boundary | Reviewed the complete 72-case product, seven import cases, three actual-limit cases, and both initial states for every snapshot trace |
| Tests 2 | Would an incorrect policy be detected? | Observed the intentional old-policy RED result, verified all 11 sensitivity controls, retained minimal restoration schedules and reproduced the ordering-only adapter mismatch separately |
| Tests 3 | Freshness, implementation correspondence and integration | Verified generated-corpus freshness, 82 public and 170 internal matches, CI contract tests, formatting and all-feature Clippy; workspace regression result below |

## Reproduction and final checks

From `formal/HoiminOracle`, use the following guarded commands **sequentially**:

```sh
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 250 -- lake build +HoiminOracle.ExceptionHierarchyModel:o
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 250 -- lake build +HoiminOracle.ExceptionHierarchyProofs:o
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 250 -- lake build generate_exception_hierarchy
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 250 -- lake exe generate_exception_hierarchy -- --check corpus/exception-hierarchy.jsonl
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 250 -- lake exe generate_exception_hierarchy -- --sensitivity
```

For the intentionally refuted old claim, replace the last generator option with
`--assert-old-safe`; exit 1 and `change, build, restore` are the expected RED result.
Regeneration uses `--output corpus/exception-hierarchy.jsonl`; do not edit JSONL.

From the repository root:

```sh
cargo test -p hoimin-cli --test lean_exception_hierarchy_oracle -- --nocapture
cargo test -p hoimin-cli --lib lean_exception_hierarchy_snapshot_correspondence -- --nocapture
HOIMIN_ORACLE_CASE=snapshot-false-35 cargo test -p hoimin-cli --lib lean_exception_hierarchy_snapshot_correspondence -- --nocapture
HOIMIN_ORACLE_CASE=candidate-true-1-1-0 cargo test -p hoimin-cli --test lean_exception_hierarchy_oracle -- --nocapture
.venv/bin/python -m pytest -q tests/test_ci_workflow.py
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

- Focused correspondence: 252 matches, zero unresolved mismatches.
- CI workflow contract tests: 142 passed.
- Formatting and all-feature Clippy: passed.
- Full workspace regression: `cargo test --workspace` passed (exit 0), including both new adapters.

This audit proves the listed Lean properties and observes Rust correspondence for
the generated cases. It does not prove the entire Rust implementation, Python's
import/binding semantics, arbitrary import hooks, parser behavior, hash collision
resistance, or concurrent filesystem schedules. No Lean-to-Rust refinement theorem
is claimed; real concurrent scheduling is outside `internal-fixture` mode.
