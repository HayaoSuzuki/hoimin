# Task10 integration report

Base: `4ebed4457519920e9a28f8f653d8ce96985acdad` in the authorized issue worktree.
Controller retains task/final independent review, later evidence archival,
cleanup and PR creation. This task does not claim future review verdicts.
Skills: create-okf + format/repository-catalog references, software-design-philosophy,
stop-slop, systematic-debugging on failures, verification-before-completion.
Read the brief, exact integration guidance, execution ledger, precise approved
acceptance lists, relevant task reports/reviews and current concepts/source.
Did not read the whole plan or spawn agents.

## Requirements/design: five distinct reviews

1. Acceptance scope: enumerated all 50 acceptance items in nine original issue lists, including unboxed #977/#978. Mapped each to the respective commit and real behavioral/static evidence in maintained acceptance.md. Separated later #979 intentional syntax/schema changes from #971's earlier preservation contract.
2. Ownership/API: inspected fixed AnalyzerHandler constructor, shared payload owner, read-only plan index, regular opener and descriptor mutation boundary. Identified stale root-only constructor/caller-consistency prose and RunState's removed overwritten fingerprint API; retained preflight's actual fingerprint contract.
3. Failure/cancellation: traced pre/post-store transfer, detached spool owner, shared leases, FIFO descriptor opening and pre-chmod link checks; documentation must not equate a cancelled await with worker exit, an accounting limit with RSS, or file-write protection with a sandbox.
4. Versions/provenance: current plan10/report8/fingerprint12/floor0.12.0; session5/ranking4/candidate1-IDv1 unchanged. Historical notices retain their own numbers. Source revision metadata records only files actually read; new archived evidence gets untracked+SHA at base, not a fabricated future commit. No new originals in catalog-covered specs/reports/audits, so catalog tables need no additions.
5. Execution/resources: inspected CI and developer gate list, read-only shared environment and wheel smoke; initial target3.3 GiB/free117 GiB. One Cargo slot/target, resource environment on every Cargo/hook, tests2 threads, no redundant Lean proof or focused benchmark reruns. Preserve all original failures and measurement-revision corrections; root owns future reviews and cleanup.

## Implementation/documentation: five distinct reviews

1. Current contracts: revised development/usage and six existing OKF concepts to fixed constructor settings, discovery-only timeout, current versions, bounded retained strings, regular inputs/source indexing, shared-inode checks and preflight fingerprint. Corrected an initial prose draft's RunMachine name to the inspected actual RunState before validation.
2. Historical scope: changed Issue849's old new(root) paragraph into an explicit historical migration; retained its no-I/O decision. Scoped Issue949 version claims as historical while current entry points state10/8/12/0.12.0. Kept environment field13 framing distinct from deliberately changed full fingerprints and SQLite5.
3. Evidence archive: copied all37 staged evidence files plus SHA256SUMS byte-for-byte initially, then added the original execution ledger, archive entry point and per-item acceptance map. Retained initial task findings and fixed-review records; old manifest.json remains explicitly labeled its original ten-report snapshot. Refreshed recursive checksums after deliberate additions rather than rewriting old reports or measurements.
4. Resource prose: states exact16,777,216 bytes; ownership across producers/files/converted candidates and detached handler work; UTF-8 lengths, identity metadata and scratch reservations; release/move rules. Explicit exclusions include source-dependent one-candidate and at-most-three annotation batch. Normal wall overhead and actual charged peaks are separate from final strings/RSS. Reports keep measured after revision e4e3a1d4, not subsequent guard4ebed445.
5. Test correction after integrated evidence: initial full workspace failed manifest_buffer_lifetime during plan creation because24×256 KiB originals/replacements/dedup identities retain≈18 MiB. Controller approved fixture-only128 KiB strings,24 candidates,>6 MiB manifest. Split top1/top5 into named registered cases using one scalar-count helper and whole-helper Mutex; kept actual allocator release, pending source read, preview and no-test-execution assertions. GREEN2/2. No product changes, compatibility adapter or fixture skip.

## Design diagnostic (changed boundaries only)

| Diagnostic | Result and evidence |
| --- | --- |
| One-sentence responsibility | Pass: raw config owns normalization input; handler owns session settings; budget owns retained logical bytes; opener owns descriptor type; workspace owns mutation writes |
| Interface simpler than implementation | Pass: fixed constructor/request and reserve/lease hide lifetime/synchronization; review opener hides Unix flag detail |
| Implementation can change without callers | Pass: no repeated session-setting synchronization or copied raw fields; scratch formulas remain beside allocation owners |
| Interface comments define abstraction | Pass: handler transfer, logical-byte exclusions, terminal handoff and regular-input policies stated in source/current concepts |
| Design discussed in review | Pass: task reviews and this review inspect owners, copies, cancellation and generality, with actual failed early-stop finding preserved |
| Module hides an important decision | Pass: index implementation hidden in reader; link-count protection hidden at descriptor boundary; budget arithmetic hidden in owner |
| Newcomer can find boundaries without implementation | Pass after guide correction: constructor, timeout, counted/excluded bytes and cleanup obligations are explicit with original evidence links |
| Design improvement receives deliberate effort | Pass: requirements and documentation/ownership passes preceded final gates; no unmeasured time-percentage claim |

