# Task 9 implementation report

Base: 8d58c121a8121e2eec87f1b54d48cd9cb0140a63. Requirements: task-9-brief.md and all resolved supplements. Controller's plan edit is authorized input. No subagents used.

## Requirements/design: five distinct review passes

1. Contract review: exact default 16,777,216 bytes; `analyzer.payload_limit`; UTF-8 logical retained payload, not RSS. Plan can reject before global count selection and existing record validation. Kept 2 MiB validation after selection.
2. Lifetime review: plan owner must include converted earlier files; handler owner must survive detached work and later requests. Non-Clone leases follow owned candidates; data fields precede lease fields. Terminal plain Discovery handoff only after generation/conversion ends.
3. Copies review: prefix membership clones replacement, merge cloned replacements, core validation owns Latin-1 and ID/path/hash scratch, store owns serialized Vec. Selected exact borrowed lookup, borrowed merge mask, core-local conservative scratch reservation, store-local exact encoded size reservation. Enum index moved outside collector to borrow destination strings; hierarchy replacements also borrow.
4. Producer continuation review: found additional unit-returning container and optional-keyword callbacks beyond hierarchy. They must stop generating after admission failure. Container's formatted fragment Vec also holds scalable copies: replace with borrowed source fragments and reproduce formatting only in the one in-flight candidate. Annotation batch remains at most three source-dependent strings as approved.
5. Verification/interface review: production request constructors and included shipping analyzer modules need direct migration, including fuzz target and standalone integration harnesses. Use bounded deterministic owner snapshots, one registered case per finite scenario, release ignored probes for pinned normal fixtures, no vacuous fixture skipping. Design score target 10/10: core accounting formula resides beside allocations, budget hides synchronization/overflow/release, no lazy-edit framework. Remaining implementation review must verify all callback exits and all move/drop boundaries.

## TDD

Initial test compile caught incorrect test access (`EffectFailed.code` vs `failure.code()`); corrected before behavioral RED.

RED command (worktree root):
`CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --offline --locked -p hoimin-cli --lib large_eight_element_population_fails_at_payload_admission -- --test-threads=2`
Log: `/tmp/hoimin-design-audit-5d3aee95/task-9-red.log`.
Expected real failure: left `analyzer.store`, right `analyzer.payload_limit`; 0 passed, 1 failed, 1000 filtered; 0.17s. This proves the 8-element ~1.2 MB source retained the population and reached record rejection before the new admission guard existed.

Final implementation, verification, measurements and completion evidence follow below.

## Implementation: five distinct review passes

1. Admission/order review: exact string lookup uses `(span.start, typed operator) -> HashMap<String, lease>` and borrowed `&str`. Duplicate and nonwinner probes allocate no identity; true count overflow still records truncation. Winning eviction drops the old key/candidate before reserving replacement payload. Existing emission sequence and canonical operator ordering remain.
2. Ownership/drop review: `Charged<T>` declares `value` before `lease`, and `PayloadLease` is non-Clone. Conversion destructures lease before candidate so failures drop candidate first; metadata grows the same lease. Plan keeps charged converted candidates across files and sheds accounting only at terminal plain Discovery handoff. Handler keeps one shared owner across serial requests and detached workers. Store setup shares that owner; disk records retain no leases.
3. Hidden-copy review: merge uses borrowed keys plus a keep mask and drops references before removing candidates. Enum/hierarchy destination collections borrow semantic-index strings. Container and optional-keyword fragment lists now borrow source text and format only the in-flight candidate. Remaining source/fact/import strings belong to declared excluded analysis state. A final audit removed even the small decimal-neighbor string batch: an iterator owns numeric values and constructs only the requested next replacement. No additional scratch exclusion is needed; annotation batch remains the approved at-most-three source-dependent exclusion.
4. Continuation/error review: prefix stores first admission failure. AST generator entry/loop/sibling checks prevent subsequent replacement construction; container/optional-keyword callbacks return continuation, hierarchy callback returns Result and stops later sites. Annotation callback stops before constructing later replacements. Distinct cancellation survives. Inspection found conversion used historical `EffectId(0)` for all errors: a new RED test demonstrated loss of run identity (0 vs 41). Internal conversion now receives effect identity from plan/run; no adapter was introduced.
5. Encoded/validation review: core-local `candidate_validation_scratch_bytes` accounts both encoding buffers, identity copies, lowercase hash, normalized path intermediates and temporary ID. Core exports `MUTANT_ID_BYTES` beside hashing so CLI metadata has no duplicated magic ID length. Store computes record size once, rejects oversized records before record allocation, then reserves the known encoded bytes next to the Vec. Existing core validation behavior and IDs remain unchanged.

