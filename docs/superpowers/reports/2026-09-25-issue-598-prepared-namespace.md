# Prepared namespace verification and review (#598)

Baseline: `4e3bc2a97cad2e4ce8ae2b11c970fc2f5dff9bb2`. Worktree: issue-598. Reviews below are self-reviews by the implementation agent, not independent approvals. Execution outcomes are recorded separately from review observations.

## Design reviews before implementation

1. **Semantic boundary review.** Compared the issue's 12-case runtime identity model with the proposed static class flag. Found that the empty custom-metaclass case cannot retain its historical candidate expectation: static proof has no execution result. Fixed the design to model runtime identity and conservative eligibility separately and preserve the runtime-positive/static-negative control.
2. **Lookup precedence review.** Read both resolver functions and class/comprehension scope construction; ran an isolated CPython 3.14 nonlocal fixture. Observed a prepared `any` overriding an explicit class `nonlocal any` via `LOAD_FROM_DICT_OR_DEREF`. Fixed the design to preserve global bypass but avoid asserting nonlocal bypass; methods and comprehension bodies skip the entire class before either directive handling or namespace guard.
3. **Coverage/precision review.** Traced generic class header traversal and declaration annotation lookup. Found the policy must cover class-visible annotation scopes and first comprehension iterables, but must not taint the class's own bases. Added explicit header/declaration boundaries and documented precision loss for `class C(object)` / `metaclass=type`, arbitrary mapping behavior, and unsupported dynamic builtins mutation.

## Plan reviews before implementation

1. **Spec-to-task review.** Mapped each design boundary to Task 1/2; found public `run` could complete with zero candidates without independently demonstrating baseline Python semantics. Added explicit CPython endpoint probes and all three injection patterns to public run coverage.
2. **Execution/API review.** Inspected existing analyzer helper signatures and CI Lean runner. Found the draft's generic test instruction omitted concrete test code and the resource-guard path; added the exact regression shape and project guard invocation. Tests must select a specific operator, not count unrelated mutation families.
3. **Evidence/independence review.** Checked planned corpus fields against the replay adapter and review obligations. Found an eligibility-only corpus could pass despite incorrect fixture assumptions. Added runtime identity observations generated from Lean, closed schema validation, exact positive candidate spans/pairs, and distinct implementation/test reviews. No Python helper edits are planned, so Python mutation workflow is not applicable.

## Execution

Implementation and correspondence are complete; final gates are recorded below. Root reported baseline `cargo test --workspace`: 2265 passed, 22 ignored across 94 suites; this agent has not rerun that baseline.

Task 1 RED: `cargo test -p hoimin-cli --lib prepared_namespace -- --nocapture` failed all three regressions on the unchanged resolver (direct custom class, builtin annotation, source-only global declaration). After the resolver fix, a test also counted the tuple-literal mutation inside `list(())`; replaced arguments with `values` to isolate the callable mutation. This was a fixture defect, not a resolver defect. Task 1 GREEN: `CARGO_BUILD_JOBS=2 cargo test -p hoimin-cli --lib` passed 683 tests, 12 ignored. No baseline tests required expectation changes.

Lean RED: a model returning `true` for all namespace trust failed `trusted true false = false` by `decide` (3.640 s, 592736 KiB peak RSS). The corrected model and explicit-premise soundness theorem built in 5.468 s, 625520 KiB peak RSS. Both used 20 s / 2048 MiB guards and heartbeat 10000; no bound was increased. The first sandboxed guard attempt failed to inspect the process table (infrastructure error); subsequent authorized guarded runs used process-table access.

Task 2 correspondence: 39 isolated CPython 3.14 inputs and public plans matched Lean expectations; all three public run inputs (source-only/destination-only/both injections) completed with zero killed mutants and empty mutant lists. Adapter schema test passed. The original 12-case matrix is retained, with custom empty class namespace now runtime-positive/static-negative. Additional controls exercise plain and empty-header classes, inherited/aliased/dynamic metaclasses, arbitrary mapping lookup, directives, closure, comprehension placement, defaults, exception pairs, deferred class/method annotations and generic class headers.

## Implementation reviews

1. **Resolver precedence pass.** Re-read `resolve_scope`, `resolve_annotation_scope`, class parent fallback, global and nonlocal routing. Confirmed lexical class skipping and global resolution precede the new flag; the flag precedes ordered AST fallback and nonlocal resolution. Source-only/destination-only globals remain suppressed unless both endpoints bypass the class. No production defect remained; added corresponding public controls to pin this boundary.
2. **Header and scope-construction pass.** Traced positional/starred bases, named/unpacked keywords, class type parameters, nested class and method construction. The only flag initialization is at class construction, after traversing its header. Empty headers default false and methods start their own scopes. Added public `__mro_entries__` and generic-header controls. A first generic fixture returned `object`, conflicting with implicit `Generic` MRO; CPython caught this as an infrastructure error. Corrected the fixture to return a normal `Base` without changing expected semantics.
3. **Shared-consumer and compatibility pass.** Checked all builtin-pair callers and annotation builtin provenance. Found that import-alias provenance uses a separate `annotation_import_stable` path; explicitly excluded that path from the guarantee rather than claiming all annotation provenance was fixed. Preserved runtime exclusion in type positions and documented conservative loss for `object`/`type` headers. Existing library expectations remained unchanged.