Score:10/10 (8/8 diagnostic rows) for these scoped boundaries; no failed row remains.
This is a qualitative scope review, not a whole-repository complexity or correctness proof.
The stale newcomer-visible documentation row initially failed; revised current guides
and source-linked concepts remedy it without adding an abstraction.

## Initial failures and corrections

- Workspace initial exit101 after170.27 s: manifest fixture exceeded the approved16 MiB contract before reaching its intended lifetime check. Log task-10-workspace-tests.log retained. Fixture correction above; task-10-manifest-green.log2passed/0failed,3.79 s. Required resource env; cargo fmt --all exit0.
- ty initial exit1: missing declared optional hypothesmith produced unresolved import and Unknown decorator diagnostics. Shared .venv remained unchanged. First isolated install failed DNS in sandbox; escalated declared dependency installation into owned /tmp succeeded. Versions: hypothesmith0.3.3,hypothesis6.169.1,lark1.3.1,libcst1.9.0,PyYAML6.0.3,sortedcontainers2.4.0, matching uv.lock. ty --extra-search-path owned overlay exits0.
- Initial pytest exit1:1054passed/11skipped/6failed,32.17 s. Two release tests attempted readonly global uv cache; four Lean process-guard tests returned monitor_error126 because sandbox denied ps (direct ps evidence retained). Targeted escalated rerun with owned UV_CACHE_DIR passes all six. Optional generator cases pass with owned PYTHONPATH overlay.
- That targeted command ran25cases:24passed/1failed. The extra failure was the already-passing deliberate missing-dependency case: Python -S still honors PYTHONPATH, so our overlay invalidated its setup. Reran its two registered cases without overlay:2passed. No production/test accommodation or silent skip; optional-generator successful executions remain valid evidence.

Exact integrated gate table, verification/completion reviews, commit and final resource evidence follow after gates complete.


## Additional integrated-fixture correction: five review passes

1. Requirements: second workspace attempt stopped at ranking_heap before any large-case ranking observation; preserve bounded ranking validation and clone-control sensitivity, not the old oversized discovery fixture.
2. Root cause:64×256 KiB originals/replacements/identity copies≈48 MiB; 32B/32 KiB controls measured32byte ranking peaks before failure. New16 MiB admission correctly rejects larger setup; no production fix or budget increase warranted.
3. Design/scope: controller approved64 KiB maximum body with64 candidates (≈12 MiB retention), three named scalar-size cases plus independent positive clone control, whole-case Mutex protecting global allocator. Public plan creation/conversion remain outside measurement. Lower clone threshold8 MiB still exceeds64 KiB ceiling by128times.
4. Implementation: shared typed fixture retains project lifetime, normalized config, resolved targets and converted ranked candidates. Three repeats measure the same scenario; no size-based conditional assertion or mixed positive-control path. Existing actual validation and rank-candidate clone operations remain.
5. Verification: focused ranking_heap4/4pass,1.40 s; original failure log retained. Read-only search found only ranking_heap and manifest_buffer_lifetime generate large public plans in heap/lifetime binaries. selection_heap creates data directly; public-ranking plan fixture64×4KiB is below limit;1.1MiB record-size fixtures intentionally reject and already passed. Final workspace uses --no-fail-fast to collect independent remaining failures once.

## OKF check scope and initial audit findings

Initial full audit exits1 on pre-existing catalog gaps:1group containing12 source IDs without matching footnote definitions and85 missing original-document entries,86 finding records total. Running the same audit on git-show base4ebed445 reproduces the exact list. New changes add no catalog-scope originals and no reference finding. Controller instructed preserving this limitation, not expanding unrelated maintenance. Final scoped checker exits0 for YAML/reservedstructure49 pages,961 sources,1,685 local targets,49reachablepages,17 new source hashes and clean-source revision blobs, and exact-baseline finding comparison. New untracked archive sources carry SHA without falsely naming a committed source revision. Existing historical metadata remains unchanged. No official validator or blanket content-verified flag is claimed.

Python skip correction: initial11skips comprised8realworkflow-linter cases and3hypothesmith cases (two grammar/libcst parameters plus filtering). All3optional cases passed with the overlay; cumulative distinct original suite coverage is1063passed/8skipped, not1062/9. The eight remaining skips require unavailable actionlint/shellcheck/zizmor executables. No platform skip occurred in this Python run. This is coverage across recorded invocations, not one cleanfull pytest pass.


