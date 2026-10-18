# Task 8: integration of issues #992–#998

Implementation base: `50b9c089`. The focused report-oracle fixture repair was
committed separately as `f5f1a1f4a8f5268183643e6d5020cc11d804f5cc` after the first
workspace run found it. A second stale metrics expectation was investigated and
corrected separately in `7ea07bbf`: early admission is a controlled failure with
valid authorized stage metrics, not an outer transition error. Its independent
review and destination protection controls passed. All commands ran in the designated issue worktree on
macOS arm64. No Linux or Windows native success is inferred from these runs.
The controller owns independent reviews, PR preparation, push, hosted checks and
final cleanup; this record covers the implementer's local integration scope.

## Changes

- Capture cleans the new `hoimin-python` SBOM sidecar before generation and in
  final cleanup. The shipping graph requires exactly one core and one Python
  component with the CLI version. Fixtures model the root→Python→parser→stacker
  relationship and retain the direct root→core edge.
- Added success/failure cleanup and missing/duplicate/wrong-version component
  cases. Graph-order properties now permute all four dependency components and
  five dependency nodes. Existing parser-provenance corruptions remain active.
- Added the pure crate manifest description. Manual Windows CI runs only the
  native-name module with `--nocapture` to expose accepted fixtures versus SKIP;
  workflow contract tests recognize this one intentional platform-specific step.
- Updated current C4, development, compatibility, dependency and release guides.
  Updated analyzer, report, selection, lifecycle and SBOM knowledge concepts and
  cataloged the new design. Historical source metadata and evidence are retained.

## Requirements self-review: five passes

1. Scope: mapped integration to all seven accepted issues and kept changes to
   distribution, documentation and existing validation. No new product knobs,
   runtime dependencies, schema changes or compatibility adapters were added.
2. Identity: checked the common reader and core borrowed identity fields. The
   compatibility notice distinguishes invalid saved IDs from unchanged valid
   schema-8 output and mutant-ID v1 bytes; package floor remains 0.13.0.
3. Ownership: checked analyzer leases, CLI source/spool ownership, checked
   selection construction and the local stop-rank map. Documentation does not
   introduce a second payload, selection or mutable reservation authority.
4. Native limits: Windows output must expose fixture rejection. Linux invalid
   bytes and Windows lone-surrogate filesystem acceptance require native CI.
   APFS host results are explicitly insufficient for those claims.
5. Resource/authority: preserved shared target, two Cargo jobs/test threads,
   disabled incremental/debug info, sequential heavy work, one implementation
   owner and no push/PR/merge/release from this task agent.

## Design self-review: five passes

1. SBOM cleanup extends the existing fixed workspace-member inventory. It keeps
   the original lock restoration and finally cleanup flow; no generator redesign.
2. Internal package checks share the existing exactly-one/matching-version
   predicate. Missing and duplicate Python tests repair graph reachability first
   so the new version contract is the rejection under observation.
3. Fixtures add Python without renumbering existing parser/stacker component
   positions, keeping prior corruption cases meaningful; permutations cover the
   expanded complete graph instead of silently dropping its last member.
4. Native CI uses one Windows-only named step and bounded settings, while the
   ordinary workspace suite remains present. Contract tests check the exact
   additional step and preserve ordinary automatic/manual step correspondence.
5. Knowledge updates add inspected current sources with actual revision/hash
   metadata. Old audit snapshots remain evidence about their recorded revisions;
   no guessed future commit or invented verification actor is recorded.

## Implementation self-review: five passes

1. Traced capture success and changed-lock rejection through `finally`; all three
   sidecars are removed and changed lock bytes are restored before rejection.
2. Reviewed the version loop's short-circuit guard: empty matches cannot index
   element zero, duplicates fail, and the only component must match root version.
3. Inspected the fixture graph and transitive preservation assertion; Python is
   connected and the vendored parser/stacker path stays reachable.