Software-design-philosophy diagnostic: 10/10 target met by the implementation boundaries (all eight rows): each module has one responsibility; reserve/retain/leases hide checked arithmetic and lifetime synchronization; core owns validation allocation knowledge and store owns encoding size; interfaces document logical-byte scope; the review includes copy/lifetime design; producer-specific code owns its borrowing/continuation decisions; explicit field order and terminal-handoff comments explain boundaries; time spent on source-copy removal avoids a general lazy-edit framework. Existing `rust.rs` remains large; changes stay in its existing collector rather than unrelated restructuring.

## Intermediate verification findings (not hidden)

- New payload suite first ran 21 passed / 3 failed. The three failures were `conversion_scratch_exact_limit_preserves_id_and_transfers_payload`, `cross_file_plan_exact_limit_hands_off_complete_plain_output`, and `cross_file_plan_retains_converted_candidates_in_shared_budget`. Independent arithmetic had omitted 4 bytes: boolean payload is 159; scratch is `4+5+5+4+15+64+64+4+4+67 = 236`; peak is 395 (not 391). A second converted file adds 159, giving 554 (not 550). Corrected exact and plus-one test constants; production accounting was unchanged. Subsequent suite: 30 passed / 0 failed, 0.15s.
- Production check passed before test migration. Compilation then identified direct-inclusion harnesses and test terminal projections that still expected cloneable/plain candidates; migrated those consumers rather than adding an API adapter. Clippy identified redundant generated tail guards/default expressions and two long existing collector methods; removed tail guards and used explicit types, documenting required in-method early-stop flow for the two long methods.
- Effect-ID RED: `CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --offline --locked -p hoimin-cli --lib conversion_payload_failure_preserves_run_effect_identity -- --test-threads=2`; log `/tmp/hoimin-design-audit-5d3aee95/task-9-effect-red.log`; 0 passed / 1 failed, actual `EffectId(0)`, expected `EffectId(41)`. Corrected the conversion context; the final focused run passed the GREEN verification.

## Verification: commands, counts, and five review passes

All Cargo commands use `CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0`. All test commands use `-- --test-threads=2` (probes additionally use `--ignored --nocapture`). No installs/syncs, main-checkout target writes, concurrent Cargo invocations, or external benchmark workload were used.