## Test reviews

1. **Negative/positive sensitivity pass.** Checked all callable families and exception routes in regressions, then reviewed positive contexts. Found the `list(())` fixture counted its nested tuple literal; isolated arguments as `values`. Verified bare/empty classes, module/method/closure, nested plain class, type parameters and both-directive controls retain candidates. All three original regressions were observed RED before the production edit.
2. **Runtime independence pass.** Reviewed generated sources against model premises, including globals/nonlocals and annotation lookup. Added four public controls for nonlocal preparation, global annotation bypass, method annotation class visibility and generic headers. CPython identity comparisons use Lean-generated booleans independently of analyzer output; public run baseline is executed explicitly even when candidate count is zero. The runtime and static expectations for an empty custom mapping stay different intentionally.
3. **Adapter and CI integrity pass.** Found a count-only parser would accept replacement of one unique case with an unrelated ID; added exact closed case identities and an unknown-ID rejection test. Checked schema version/mode/unknown fields/missing/duplicate rows, pair/spans/symbol checks and timeout classification. Inspected CI model/module lists, executable target and freshness/sensitivity entries. Clippy flagged an unnecessary operator string clone; removed it. Generator development failures (reserved identifier and record indentation) were syntax/infrastructure errors, never counted as semantic mismatches.

## Claim and correspondence boundary

A builtin/exception pair admitted by this policy has two builtin endpoints, assuming ordinary class namespaces have no injected endpoints and module/builtins state is not dynamically modified. For the four nullable-annotation fixtures, the first observation checks `int` identity and the second is an explicit constant `true`: the literal `None` introduces no name lookup. These fixtures exercise builtin annotation provenance, not a claim that `int | None` itself is a builtin name. The Lean theorem quantifies five Booleans (32 valuations); fixtures enumerate 39 scope/header/directive cases, not execution traces. There are no transitions, trace depth or growing search bounds. Parsing, arbitrary runtime mapping implementation, the Python compiler and Rust execution remain outside the proof.

| Premise / observation | Lean representation | Public setup / observation | Mode |
| --- | --- | --- | --- |
| Class-visible lookup per endpoint | `sourceVisible`, `destinationVisible` | module/class/method, directives, comprehension and annotation fixtures; CPython identity | strict |
| Ordinary namespace known statically | `ordinary` | bare or empty class header versus any base/keyword header | strict |
| Endpoint overrides | `injectSource`, `injectDestination` | metaclass dict/mapping; `is builtins.*` in Python | strict |
| Runtime identity | `runtimeBuiltin` | isolated CPython `subject.observed` | strict |
| Candidate eligibility | `allowed` | public `plan`, exact count/operator/pair/span/symbol | strict |
| Killed-count exclusion | Three matrix fixtures selected by `runCheck` | explicit Python baseline plus public `run` JSON | strict |
| Deliberately broken lookup policies | Three `broken*` functions | model witnesses only | model-only |

Sensitivity covers ignoring preparation, checking only the source endpoint, and incorrectly capturing the class in a method. The first two are boundary/precedence failures; the third checks safe positive controls. Atomicity and idempotency do not apply to this pure lookup predicate. No unresolved same-premise mismatch remains in the exercised corpus. Empty custom mappings are intentionally runtime-positive/static-negative; treating that precision change as a runtime mismatch would compare different predicates.

The fix covers shared builtin and exception pairs plus builtin annotation provenance. Import/module aliases in annotations use the separate `annotation_import_stable` path; this change does not establish their safety under custom mapping lookup. Arbitrary dynamic globals, monkeypatched builtins/`__build_class__`, and proving that explicit `object`/`type` headers are safe are excluded. Runtime evidence is macOS arm64 with CPython 3.14.7; CI definitions were inspected but GitHub Actions was not executed locally.