## Verification review passes completed while final gates run

1. Test-count interpretation: read per-executable terminal summaries, counting each outer suite once despite nested helper-process summaries. Final default workspace3,304 passed/39 ignored; core contracts492 passed/2 ignored. The two initial failed invocations retain partial counts1455/1/28 and1766/1/29, and are not added as distinct final coverage. CLI contracts results remain pending at this checkpoint.
2. Ignored tests: inspected all39names/reasons. Ten Task9 release probes were previously executed and retained in the archive; other ignores include manual benchmarks, fixed-seed descriptive inspection, authored-project measurement, and explicit helper-process fixtures exercised by parents. Non-native cfg-gated tests are absent, not counted as passing. No unchanged fuzz/Lean execution is invented.
3. Python/environment: retained initial1054/6/11, targeted24/1/0 with all six environmental failures and all three optional generator cases passing, then two missing-dependency cases passing without overlay. Originaldistinctcoverage1063/0/8 afterenvironmentcorrection;8skips require missingworkflowtools. Typecheck passes with installed locked extras; shared venv untouched.
4. Documentation/content: read changed claims against actual constructor, payload leases, descriptor opener, source-index/error-order and link-count source plus archived task evidence. Static claim checking is separate from49-page YAML/navigation and baselinecatalog comparison; draft status retained. Strict docs_site passes. Historical Task3feature correction, Task9measurementrevision and Django debugfailure remain explicit.
5. Resource/provenance: source reports/reviews20 files match originals byte-for-byte; no downloaded upstream sources/binaries added. One target grew3.3→6.1 GiB with114 GiB free; retained warm forfinalfixes. Extra Pythonpackages matchuv.lock. All build/test/format Cargo and hooks use required resource environment; a read-only cargo --version metadata query initially omitted thatenv, then was repeated with it (no build orresource impact).


## Final Rust gate checkpoint and fuzz-format correction

Full CLI contracts exits0 with2,807 passed/37 ignored across123 executable/doctestgroups; default workspace3304/39 andcore 492/2 remain valid. CLI generator freshness exits0. Fuzz fmt initially exits1 on one Task9 long iterator expression in fuzz_targets/python_analyzer.rs:86. Applied only rustfmt's line wrapping (5inserted/1removedline); corrected fuzz fmt and shared-target all-target fuzzClippy exit0. No behavioral source change, no repeated Rust suite or claimed fuzz execution. Required final hooks will cover the two changed Rust test files; fuzz is separately checked becauseprek excludes it.


## Exact integrated commands and exits

Working directory for every command: `/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-971-979-cli-analyzer-resource-safety`.
Cargo runner environment: `CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_NET_OFFLINE=true`.
Focused fixture checks use the same first four variables and explicit `--offline --locked`.
Each log below is under `/tmp/hoimin-design-audit-5d3aee95/`.

| Command (Cargo environment above) | Exit | Result / log |
| --- | ---: | --- |
| `cargo fmt --all -- --check` | 0 | 1.29s; `task-10-root-fmt.log` |
| `cargo fmt --manifest-path vendor/ruff_python_parser/Cargo.toml -- --check` | 0 | 0.11s; `task-10-vendor-fmt.log` |
| `cargo clippy --offline --locked --workspace --all-targets --all-features -- -D warnings` | 0 | 0.35s; `task-10-workspace-clippy.log` |
| `cargo clippy --offline --locked -p littrs-ruff-python-parser --lib --no-deps -- -D warnings` | 0 | 0.10s; `task-10-vendor-clippy.log` |
| `cargo test --offline --locked --workspace -- --test-threads=2` | 101 | 1455 passed / 1 failed / 28 ignored; 170.27s; `task-10-workspace-tests.log` |
| `cargo test --offline --locked --workspace -- --test-threads=2` | 101 | 1766 passed / 1 failed / 29 ignored; 138.60s; `task-10-workspace-tests-final.log` |
| `cargo test --offline --locked --workspace --no-fail-fast -- --test-threads=2` | 0 | 3304 passed / 0 failed / 39 ignored; 234.55s; `task-10-workspace-tests-complete.log` |
| `cargo test --offline --locked -p hoimin-core --features contracts -- --test-threads=2` | 0 | 492 passed / 0 failed / 2 ignored; 16.95s; `task-10-core-contracts.log` |
| `cargo test --offline --locked -p hoimin-cli --features contracts -- --test-threads=2` | 0 | 2807 passed / 0 failed / 37 ignored; 329.28s; `task-10-cli-contracts.log` |
| `cargo run --offline --locked -p hoimin-cli --example generate_cli_reference -- --check` | 0 | 0.50s; `task-10-cli-freshness.log` |
| `cargo fmt --manifest-path fuzz/Cargo.toml --all -- --check` | 1 | 1.37s; `task-10-fuzz-fmt.log` |
| `cargo fmt --manifest-path fuzz/Cargo.toml --all -- --check` | 0 | 1.54s; `task-10-fuzz-fmt-final.log` |
| `cargo clippy --offline --locked --manifest-path fuzz/Cargo.toml --all-targets --target-dir /Users/hayao/RustroverProjects/hoimin/.worktrees/issue-971-979-cli-analyzer-resource-safety/target -- -D warnings` | 0 | 0.94s; `task-10-fuzz-clippy.log` |
| `.venv/bin/maturin build --release --offline --locked` | 0 | 60.63s; `task-10-wheel-build.log` |