1. Behavioral/TDD review: initial 8-element RED became GREEN (1 passed, 0 failed; `task-9-green.log`). Conversion effect-ID RED became GREEN inside the final broad focused run. New payload coverage totals 33 named tests including two bounded 32-case properties. Exact and plus-one UTF-8 length boundaries, sum/accumulation overflow, empty/zero, duplicate/nonwinner, winning eviction, finish transfer, all three simultaneous producers, cross-file conversion, enum/hierarchy overlap, annotation stop, Latin-1 scratch, metadata rejection, encoded-buffer overlap, partial spool cleanup and detached owner reuse all passed. Assertions use independent exact byte totals rather than RSS thresholds.
2. Compatibility review: final pre-numeric-refactor focused command was `cargo test --offline --locked -p hoimin-cli --lib --test rust_analyzer --test lean_bounded_candidate_discovery_oracle --test container_element_delete --test optional_keyword_delete --test enum_member_replace --test exception_hierarchy --test exception_hierarchy_scopes --test lean_exception_hierarchy_oracle --test valid_python_corpus --test plan -- --test-threads=2`. Log `task-9-focused.log`, exit 0: lib 1015/28 ignored; container 4; enum 5; hierarchy 22; hierarchy-scopes 5; bounded-discovery Rust oracle adapter 5; hierarchy oracle 2; optional keywords 6; plan/verify 105/1 ignored; rust analyzer 334/3 ignored; valid Python corpus 319/3 ignored. Aggregate 1822 passed, 35 ignored. Child-process self-tests print their own summaries and are not double-counted. This includes existing store roundtrip/record-size tests and selection/ID/order/dedup/truncation checks. No Lean proof execution is claimed or substituted for Rust correspondence.
3. Final-refactor/core review: the only subsequent production changes were lazy decimal-neighbor iteration and the probe panic description. Covering command `cargo test --offline --locked -p hoimin-cli --lib neighbor -- --test-threads=2` passed 7/7 (`task-9-neighbor.log`). `cargo test --offline --locked -p hoimin-core --lib --test candidate_policy --test source_encoding --test source_encoding_properties -- --test-threads=2` passed 63, ignored 2 (`task-9-core.log`: 34+19+8+2). These commands cover the final production source. Earlier test-compilation failures were direct-inclusion ownership migrations, including a terminal projection used after move; they did not run tests and were corrected before the successful broad command.
4. Resource/fixture review: exact large-source bytes remain 1,200,042 / 1,200,080 / 1,200,160 for 8/16/32 elements. Charged peaks were 16,500,625 / 16,126,257 / 16,465,165; admissions 5/4/4; generated original copies 6/5/5; actual replacement-builder calls 6/5/5; replacement bytes built 6,300,264 / 5,625,490 / 5,813,550. Every error released used bytes to zero. Unit probes use `a.py` (4 path bytes); public CLI probes use `source.py` (9), so their exact path charges differ. Peaks include successful partial reservations for the triggering failed admission, not only candidates accepted into a prefix. All remain <=16,777,216 and stop before retaining the entire population.
5. Delivery/evidence review: release ignored probes exercise the production whole-plan path, verify pinned SHA-256 before analysis, fail on missing/changed files and fail clearly if invoked in debug. The existing Django duplicate-unary-not debug assertion remains unchanged; release qualification passed. Fuzz target directly includes the shipping modules and now shares request ownership and compares borrowed candidate values. Separate-workspace compile/lint use the same target directory; compilation does not mean fuzz execution. All required commit hooks passed; their results are recorded below.

### Actual normal charged peaks

Command (full prefix as above): `cargo test --release --offline --locked -p hoimin-cli --lib analyzer::payload_probes -- --ignored --nocapture --test-threads=2`.
Log `/tmp/hoimin-design-audit-5d3aee95/task-9-charged-peaks.log`; extracted `/tmp/hoimin-design-audit-5d3aee95/task-9-charged-peaks.jsonl`. Release compilation: 1m43s; tests: 10 passed / 0 failed, 0.14s. Default features, repository thin-LTO release profile. The test statistics add instrumentation not present in the public binary.

| Fixture/profile/operators | Candidates | Charged peak bytes | Successful reservations/releases |
| --- | ---: | ---: | ---: |
| proofreadtools/full/default | 31 | 6,508 | 124/124 |
| proofreadtools/focused/default | 31 | 6,508 | 124/124 |
| proofreadtools/full/boolean_literal | 0 | 0 | 0/0 |
| django/full/default | 1,309 | 322,322 | 5,236/5,236 |
| django/focused/default | 1,289 | 318,681 | 5,156/5,156 |
| django/full/boolean_literal | 55 | 10,183 | 220/220 |
| generated/full/default | 9,000 | 1,542,957 | 36,000/36,000 |
| generated/focused/default | 9,000 | 1,542,957 | 36,000/36,000 |
| generated/full/boolean_literal | 0 | 0 | 0/0 |
| all three files/full/default | 10,340 | 1,970,914 | 41,360/41,360 |

All `terminal_used` values were zero at the final plain Discovery handoff; no producer reuses that emptied owner. The multifile case retained earlier converted candidates while analyzing later files. It uses a 20,000 count ceiling to include all 10,340; individual cases have fewer than 10,000 and match before-matrix counts. Peak scope includes producers, retained identity copies, merge/selection, converted metadata and core validation scratch across all target files; plan record-size counting allocates no encoded record buffer. Run encoded-buffer overlap is covered separately.

Pinned source SHA-256 values:
- proofreadtools: `baaca907fb57acb1aee3a9e32f37958e59daeec7a39cc4a37a07096f06550f75`
- django: `924cb2668bbe7b7f55bfe204528984bd484398f0e0911d1fbfa9967438f809ea`
- generated: `6e0ed0bde15d87b67a13fbbe24e81f91c5c0a143abf0a2d567c172f351750495`