4. Reviewed every changed current guide against actual APIs and policy ranges:
   `RunSelection`, `plan.selection()`, three ELv2 shipping exceptions, both fuzz
   lock entries and the shared Cargo analyzer boundary replace stale descriptions.
5. Reviewed the documentation source additions separately from historical
   metadata; source revision/hash/footnote checks are separate from factual claim
   inspection. New English sections preserve the existing concept context.

## Tests self-review: five passes

1. RED: five behavioral cases failed before production changes, two on the
   surviving Python sidecar and three because invalid Python components were
   accepted. Failures were assertions/expected exceptions rather than import or
   compilation errors (`task8-red.log`).
2. GREEN: the complete SBOM unit/property/Lean-corpus subset passed 288 cases.
   Expected component names, version and cleanup absence are independent values;
   mocks stand in only for Cargo tool invocation and generate real sidecar files.
3. AAA/scenario scope: corruption dimensions are parameterized into separate
   cases; success and changed-lock cleanup paths are separately parameterized.
   The ordinary valid graph remains a positive control and permutations include
   every component/dependency node.
4. Mutation sensitivity: selected tuple-element deletions removing Python from
   capture cleanup and version validation were both killed by the new behavioral
   tests. Each verify completed exit 0 with killed=1/survived=0 and no disk stop.
   One immutable mktemp directory held manifests/reports; its exact cleanup trap
   completed, retaining only concise summaries outside the repository.
5. Environment/evidence: initial full Python failures were traced to sandbox ps
   denial, then the entire suite passed outside sandbox. Workflow tools were
   installed in the task venv; eight PATH-dependent skips were rerun with that
   venv on PATH and all eight passed. Native platform limits remain explicit.

## Verification so far

All Cargo commands/hooks inherit `CARGO_TARGET_DIR=/tmp/hoimin-issues-992-998/target`,
`CARGO_BUILD_JOBS=2`, `CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0`,
`CARGO_PROFILE_TEST_DEBUG=0`, `RUST_TEST_THREADS=2`, `PREK_CONCURRENCY=1`, and
`CARGO_NET_OFFLINE=true`. Python uses only the task `.venv`, Python 3.14.7, and
`UV_CACHE_DIR=/tmp/hoimin-issues-992-998/uv-cache`.

- Frozen fuzz/workflow groups installed with `uv sync --frozen --group fuzz
  --group workflow --no-install-project`; initial sandbox network denial was
  resolved through approved escalation.
- `pytest -q tests/test_sbom.py tests/test_sbom_properties.py
  tests/test_sbom_oracle.py`: 288 passed.
- Workflow unit/linter subset: 176 passed, 8 tool-discovery skips; focused
  `tests/test_workflow_lint.py` with task venv on PATH: 8 passed.
- Initial full Python: 1065 passed, 8 skipped, four process-group/RSS guard
  failures from sandbox ps denial. Approved outside-sandbox full rerun:
  **1069 passed, 8 skipped**; those eight tool cases passed in the focused rerun.
- Ruff formatting/lint, ty, edited-workflow actionlint and offline zizmor passed.
- Selected SBOM mutations: IDs
  `m1_f346c5e0a60b296a4f8574eac2982e32113de5b9869877b8ab1879e7ddb514ec`
  and `m1_226ea83df2f804118a402e77c3a379c7d1d7ea890e624d82b555820c6c155cec`,
  two complete verifications, 2 killed/0 survived. Ordinary focused command first
  passed five cases. Release.py mutation evidence from the controller is reused;
  release.py was not changed or rerun here.
- Initial `cargo test --locked --workspace` stopped at three public tests in
  `lean_progress_decision_oracle`: fixture IDs were still synthetic. Routed to
  controller; separate focused repair `f5f1a1f` passed five tests and independent
  review. Final integration rerun follows the repair.
- One early invocation used nonexistent `tests/test_workflow_contracts.py` and
  ran no tests; the actual workflow files were then run successfully.

Logs/summaries are under `/tmp/hoimin-issues-992-998/task8-*` and
`sbom-mutation-*-summary.json`. These paths are local evidence, not shipped
artifacts. Final integration and documentation validation results follow below.