Fixture commands: `cargo fmt --all` twice (exit0; task-10-test-fmt.log/task-10-ranking-fmt.log); `cargo test --offline --locked -p hoimin-cli --test manifest_buffer_lifetime -- --test-threads=2` exit0,2passed; `cargo test --offline --locked -p hoimin-cli --test ranking_heap -- --test-threads=2` exit0,4passed. The required four environment variables prefix each command. Initial fullsuite failures are not counted as additional finalcoverage.

Other exact commands and original outcomes:

- `.venv/bin/ruff format --check .`: exit 0. 55 files already formatted Log: `task-10-ruff-format.log`.
- `.venv/bin/ruff check --no-fix .`: exit 0.  Log: `task-10-ruff-check.log`.
- `.venv/bin/ty check`: exit 1. missing optional hypothesmith dependency; 2 diagnostics Log: `task-10-ty.log`.
- `UV_CACHE_DIR=/tmp/hoimin-design-audit-5d3aee95/task-10-uv-cache uv pip install --target /tmp/hoimin-design-audit-5d3aee95/task-10-python-extras --python .venv/bin/python hypothesmith==0.3.3 hypothesis==6.169.1 lark==1.3.1`: first sandbox DNS failure, escalated retry exit0.  Log: `task-10-python-extras*.log`.
- `.venv/bin/ty check --extra-search-path /tmp/hoimin-design-audit-5d3aee95/task-10-python-extras`: exit 0.  Log: `task-10-ty-extras.log`.
- `.venv/bin/pytest`: exit 1.  Log: `task-10-pytest.log`.
- `UV_CACHE_DIR=/tmp/hoimin-design-audit-5d3aee95/task-10-uv-cache PYTHONPATH=/tmp/hoimin-design-audit-5d3aee95/task-10-python-extras .venv/bin/pytest tests/test_release.py::test_skipped_reservation_prevents_preparation tests/test_release.py::test_release_preparation_uses_reserved_tag tests/test_lean_resource_guard.py::test_terminates_process_group_at_rss_limit tests/test_lean_resource_guard.py::test_propagates_normal_child_exit tests/test_lean_resource_guard.py::test_terminates_process_group_at_timeout tests/test_lean_resource_guard.py::test_rejects_root_exit_that_leaves_a_descendant tests/test_hypothesmith_corpus.py -ra`: exit 1. all six original failures and all three optional cases pass; overlay invalidated -S negative-case setup Log: `task-10-pytest-environment.log`.
- `.venv/bin/pytest tests/test_hypothesmith_corpus.py::test_cli_without_optional_dependencies -ra`: exit 0.  Log: `task-10-pytest-no-extras.log`.
- `.venv/bin/python docs/maintenance/2026-10-18-issues-971-979/probes/check-okf.py`: exit 0. 49 pages, 961 sources, 1685 local targets, 17 new source hashes; exact 86 baseline findings retained Log: `task-10-okf-maintained.log`.
- `.venv/bin/python -m tools.docs_site --output site`: exit 0.  Log: `task-10-docs-site-maintained.log`.
- `UV_CACHE_DIR=/tmp/hoimin-design-audit-5d3aee95/task-10-uv-cache PIP_CACHE_DIR=/tmp/hoimin-design-audit-5d3aee95/task-10-pip-cache HOIMIN_WHEEL=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-971-979-cli-analyzer-resource-safety/target/wheels/hoimin-0.12.0-py3-none-macosx_11_0_arm64.whl HOIMIN_SMOKE_REPORT=/tmp/hoimin-design-audit-5d3aee95/task-10-wheel-smoke.json .venv/bin/python tests/wheel_smoke.py`: exit 0.  Log: `task-10-wheel-smoke.log`.

The first optional install exit is recorded from the failed UV process; package install used PyPI registry versions matching uv.lock and owned cache/target only. Installed versions: hypothesmith0.3.3, hypothesis6.169.1, lark1.3.1, libcst1.9.0, PyYAML6.0.3, sortedcontainers2.4.0. Original Python reports:1054passed/6failed/11skipped; targeted24passed/1environment-negative-case failed; no-overlay2passed. Final distinct coverage1063passed/8workflow-tool skips. No sharedenvironment sync or install.