These are conservative charged peaks, distinct from controller's before final-string totals 6,228/322,071/1,542,696. Internal `analysis_seconds` observations in JSONL come from two test threads and are not an aligned CLI runtime-overhead comparison. Controller owns the after-commit public CLI runtime/RSS/hash matrix; no runtime-overhead conclusion is claimed here. `cargo test --release --lib` does not rebuild `target/release/hoimin`; controller must rebuild that binary before its after matrix.


## Fuzz and commit quality gates

Both commands used the full controlled Cargo environment above and
`CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-971-979-cli-analyzer-resource-safety/target`:
- `cargo check --offline --locked --manifest-path fuzz/Cargo.toml --bin python_analyzer`: exit 0, 14.91s, `task-9-fuzz-check.log`.
- `cargo clippy --offline --locked --manifest-path fuzz/Cargo.toml --bin python_analyzer -- -D warnings`: exit 0, 0.99s, `task-9-fuzz-clippy.log`.

This is compilation/lint evidence only; no libFuzzer execution was performed. The 4,096-source-byte/32-candidate harness bounds, immediate cancellation assertion and ordered-prefix comparison remain in the migrated harness.

Commit command: `CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_NET_OFFLINE=true git commit -m 'fix(analyzer): bound retained candidate payload across discovery'`.
Index staging and commit used the required authorized `require_escalated` route; no hooks were bypassed and no push occurred.
Log: `/tmp/hoimin-design-audit-5d3aee95/task-9-commit.log`.
Mandatory hook output:
- cargo fmt (workspace): Passed
- cargo fmt (vendored parser): Passed
- cargo clippy (workspace, all targets/all features, -D warnings): Passed
- cargo clippy (vendored parser): Passed

## Completion: five distinct review passes

1. Contract closure: checked the brief's exact 16,777,216-byte default, checked arithmetic, analyzer.payload_limit mapping, plan wrapper preservation, no successful shortening, separate selected-record limit, and the approved source/AST/one-in-flight/annotation exclusions against implementation and resource guide. Scalable producer batches are borrowed or removed; decimal replacements add no extra exception.
2. Ownership closure: verified one work-owned plan budget and one handler-owned run budget, charged candidates through producer finish/merge/conversion, prior converted files retained in the plan, store-buffer overlap and actual detached-worker release. A test hook pauses a worker holding a real charged candidate; handler reuse cannot forget those bytes. Cancellation/error paths delete partial spools. Terminal plain Discovery is the only production untracking boundary.
3. Verification freshness: broad required suites passed before the final narrow numeric-batch refactor; seven covering neighbor tests, core tests, release production-path probes, fuzz compile/lint and full mandatory hooks cover final sources. No unrelated suite was rerun after a successful result without a changed-code justification. Task10 owns the integrated final gate. Public after-CLI runtime/RSS/hash comparison belongs to controller after Cargo release and is not falsely represented as complete here.
4. Reviewable scope: staged only 24 task files, including controller-authorized Task9 plan clarification. No dependencies, Cargo lockfiles, unrelated fact-index fixes, CLI options, compatibility adapters or main-checkout target writes. `.superpowers` report stays ignored; it was not force-added. Existing large analyzer file remains a maintenance constraint, explicitly noted above.
5. Evidence/delivery closure: retained both behavioral RED cases, initial arithmetic mistakes and corrected independent totals, actual charged peaks and all limitations. No Django debug result is claimed; release qualification succeeded. All commit hooks passed. Cargo slot was released to controller after the commit, with no build/probe process left running. Only final ignored-report writing and read-only git status remained after release.

Status: DONE. Commit: `e4e3a1d44ed390d95d4e605079c5ce4f6975926a` (`fix(analyzer): bound retained candidate payload across discovery`).

Known limits: this is a retained logical payload guarantee, not process RSS or all construction scratch. Conservative validation reservations can reject before the exact allocation peak. Resource failure may precede global selection or record validation. Django's pre-existing debug-only duplicate-unary-not assertion is unchanged; release fixtures passed. Fuzz target was compiled/linted, not fuzzed. Controller's public after matrix is a delegated follow-up, not an unreported implementation test.