The first all-features run reached 1230 passed / 1 failed / 25 ignored across
79 summaries, then stopped on the stale metrics sidecar expectation. Its exact
focused RED is retained in `task-4-integration-red.log`; the aggregate log name
was reused by the final run. The final aggregate uses `--no-fail-fast` so later
targets are collected if another noncritical failure occurs.

## Completed Rust aggregate and focused repairs

`cargo test --locked --workspace --all-features --no-fail-fast` at `7ea07bbf`
completed **2879 passed, 2 failed, 33 ignored** across 167 suite summaries, exit
101. The only failed targets were `progress_details_heap` and `progress_heap`,
one test each, because their public-reader fixtures still used synthetic IDs.
The controller routed both together; fixture-only commit
`60dab172af34dc929edc2fb87f27b7724b7d8542` preserves allocation boundaries and
unchanged memory ceilings. Both focused targets passed (two tests total), hooks
passed, and independent review approved the correction. No production Rust
change followed the aggregate, so already-green targets were not repeated.
This is split aggregate-plus-focused evidence, not an exit-zero aggregate run.

The 33 ignored cases consist of ten release probes needing verified external
fixtures, thirteen opt-in benchmarks/measurements, nine subprocess helper
fixtures, and one descriptive sampling inspection. Native Linux/Windows
cfg-gated cases were not compiled/executed on this host and are separate from
those counts. The aggregate log and parsed summary are retained as
`task8-workspace-all-features.log` and `task8-aggregate-summary.json`; focused
repair evidence is `task-2-integration-heap-fix.log`.

## Remaining local gates

- Workspace/vendor formatting, workspace all-target/all-feature Clippy with
  warnings denied, vendored-parser Clippy and generated CLI-reference test/check
  passed after the focused fixture corrections.
- Strict boundary runner/report passed with ten matches, zero mismatches and
  zero infrastructure errors. Three report-only gaps remain explicit:
  preparation cancellation latency, delegated Linux hard enforcement, and
  native Windows PID/fault scenarios.
- All forty active performance gates passed, including hierarchy lookups, four
  stop-rank cases and the repaired progress heap target. This is registry gate
  evidence, not a new wall-time benchmark claim.
- Both Cargo metadata graphs resolved under `--locked --all-features`. Parsed
  root/fuzz lockfiles contain exactly the expected 0.13.0 internal packages.
  Normal core dependencies exclude CLI/I/O backends; pure Python dependencies
  exclude CLI/Tokio/SQLite/tempfile. Normal CLI features exclude `test-support`.
- cargo-deny 0.20.2 passed licenses and sources for both all-feature workspaces,
  with locked graphs and warnings denied.
- `maturin sdist` built the source archive. It retained existing missing
  description warnings for core/CLI; the new Python crate has its description.
  Archive inspection found only the parser LICENSE under vendor, while the
  packaged Cargo patch still points to missing parser source. Routed this named
  packaging finding to the controller before claiming source-build success.

## Source distribution follow-up: five review passes

1. **Requirements/cause:** the real extracted source build failed Cargo metadata
   because the root patch pointed to an absent vendored parser manifest. The
   archive contained only that parser's license. Source distributions must carry
   the same patched parser consumed by CLI/Python analysis, without changing
   runtime or license policy. Original evidence: `task8-sdist-build-red.log`.