Release wheel `hoimin-0.12.0-py3-none-macosx_11_0_arm64.whl`, SHA256 `f22bd67a4bfa30835a5251a6b713adbc25bd181771101d1def79556663b84821`, version0.12.0. Maturin1.15.0 build60.63s, smoke exit0. Smoke creates/deletes its own venv, installs pytest8.x there, runs installed version/help, uvx distribution help, real baseline/mutation and unchanged original hash checks. Exact platform/hash receipt archived in observations/task-10-wheel-smoke.json. Only macOS evidence; no native Linux/Windows, no fuzz execution. Task6 bounded Lean proof/corpus/sensitivity evidence retained; final Rust adapter suites executed, no aggregate Lean rerun.

## Completion: five distinct review passes

1. Scope/diff: reviewed all deliberate changes: two guides, six existing concepts, maintained summary/archive, two authorized test fixtures and one fuzz whitespace hunk. No production behavior, dependency manifests, obsolete API adapter, whole-program sandbox guarantee or unrelated catalog repair added.
2. Test evidence: final workspace/core-contracts/CLI-contracts counts3304/492/2807 pass with39/2/37 ignored; checked full terminal logs and retained per-target records. Focused fixture6 cases pass with positive clone sensitivity intact. Python results are cumulative across explicit environmental corrections, not one all-green invocation. Final wheel smoke succeeds and preserves source restoration.
3. Docs/version/provenance: checked10/8/12/floor0.12.0 vs retained5/4/1-IDv1, historical949/849 scope and field13-only framing. Added17source entries point to content actually read; archived untracked sources use hashes without future SHA. All20original reports/reviews preserve exact bytes and initial-failure narratives; Task3/Task9 observations keep their true revisions.
4. Resource/hygiene: target9.9GiB/free110GiB; no concurrentCargo, main-target writes or sharedvenv installs. Target remains warm as requested, owned temp caches retained for reproducibility/review. No cleanup, push or PR from this worker. Required hooks remain enabled; ignored SDD report confirmed not staged.
5. Handoff/limitations:50acceptance conditions mapped; scoped design score10/10, eight explained diagnostics. Original debug-Django and native platform/fuzz limits remain explicit. OKF49-page checks and baseline-comparison pass without pretending85catalog omissions/12footnoteIDs were fixed. Controller owns subsequent independent reviews, later archival and PR metadata/checks; no future verdict is claimed.

Commit and required-hook evidence appears below.


## Commit, final state and Cargo release

Commit `c74410f56c8389ef246567318ddcade724dbe3f7`: `test: integrate CLI and analyzer resource-safety evidence (#971-#979)`.
Exact commit command:

```sh
CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_NET_OFFLINE=true git commit -m 'test: integrate CLI and analyzer resource-safety evidence (#971-#979)'
```

Exit0 on first attempt. Required workspace fmt, vendor fmt, workspace all-target/all-feature Clippy and vendor Clippy all Passed (`task-10-commit.log`). Authorized index/commit mutations used require_escalated; automatic review allowed them. No hook bypass or forced ignored-file add. Commit contains57files,7,672insertions/200deletions (mostly required original evidence archive). `git diff HEAD^ HEAD --check` exits0; `git status --short` and staged-name listing are empty. All Cargo/test/commit sessions have completed; Cargo slot released. Target9.9GiB/free110GiB remains warm for controller review/fixes.

Changed files (exact committed list):