## Files committed

- `crates/hoimin-cli/src/analyzer/exception_hierarchy.rs`
- `crates/hoimin-cli/src/analyzer/exception_project.rs`
- `crates/hoimin-cli/src/analyzer/exception_project_oracle_tests.rs`
- `crates/hoimin-cli/src/analyzer/mod.rs`
- `crates/hoimin-cli/src/analyzer/payload.rs`
- `crates/hoimin-cli/src/analyzer/payload_probes.rs`
- `crates/hoimin-cli/src/analyzer/payload_tests.rs`
- `crates/hoimin-cli/src/analyzer/rust.rs`
- `crates/hoimin-cli/src/analyzer/rust/container_elements.rs`
- `crates/hoimin-cli/src/analyzer/rust/depth.rs`
- `crates/hoimin-cli/src/analyzer/rust/enum_members.rs`
- `crates/hoimin-cli/src/analyzer/rust/finally_transfer_tests.rs`
- `crates/hoimin-cli/src/analyzer/rust/loop_transfer_tests.rs`
- `crates/hoimin-cli/src/analyzer/rust/method_replacement_tests.rs`
- `crates/hoimin-cli/src/analyzer/rust/optional_keywords.rs`
- `crates/hoimin-cli/src/analyzer/rust/whole_expression_selection_tests.rs`
- `crates/hoimin-cli/src/analyzer/rust_tests.rs`
- `crates/hoimin-cli/src/analyzer/store.rs`
- `crates/hoimin-cli/tests/rust_analyzer.rs`
- `crates/hoimin-cli/tests/valid_python_corpus.rs`
- `crates/hoimin-core/src/candidate.rs`
- `docs/development.md`
- `docs/superpowers/plans/2026-10-18-issues-971-979.md`
- `fuzz/fuzz_targets/python_analyzer.rs`

## Controller public release comparison (completed after implementation)

Default-feature public binary rebuilt at e4e3a1d44ed390d95d4e605079c5ce4f6975926a with the same bounded Cargo environment; `cargo build --offline --locked --release -p hoimin-cli --bin hoimin` passed in 50.41s. SHA-256 bf6caf83c55f3fb4c52733552f95e7e3c8d8241b33f0af3cc33fe3367e3b3b74. Build log: task-9-release-after-build.log. The bounded archived measure-payload-normal.py and measure-payload-large.py drivers ran sequentially, with no Cargo/compiler overlap. Exact commands use --binary target/release/hoimin --revision e4e3a1d44ed390d95d4e605079c5ce4f6975926a, and normal additionally --inputs /tmp/hoimin-design-audit-5d3aee95/analysis-inputs --build-profile release.

Five controller evidence reviews:
1. Provenance: both builds use default features/release/incremental=0; pinned source hashes and repetition counts match. Before binary/revision and raw observations remain preserved.
2. Behavior: all 27 normal runs exited 0 with empty stderr. Per paired run, source SHA, canonical candidate SHA, count, truncation, final logical string bytes and candidate JSON size match exactly.
3. Overhead: three-run medians below are observations, not thresholds. Generated/full wall increased about 1.65 ms (3.6%); generated/focused was nearly unchanged. No claim of universally faster normal analysis.
4. Large-source failure: all nine after runs exit 2 with `plan.discovery: source.py: candidate payload exceeds internal limit 16777216 bytes`; same source bytes/SHA as before; no watchdog fired. Before failure was the later 2 MiB record limit. The earlier explicit failure is the approved contract.
5. Memory scope: RSS remains supplementary, distinct from instrumented conservative charged peaks. The 32-element RSS median decreased from 125,370,368 to 26,902,528 bytes, but source/parser/AST/in-flight construction remain excluded from the payload guarantee. Target3.3GiB/free117GiB; no cleanup needed before final review fixes.