2. **Design/scope:** added exactly 24 literal tracked vendored paths as
   sdist-only Maturin includes: manifest, license, provenance README and 21 Rust
   source files. No directory glob can collect build/cache/scratch files. This
   uses Maturin's documented `include` format selection
   ([configuration](https://www.maturin.rs/config)); no new generator or checker
   framework was introduced.
3. **Implementation/ownership:** actual archive contents equal those 24 paths,
   all paths are tracked and all includes are sdist-only. The test creates only
   one owned temporary sentinel directory, removes it even on failure, and uses
   the declared Maturin dev dependency through the current Python interpreter.
   It requires no additional CI tool or build job.
4. **Tests:** initial real-archive regression was 4 failed / 3 passed, matching
   the absent manifest, two sampled sources and README. Final regression is
   27 passed: all 21 Rust files plus manifest/provenance/API bytes are compared
   independently of the include list, and untracked build/cache/scratch output
   exclusion is checked. The final focused workflow/packaging subset is
   **211 passed**, with all real workflow tools available on PATH.
5. **Integration/limits:** the corrected extracted source builds a release wheel
   under pinned Rust 1.99.0, then its temporary tree is removed. One attempt
   first used host default Rust 1.97.1 outside the checkout; it correctly failed
   the declared rust-version and is retained in `task8-sdist-toolchain.log`.
   Explicit `RUSTUP_TOOLCHAIN=1.99.0` produced the successful build in
   `task8-sdist-build-green.log`. Runtime Rust code is unchanged, so the approved
   aggregate-plus-focused Rust evidence remains applicable.

## Final artifact, fuzz and documentation results

- Checkout release wheel and wheel built from the extracted sdist both passed
  `tests/wheel_smoke.py` outside the sandbox in isolated temporary environments.
  These checks exercise real CLI mutation behavior, help/metadata, Python path
  handling and original source preservation. Receipts are
  `task8-wheel-smoke.json` and `task8-sdist-wheel-smoke.json`; they identify
  macOS 15.7.7 arm64 and Python 3.14.7. No publication occurred.
  Checkout wheel SHA-256:
  `549230fee38a004ffc8ed9a304af387710e1e1468e51f1bf4dbe7257ed83ed32`;
  extracted-sdist wheel SHA-256:
  `31108ef4191141db32f9f52a2da54a3c5a2c6d4c6da614e0e430c3a4e3f4c568`.
- `cargo +nightly-2026-10-08 fuzz build` built all seven binaries against the
  final runtime code. Native runs with `ASAN_SYMBOLIZER_PATH=/usr/bin/atos`
  exercised 38 committed fixed inputs plus 128 bounded runs per target (896
  total), with 4096-byte inputs, 5-second input timeout and 1024 MiB RSS cap.
  All exited zero, with zero sanitizer errors and zero symbolizer warnings.
  The earlier sandboxed warning was not reproduced with this current binary
  and native invocation; no sanitizer setting was disabled. Exact per-target counts/logs are
  in `task8-fuzz-summary.json` and `task8-fuzz-*-{fixed,bounded}.log`.
- Final Ruff formatting (56 maintained files), Ruff lint, ty and `uv lock
  --check --offline` passed. The lock check resolved 85 packages unchanged.
- OKF structure: safe YAML parsing and reserved-file checks passed all 49 bundle
  pages. Reference checks separately passed all 14 new source hashes/revisions,
  source footnotes, changed links, six updated concept/index routes and the
  new design catalog row. Existing draft statuses and unrelated source metadata
  remain intact. Seventeen historical moved-path source entries were retained
  with explicit recorded-revision/snapshot retrieval guidance; this check does
  not claim that every historical current-tree link resolves.
- Claims were separately read against core identity/selection/stop, pure Python
  API/hierarchy, workspace disk/copy and controlled-failure metrics sources.
  Format checks are not asserted as verification of every historical claim.

Consulted/updated OKF pages: analyzer, session-report, selection-plan-verify,
runtime-lifecycle, release-sbom and design-documents. Native Linux invalid-byte
and Windows accepted-surrogate execution remain controller-owned CI evidence,
as do Linux hard resource enforcement and Windows PID/fault report gaps.
No new Lean theorem/model was changed; host Rust adapters and the strict
registry provide the executed model-correspondence evidence in this scope.

All local implementation work is ready for independent review. Repository
commit hooks remain enabled; actual commit/hook outcome is recorded in the SDD
report and `task8-commit.log`. The controller retains PR-preparation reviews,
remote actions, native CI and final task-owned artifact cleanup.