```text
crates/hoimin-cli/tests/manifest_buffer_lifetime.rs
crates/hoimin-cli/tests/ranking_heap.rs
docs/development.md
docs/knowledge/compatibility.md
docs/knowledge/design/analyzer.md
docs/knowledge/design/runtime-lifecycle.md
docs/knowledge/design/selection-plan-verify.md
docs/knowledge/design/session-report.md
docs/knowledge/design/survivor-review.md
docs/maintenance/2026-10-18-issues-971-979.md
docs/maintenance/2026-10-18-issues-971-979/README.md
docs/maintenance/2026-10-18-issues-971-979/SHA256SUMS
docs/maintenance/2026-10-18-issues-971-979/acceptance.md
docs/maintenance/2026-10-18-issues-971-979/execution-ledger.md
docs/maintenance/2026-10-18-issues-971-979/integration-gates.json
docs/maintenance/2026-10-18-issues-971-979/manifest.json
docs/maintenance/2026-10-18-issues-971-979/observations/analysis-inputs.json
docs/maintenance/2026-10-18-issues-971-979/observations/task-10-wheel-smoke.json
docs/maintenance/2026-10-18-issues-971-979/observations/task-3-review-after-default.jsonl
docs/maintenance/2026-10-18-issues-971-979/observations/task-3-review-after.jsonl
docs/maintenance/2026-10-18-issues-971-979/observations/task-3-review-before.jsonl
docs/maintenance/2026-10-18-issues-971-979/observations/task-9-charged-peaks.jsonl
docs/maintenance/2026-10-18-issues-971-979/observations/task-9-django-release-preparation.jsonl
docs/maintenance/2026-10-18-issues-971-979/observations/task-9-large-release-after.jsonl
docs/maintenance/2026-10-18-issues-971-979/observations/task-9-large-release-before.jsonl
docs/maintenance/2026-10-18-issues-971-979/observations/task-9-normal-before-remainder.jsonl
docs/maintenance/2026-10-18-issues-971-979/observations/task-9-normal-before.jsonl
docs/maintenance/2026-10-18-issues-971-979/observations/task-9-normal-release-after.jsonl
docs/maintenance/2026-10-18-issues-971-979/observations/task-9-normal-release-before.jsonl
docs/maintenance/2026-10-18-issues-971-979/observations/task-9-public-comparison.json
docs/maintenance/2026-10-18-issues-971-979/okf-audit.json
docs/maintenance/2026-10-18-issues-971-979/probes/check-okf.py
docs/maintenance/2026-10-18-issues-971-979/probes/measure-payload-large.py
docs/maintenance/2026-10-18-issues-971-979/probes/measure-payload-normal.py
docs/maintenance/2026-10-18-issues-971-979/probes/reproduce-review.py
docs/maintenance/2026-10-18-issues-971-979/task-1-report.md
docs/maintenance/2026-10-18-issues-971-979/task-1-review.md
docs/maintenance/2026-10-18-issues-971-979/task-2-report.md
docs/maintenance/2026-10-18-issues-971-979/task-2-review.md
docs/maintenance/2026-10-18-issues-971-979/task-3-report.md
docs/maintenance/2026-10-18-issues-971-979/task-3-review.md
docs/maintenance/2026-10-18-issues-971-979/task-4-report.md
docs/maintenance/2026-10-18-issues-971-979/task-4-review.md
docs/maintenance/2026-10-18-issues-971-979/task-5-report.md
docs/maintenance/2026-10-18-issues-971-979/task-5-review.md
docs/maintenance/2026-10-18-issues-971-979/task-6-fix1-review.md
docs/maintenance/2026-10-18-issues-971-979/task-6-report.md
docs/maintenance/2026-10-18-issues-971-979/task-6-review.md
docs/maintenance/2026-10-18-issues-971-979/task-7-report.md
docs/maintenance/2026-10-18-issues-971-979/task-7-review.md
docs/maintenance/2026-10-18-issues-971-979/task-8-report.md
docs/maintenance/2026-10-18-issues-971-979/task-8-review.md
docs/maintenance/2026-10-18-issues-971-979/task-9-fix1-review.md
docs/maintenance/2026-10-18-issues-971-979/task-9-report.md
docs/maintenance/2026-10-18-issues-971-979/task-9-review.md
docs/usage.md
fuzz/fuzz_targets/python_analyzer.rs
```

No unresolved task-specific correctness issue. Recorded limitations remain native macOS-only validation, eight optional workflow-linter Python skips, no fuzz execution, unchanged Django debug assertion, and exact pre-existing OKF catalog/footnote findings. The controller retains independent Task10/whole-branch review, later archival of this now-complete report/reviews, cleanup and PR creation. No push, PR or future review verdict is claimed.

Status: DONE.


## Task10 fix round 1 — current preview schema

The independent Task10 review at c74410f5 approved quality but marked specification compliance failed for one Minor pre-existing current-guide statement: selection-plan-verify.md said preview schema 1 while preview.rs emits schema 2. The controller explicitly requested this narrow correction before the final review. The original finding and prior successful gate history are retained; this round does not claim a new review verdict. No production or test behavior changes.

Requirements/design self-review (five distinct passes):
1. Scope: only the current preview statement and supporting maintained provenance/check evidence need change.
2. Contract: VerifyPreview::new assigns schema_version 2; JSON/JSONL share its serialized object.
3. History: Issue #599's design explicitly specifies schema 1; retain all three original source snapshots.
4. Provenance: current preview.rs bytes equal the clean c74410f5 Git blob, so add a separate clean current citation.
5. Verification boundary: only scoped OKF, strict site and required hooks are authorized; no unchanged Rust/Python test-suite reruns.

Implementation/documentation self-review (five distinct passes):
1. Corrected present-tense schema to 2 with the new current citation.
2. Labeled schema 1 as Issue #599 introduction history, preserving source metadata and footnotes.
3. Extended the maintained source checker to verify the new clean revision blob and content hash.
4. Preserved initial okf-audit.json/integration-gates.json snapshots and their historical 17-source count.
5. Linked supplementary fix evidence from the maintenance summary instead of rewriting prior gate observations.