| Input/operators/profile | Wall median before / after (s) | RSS median before / after (bytes) |
| --- | ---: | ---: |
| proofreadtools/default/full | 0.009750 / 0.007233 | 8503296 / 8388608 |
| proofreadtools/boolean_literal/full | 0.006789 / 0.006444 | 7979008 / 7815168 |
| proofreadtools/default/focused | 0.006155 / 0.006044 | 8372224 / 8470528 |
| django/default/full | 0.015649 / 0.014998 | 11845632 / 12042240 |
| django/boolean_literal/full | 0.008435 / 0.007998 | 9928704 / 9895936 |
| django/default/focused | 0.015161 / 0.014819 | 11862016 / 11960320 |
| generated/default/full | 0.045955 / 0.047601 | 23887872 / 21315584 |
| generated/boolean_literal/full | 0.008983 / 0.009009 | 11681792 / 11501568 |
| generated/default/focused | 0.048183 / 0.047801 | 23724032 / 24150016 |

| Large elements | Wall median before / after (s) | RSS median before / after (bytes) |
| --- | ---: | ---: |
| 8 | 0.018707 / 0.013361 | 38436864 / 29065216 |
| 16 | 0.028387 / 0.013775 | 72974336 / 26492928 |
| 32 | 0.050612 / 0.012931 | 125370368 / 26902528 |

Raw evidence: task-9-normal-release-before.jsonl, task-9-normal-release-after.jsonl, task-9-large-release-before.jsonl, task-9-large-release-after.jsonl and task-9-public-comparison.json. Each matrix has three repetitions per case.


## Review fix round 1 — subscript continuation after payload failure

Independent review `task-9-review.md` found one Important issue at e4e3a1d4: `collect_subscript` continued from failed index-neighbor admission into the mapping replacement helper. This fix is restricted to that continuation boundary and its regression. The controller's public measurements above remain measurements of e4e3a1d4; they are preserved without rerunning or relabeling them.

Five requirements/design review passes:
1. Requirement review: the approved first-failure contract requires stopping construction before a later mapping replacement; returning the same PayloadLimit only after constructing it is insufficient.
2. Control-flow review: `collect_index_neighbors` stops its own loop after failed admission, but its caller continued to `subscript_to_mapping_get_replacement`. Numeric key `1` passes the existing mapping-key gate, so `x = items[1]` exposes the gap.
3. Ownership review: an immediate return after neighbor dispatch leaves existing RAII cleanup and sticky error propagation intact. No new allocation owner, budget scope, reservation calculation or diagnostic is needed.
4. Test-design review: one independently registered AAA case enables both relevant operators, sets a one-byte budget and observes the real helper-entry and allocation counters. It asserts PayloadLimit, zero retained bytes and one triggering generated candidate. The direct subscript helper positive control already proves the counters see an allocation, so zero entries cannot be inferred merely from candidate counts.
5. Module-design review: the collector owns the sequencing decision, and a local guard is the smallest useful change. Software-design score for this bounded change: 10/10; all eight diagnostic rows remain satisfied, with no new interface, leaked accounting knowledge or redundant abstraction. The existing large analyzer module remains the previously recorded broader maintenance constraint.

Five implementation review passes:
1. Placement: the new guard follows the complete slice/index dispatch and precedes mapping gates and helper entry, covering the reported index transition without altering dispatch order.
2. Successful-path preservation: an absent failure takes exactly the existing mapping gates, builder and admission path; no operator selection, range, replacement or sorting rule changes.
3. Error/lifetime preservation: the guard reads the sticky candidate failure and returns locally; the existing result propagation reports PayloadLimit and releases the prefix. It does not clear or replace any error.
4. Instrumentation: the new case uses the existing thread-local counters and resets them before its single analysis action. Production helper code and counter placement are unchanged; the real helper positive control remains available.
5. Scope/diff: product changes are three lines in `collect_subscript`; the only test change is one 24-line case in `rust/method_replacement_tests.rs`. No dependencies, interfaces, payload formulas, fixtures or public CLI behavior changed beyond stopping the forbidden post-failure work.

Behavioral RED evidence: `task-9-fix1-red.log`, command below, exited 101 after compilation (15.04 s). One test failed at the work-counter assertion: actual `([0, 0, 0, 0, 0, 0, 1], 1)`, expected `([0, 0, 0, 0, 0, 0, 0], 0)`. PayloadLimit, zero retained bytes and one generated candidate assertions preceding it passed. This demonstrates the concrete extra helper entry/allocation, rather than a compiler failure or candidate-count proxy.