## Reproduction and maintained gates

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test -p hoimin-cli --lib prepared_namespace
CARGO_BUILD_JOBS=2 cargo test -p hoimin-cli --test lean_prepared_namespace_oracle
CARGO_BUILD_JOBS=2 cargo test --workspace
cargo fmt --all -- --check
CARGO_BUILD_JOBS=2 cargo clippy --workspace --all-targets --all-features -- -D warnings
```

From `formal/HoiminOracle`, use the existing guard (process-table access required):

```sh
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 250 --stats /tmp/prepared-model.json -- lake build HoiminOracle.PreparedNamespaceModel
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 250 --stats /tmp/prepared-fresh.json -- lake exe generate_prepared_namespace -- --check corpus/prepared-namespace.jsonl
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 250 --stats /tmp/prepared-sensitive.json -- lake exe generate_prepared_namespace -- --sensitivity
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 250 --stats /tmp/prepared-stats.json -- lake exe generate_prepared_namespace -- --stats
```

To reproduce the minimal original case independently, take corpus row `class-true-false`, save its `source` to `subject.py` in an empty directory and run `hoimin plan --root <directory> --file subject.py --operators collection_any_all --allow-best-effort-memory --min-free-space 1B -- <absolute-python> -c 'import subject; assert subject.result == "custom"'`. Expected: no candidates. Changing `plan` to `run --format json` must report no killed mutants. The historical baseline emitted one unwanted candidate; the pre-fix Rust regressions confirmed the same faulty resolution before this implementation.


### Final gate findings and repairs

The first full workspace run found three pre-existing `type_parameter_bindings` tests expecting five direct class-body candidates under nonempty headers. Those expectations contradict the reviewed conservative namespace policy. Changed only those five `# keep` markers to `# prepared namespace`; header, method, plain-class and outer-module expectations remain. The focused eight-test suite passes after this intentional precision update. This differs from Task 1's library-only result, where no existing expectation changed.

Root's independent Python CI gate found a missing `LEAN_CORPUS_BY_EXECUTABLE` registration. After adding the data entry, the focused contract exposed generator-order drift between CI and lakefile; moved the CI entry to match. The focused workflow contract passed (1 passed, 39 deselected). `tests/test_ci_workflow.py` changes one metadata entry and no production Python; the hoimin-mutation-testing skill forbids targeting test modules, so there is no production Python mutation target for this edit. These two CI repairs extend test review pass 3.

Root's independent reviewer approved the code and separately replayed all 39 CPython fixtures. That is independent evidence reported by root, not a substitute for this agent's executed checks.


## Executed verification

- Task 1 library: 683 passed, 12 ignored. Focused generic integration after precision update: 8 passed.
- Final public oracle: 3 Rust tests passed; 39 CPython identity/plan comparisons and 3 explicit baselines/public runs.
- Main Lean library (`lake build HoiminOracle`): passed; independently cloned existing cache, 3.673 s and 1249408 KiB peak process-tree RSS. New model proof: 5.468 s / 625520 KiB; final generator: 2.501 s / 623456 KiB; final executable freshness (including rebuild): 5.406 s / 773024 KiB; sensitivity: 0.568 s / 57152 KiB. All used 20 s deadlines, 2048 MiB RSS limits, `-j1` and heartbeat 10000 for local theorems. No timeout, RSS stop, unbounded heartbeat, or larger search run occurred.
- Freshness negative control: intentionally appended a newline to a temporary copy; `--check` exited 1 with `stale prepared namespace corpus`, as required. The committed corpus was not changed by this check.
- Formatting and whitespace checks passed. OKF YAML structural validation parsed all 29 Markdown files in `docs/knowledge/`; targeted source/link checks are recorded below.
- Root independently ran Python tests: 104 passed plus 227 subtests; Ruff format checked 19 files and Ruff lint passed. Vendor parser formatting and dedicated clippy were reported passing by root on the unchanged shared vendor revision.

Final workspace verification passed: 2277 tests passed, 22 ignored across 95 suites. Core contracts passed: 320 tests, 2 ignored across 26 suites. CLI contracts passed: 1942 tests, 20 ignored across 69 suites. Final all-target/all-feature clippy passed with `-D warnings`. No GitHub-hosted run, non-macOS execution, or live PR publication is claimed by this report. Root owns independent final review and publication.


The final adapter-only delta gives plan fixtures an identity-only baseline command, because some annotation/header fixtures have no `subject.result`; run fixtures retain both identity and result assertions. Root reviewed this change without findings. After the complete suites, the exact oracle was rerun with contracts (3 passed, 39 identity/plan cases and 3 public runs), followed by final all-feature clippy and formatting. Whole workspace and contracts suites were not repeated for this test-command-only change.

OKF final checks: all 29 catalog Markdown files passed YAML structure checks; all four added source records matched current content hashes, had matching citations/footnotes, and resolved to existing files. Existing catalog metadata was preserved. The updated analyzer topic remains reachable through the existing index, and the new design/report are registered in the reference indexes. Content review checked the conservative class criterion, source/destination global distinction, runtime-versus-static expectation, annotation limits and actual test results against code and reports. No `verified` metadata or human review was invented.

Completion: design, plan, implementation and tests each received three documented self-review passes; independent review additionally approved the implementation, expectation changes and CI registration. Root owns wheel packaging, push and PR creation. No merge or publication was performed by the implementation agent.