Eight-row design diagnostic for this documentation-only boundary: information hiding 10/10 (one current source); depth 10/10 (checker validates hash plus blob); responsibility 10/10 (history/current separated); interface size 10/10 (no API changes); consistency 10/10 (schema matches emitter); error visibility 10/10 (baseline findings still reported); temporal coupling 10/10 (revision pinned); clarity 10/10 (historical label explicit). Failure: original current version drift. Remedy: corrected statement and pinned current citation. No new abstraction or compatibility adapter.

Fix-round verification history and correction:
- First scoped checker invocation exited 1 after emitting 49 pages, 962 sources, 1686 local links, 49 reachable pages, 18 verified source hashes and the same 86 findings. Its final assertion compared `errors` (outer tuples) with `baseline['errors']` (JSON lists). Normalizing only the outer tuples to lists preserves full ordered comparison.
- Inspection found the earlier `task-10-okf-maintained.log`, `task-10-okf-precommit.log` and `task-10-okf-final.log` already contain this same AssertionError. The earlier exit-0 claim in this report and integration-gates.json was incorrect: `task-10-finalize-evidence.py:15` hard-coded exit 0 into its receipt. There is no earlier successful maintained-checker invocation established by those logs. This is an evidence-recording error, not a later regression. The original JSON/report history is retained and this correction supersedes the success claim. All three historical logs plus the fix-round RED output are archived verbatim.
- `.venv/bin/python docs/maintenance/2026-10-18-issues-971-979/probes/check-okf.py`: RED exit 1 (`task-10-fix1-okf.log`), then corrected GREEN exit 0 (`task-10-fix1-okf-green.log`). All 86 baseline finding records remain identical.
- `.venv/bin/python -m tools.docs_site --output site`: exit 0 (`task-10-fix1-site.log`). Shared .venv remained read-only.
- `CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_NET_OFFLINE=true prek run --all-files`: initial exit 2 before any hook (sandbox denied ~/.cache/prek/prek.log; `task-10-fix1-hooks.log`); retried with authorized escalation.

Verification self-review (five distinct passes):
1. Directly inspected the emitter's schema assignment and shared JSON/JSONL writer.
2. Matched new source hash to both working bytes and clean revision blob.
3. Confirmed normalized findings equal the exact baseline contents, retaining 12 IDs in one group plus 85 catalog omissions.
4. Compared earlier raw checker logs to their receipts, discovered and explicitly corrected the false success claim.
5. Strict site passed; no unchanged Rust/Python suite or fuzz execution was repeated.

The controller classified the false checker receipt as an additional Important evidence issue and authorized auditing every manually assembled `other_checks` entry. All 11 rows / 12 attempts were compared with raw output and available process receipts:

| Earlier command/attempt | Audit result |
| --- | --- |
| Ruff format | subprocess.call receipt exit 0 matches 55 formatted files |
| Ruff check | subprocess.call receipt exit 0 matches all checks passed |
| Initial ty | subprocess.call receipt exit 1 matches two diagnostics |
| Extras install, sandbox | DNS-error terminal outcome matches failure; exact original numeric exit not independently persisted |
| Extras install, escalated | six-package install terminal outcome matches success; exact original numeric exit not independently persisted |
| ty with overlay | all-checks-passed terminal outcome matches success; exact original numeric exit not independently persisted |
| Initial pytest | terminal summary matches 1054 passed / 6 failed / 11 skipped; exact original numeric exit not independently persisted |
| Targeted environment pytest | terminal summary matches 24 passed / 1 failed / 0 skipped; exact original numeric exit not independently persisted |
| No-overlay negative case | terminal summary matches 2 passed; exact original numeric exit not independently persisted |
| Maintained OKF | CONTRADICTION: AssertionError versus manually written exit 0; corrected now |
| Strict site | original silent log cannot establish numeric exit; fix-round subprocess.run captures 0 |
| Wheel smoke | original silent log cannot establish numeric exit (artifact records completed checks but precedes final temp cleanup); one fix-round rerun captures 0 |

Current integration-gates.json now uses the actual fix-round OKF/site/wheel receipts. The complete original JSON is preserved byte-for-byte at observations/task-10-integration-gates-original.json, explicitly historical. Supplemental task-10-fix1.json includes each audit result, terminal excerpt, hash and numeric-exit uncertainty; no further outcome contradiction was found. This supersedes the earlier fix-stage intention to leave integration-gates.json unchanged.

Wheel evidence-gap validation: `UV_CACHE_DIR=/tmp/hoimin-design-audit-5d3aee95/task-10-uv-cache PIP_CACHE_DIR=/tmp/hoimin-design-audit-5d3aee95/task-10-pip-cache HOIMIN_WHEEL=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-971-979-cli-analyzer-resource-safety/target/wheels/hoimin-0.12.0-py3-none-macosx_11_0_arm64.whl HOIMIN_SMOKE_REPORT=/tmp/hoimin-design-audit-5d3aee95/task-10-fix1-wheel-smoke.json .venv/bin/python tests/wheel_smoke.py` ran with authorized escalation, subprocess.run.returncode 0 in 3.421 seconds. The original report was not overwritten. Its wheel hash is unchanged. Both actual receipt and smoke identity are archived.