All commands in this round use `CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0`. RED and GREEN use:
```sh
cargo test --offline --locked -p hoimin-cli --lib subscript_payload_failure_stops_before_mapping_builder -- --test-threads=2
```
Logs are under `/tmp/hoimin-design-audit-5d3aee95/`. The required final integrated suite remains Task 10; this round intentionally does not repeat the unrelated 1,822-test run or public before/after matrices.

GREEN `task-9-fix1-green.log`: exit 0, 1 passed, 0 failed/ignored; compile 13.44 s and test 0.01 s. The same test and assertions were retained unchanged after adding the guard.

Focused verification `task-9-fix1-focused.log`:
```sh
cargo test --offline --locked -p hoimin-cli --lib --test rust_analyzer -- payload_tests method_replacement_tests structure_index_neighbor structure_slice_neighbor structure_replacements_preserve_grouped_calls_and_mapping_delimiters --test-threads=2
```
Exit 0; 59 registered executions passed (46 library + 13 direct-inclusion integration), 0 failed, 0 ignored. Compile 11.49 s; suite times 0.14 s and 0.01 s. Output contains no warnings. These selections cover all 33 existing payload cases plus the relevant helper and subscript behavior; the same analyzer module is intentionally checked in its two compilation contexts.

Five verification review passes:
1. Failure specificity: the RED trace reaches only the work-counter mismatch, proving the tiny fixture triggers the reported bug through production parsing, collection, admission and builder code. It records one real extra mapping allocation.
2. Regression sensitivity: adding the local guard alone changes that assertion to zero entries/allocations; the failure type, budget release and triggering generated count remain checked by the unchanged regression.
3. Positive-path coverage: the direct helper control, selected-family helper tests, grouped mapping delimiters and decimal index/slice fixtures all pass; an unconditional return at the new location would break selected mapping output coverage.
4. Resource-lifetime coverage: all 33 existing payload cases pass, including exact boundaries, conversion scratch, shared producers, cross-file accumulation, detached-worker retention and store cleanup. No new budget arithmetic or lease operation is introduced by this fix.
5. Scope/provenance: the final source revision consists only of the guard and regression. The focused command tests that revision with two test threads and the required constrained build environment; no broad suite, benchmark, release probe or fuzz execution is claimed for this fix.

Commit: `4ebed4457519920e9a28f8f653d8ce96985acdad` — `fix(analyzer): stop subscript builders after payload failure`. Two files, 27 insertions. Authorized escalated `git add` named only `crates/hoimin-cli/src/analyzer/rust.rs` and `crates/hoimin-cli/src/analyzer/rust/method_replacement_tests.rs`. The ignored report remains unstaged; no forced add or push occurred.

Mandatory hook command used the same four build environment values plus `CARGO_NET_OFFLINE=true`:
```sh
git commit -m 'fix(analyzer): stop subscript builders after payload failure'
```
`task-9-fix1-commit.log`: exit 0. Workspace fmt, vendored-parser fmt, workspace Clippy and vendored-parser Clippy all reported Passed. No hook was bypassed. Post-commit `git status --short` was empty.

Five completion review passes:
1. Finding closure: the single Important review finding is addressed at the exact inter-builder boundary. Real production-path RED/GREEN evidence establishes the missed allocation and its prevention.
2. Acceptance closure: the new one-case regression checks explicit PayloadLimit, zero outstanding charged bytes, one triggering generation, zero mapping-helper entries and zero replacement allocations. It passes in both compilation contexts.
3. Change integrity: committed changes are only the three-line stop guard and its 24-line regression. Existing diagnostics, candidate ranking, source handling and accounting rules remain untouched; the working tree is clean.
4. Evidence integrity: fresh focused checks and all four mandatory hooks passed. Earlier e4e3a1d4 public observations remain labeled and preserved above. This round does not reinterpret those timings as measurements of the fix commit or claim integrated/fuzz execution.
5. Handoff/limits: no unresolved finding or new architectural obstacle remains in this round. The previously documented retained-payload scope, Django debug qualification and fuzz-execution limitation remain unchanged. Independent re-review and Task 10 integrated verification belong to the controller. Cargo is released with no running build/test/probe from this implementer.

Review fix round 1 status: DONE.