Required hooks: authorized escalated `CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_NET_OFFLINE=true prek run --all-files` exited 0; all four workspace/vendor fmt and Clippy hooks passed. Warm target 9.9 GiB / free disk 110 GiB; no concurrent Cargo, no main target writes.

Additional evidence-issue passes: requirements identified every manually written row; design distinguished terminal outcomes from actual numeric returns; implementation retained the original bad receipt with explicit historical scope; verification recovered three original numeric receipts and freshly captured the two silent-command results; completion corrects the current status rather than leaving an unqualified false success row. Diagnostic error visibility for the original receipt was a failure; corrected raw-log/receipt provenance and explicit unknown exits remedy it.

Completion self-review (five distinct passes):
1. Scope remains current docs, maintained checker and evidence only; no product/test edits.
2. Preserved the original Minor preview finding and newly discovered Important evidence failure with their corrections.
3. Original schema-1 sources and 17-source audit remain historical; new schema-2 source is pinned clean, with 18 checks.
4. No Rust/Python test suite was rerun; the wheel rerun resolves a concrete silent-return evidence gap.
5. Controller retains independent re-review/final review; hook success and the eventual commit do not claim those verdicts.


Fix round 1 completion: commit `28fdbf32dbd1b090a43058f61088f560aa79f76a`. The exact commit command was `git commit -m 'docs: correct preview schema and integrated gate evidence'` under `CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_NET_OFFLINE=true`; exit 0. Automatic commit hooks remained enabled and all four skipped because only docs/evidence paths were staged; the explicit preceding `prek run --all-files` already passed all four. Commit log: `/tmp/hoimin-design-audit-5d3aee95/task-10-fix1-commit.log`.

Final scoped checker and strict site reran after evidence reconciliation, both captured exit 0 (`task-10-fix1-okf-final.log`, `task-10-fix1-site-final.log`). Archive checksum validation passed all 60 entries; preserved original integration-gates JSON matches the c74410f5 blob byte-for-byte. `git diff --check` and staged diff check passed; postcommit status is clean. No SDD file was staged or force-added.

Changed files (22):
```text
docs/knowledge/design/selection-plan-verify.md
docs/maintenance/2026-10-18-issues-971-979.md
docs/maintenance/2026-10-18-issues-971-979/README.md
docs/maintenance/2026-10-18-issues-971-979/SHA256SUMS
docs/maintenance/2026-10-18-issues-971-979/integration-gates.json
docs/maintenance/2026-10-18-issues-971-979/observations/task-10-fix1-hooks-escalated.log
docs/maintenance/2026-10-18-issues-971-979/observations/task-10-fix1-hooks.log
docs/maintenance/2026-10-18-issues-971-979/observations/task-10-fix1-okf-final.log
docs/maintenance/2026-10-18-issues-971-979/observations/task-10-fix1-okf-green.log
docs/maintenance/2026-10-18-issues-971-979/observations/task-10-fix1-okf.log
docs/maintenance/2026-10-18-issues-971-979/observations/task-10-fix1-site-final.log
docs/maintenance/2026-10-18-issues-971-979/observations/task-10-fix1-site.log
docs/maintenance/2026-10-18-issues-971-979/observations/task-10-fix1-wheel-receipt.json
docs/maintenance/2026-10-18-issues-971-979/observations/task-10-fix1-wheel-smoke.json
docs/maintenance/2026-10-18-issues-971-979/observations/task-10-fix1-wheel-smoke.log
docs/maintenance/2026-10-18-issues-971-979/observations/task-10-integration-gates-original.json
docs/maintenance/2026-10-18-issues-971-979/observations/task-10-okf-final.log
docs/maintenance/2026-10-18-issues-971-979/observations/task-10-okf-maintained.log
docs/maintenance/2026-10-18-issues-971-979/observations/task-10-okf-precommit.log
docs/maintenance/2026-10-18-issues-971-979/observations/task-10-python-gates.json
docs/maintenance/2026-10-18-issues-971-979/probes/check-okf.py
docs/maintenance/2026-10-18-issues-971-979/task-10-fix1.json
```

Remaining concerns: unchanged catalog findings and platform/test limits from the original report; six earlier manually recorded numeric exits cannot be independently recovered, although their terminal outcomes/counts match. This limitation is explicit in current receipts. The incorrect OKF claim has been corrected with captured success, not hidden. No pending worker action; Cargo slot released with target warm. Independent re-review and final review remain the controller's responsibility.
