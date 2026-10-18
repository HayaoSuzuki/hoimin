# Local review checkpoint — issues #992–#998

Production and integration review: `8ebcea20..c887b843`. Final prose correction: `c887b843..9f1c97da`. No Critical or Important finding remains; the sole final documentation finding is addressed. This checkpoint precedes PR creation/native CI. Later hosted outcomes belong to the PR validation record. No merge or release is authorized by this work.

Each task has its stage self-review and executed checks in the adjacent task record. The reports below preserve independent review findings before temporary coordination files are removed.

## Integration decisions

Ruling: Use one internal hoimin-python crate before hierarchy optimization — existing fuzz consumers need pure API without CLI I/O dependencies — if this boundary proves too broad, rework the extraction before its task review.

Ruling: Share one task-owned Cargo target and run implementations sequentially — avoids duplicate builds and shared-file conflicts — cost is less parallel throughput.

Ruling: Include a minimal source-distribution packaging correction in Task8 if unpacked validation confirms the missing patched-parser source — all production consumers must use the same patched parser and the requested distribution validation must be meaningful — cost if wrong is unnecessary packaging scope; keep changes limited to the missing build inputs and regression evidence.

Ruling: Leave the pre-existing core/CLI missing-description warnings as documented optional metadata work — both omissions predate this branch and actual wheel/sdist build and smoke checks pass — cost is continued nonfunctional packaging warning noise.


---

Spec compliant; quality Approved. Reviewer /root/review_992_998_task1.

No Critical/Important findings. Private API boundary, charged output ownership, test-only counters, CLI/fuzz Cargo consumers, release version updates and registration preservation verified from diff and evidence. No tests rerun.

Minor: crates/hoimin-python/Cargo.toml lacks description (sdist warning); final fuzz should configure working symbolizer. Integration Task 8 owns these and current C4/dependency docs.

Evidence: python_api.rs cross-crate lease lifetime, payload_tests.rs cancellation cleanup, task-1-registration-summary.json distinct 342 private tests/33 payload/19 selectors; release.py and test_release.py package updates.


---

Spec compliant; quality Approved. Reviewer /root/review_992_998_task2.

No Critical/Important/Minor findings. Common identity validation after structural checks and before consumption; review duplicate removed with descriptor checks retained; borrowed projection keeps hash bytes and avoids payload copies. Negative formats/fields and positive display/status/order cases, genuine golden/Lean fixture IDs and published vectors checked.

Named outside checks: JSON/JSONL converge through validate_document/validate_structure; unchanged hashing domain/framing/canonicalization/field order. Recorded 901 CLI plus40 core passed, no warnings/errors. No tests rerun; read-only.


---

Spec compliant; quality Approved. Reviewer /root/review_992_998_task3.

No Critical/Important/Minor findings. Native names remain I/O authority, bounded lossy text diagnostic only; independent Linux regular/nested/symlink tests, Windows surrogate tests, real CLI restoration/mutation/original/cleanup assertions. Actual native platform execution pending final CI.

Focused omitted-context checks: walker bodies retain native arguments and no-follow/reparse/identity guards; owned.rs truncator keeps UTF-8 boundary. No tests rerun.


---

Spec compliant; quality Approved. Reviewer /root/review_992_998_task4.

No Critical/Important/Minor findings. Immutable limits reach both preflight paths/streaming, core owns final reservation, copy equality distinct from owned-disk >=. Counter tests prove static zero writes, streaming growth before writes and rollback; CLI marker and shell forwarding cover distinct boundaries. All20 review passes present.

Outside-diff named risk: manifest.rs path-keyed metadata inventory deduplicates with checked addition consistently with snapshot. Final logs confirm focused suites/hooks, no warnings/errors. No test reruns. Whole-branch cancellation/ownership/identity validation remains Task8 integration.


---

Spec compliant; quality Approved. Reviewer /root/review_992_998_task5.

No Critical/Important/Minor findings. Loaded exact+dotted-prefix range retains authoritative path filtering, lexical order, offsets,65,536 ceiling. Empty-module cancellation covered; counters test-only, parameterized import/size scenario with literal bounds; descendants positively covered.

Checked cut-off full build_visible and independently compared saved8 before/after complete candidate/diagnostic/truncation rows. Existing logs show pure366, CLI22+5+2, exactgate1 pass; no reruns or warnings/errors.


---

Spec compliant; quality Approved. Reviewer /root/review_992_998_task6.

No Critical/Important/Minor findings. One private selection owner validates mode/count/raw Sample IDs+order, allows empty Explicit/Top, owns matched/Collecting/Ready/provenance. RunState initializes once; budget/report/scheduling sameowner. VerifiedPlan protection retained, no split shellmode inference/latesetter.

Named checks: pending reads retired before stop Collecting→Ready and retired completions rejected before Ordered unreachable branch; immutable sample constructors enforce nonempty/unique/population bound; all in-tree removedAPIs migrated and core exports newtypes. Existing core/CLI/docs/lint evidence checked, no reruns. Integration Task8 remains.


---

Spec compliant; quality Approved. Reviewer /root/review_992_998_task7.

No Critical/Important/Minor findings. One local borrowed rank map, stable ordering/variant handling/sequencefallback unchanged. Actual transition workcounts2k/4k separated initial/repeated, independent literal lifecycleorder/NotRun/retiredACK/summaryonce, unorderedties. Measurement limits and20selfreviewpasses clear.

Named duplicate-rank risk resolved by checked ordered constructor dedup before rankstorage. Existing Collecting/missing/synthetic coverage supported by unchangedhandling and110machine tests; broaderintegrationTask8. No reruns.


---

# Task8 scoped review

Reviewer /root/review_992_998_task8; range50b9c089..c887b843.
Spec compliant and quality Approved; no Critical/Important findings.

Checked three-member SBOM cleanup and matching versions; real archive independent source-byte/exclusion tests; all14 added source hashes and exact24 vendor archive inputs; ordinary full-pytest CI toolchain dependency. Retained logs substantiate artifacts/mutations/wheels/heap fixes. No tests rerun.

Minor: docs/maintenance/2026-10-19-issues-992-998/task-8.md:184 and task8-sdist-green.log:4,:8 show existing missing-description warnings for core/CLI manifests. Reviewer recommends tracking separately; final reviewer to triage.

Cannot verify from diff: native Linux/Windows execution and controller finalreview/PR/remote actions. These remain explicit controller gates. Rust evidence remains2879pass/2failed/33ignored exit101 +2 focused repaired passes, not an exit-zero aggregate. Controller confirmed production unchanged after full collection and read independently reviewed focused evidence.


---

# Task 2 integration fixture review

Reviewer: /root/review_992_integration_fixture
Range: 50b9c089..f5f1a1f4
Spec compliance: Approved. Task quality: Approved. No Critical, Important or Minor findings.

Verified genuine IDs only for public ingestion, unchanged production validation and independent literal expected vectors. Named-risk checks confirmed MutantKey excludes file_hash, role/content mapping preserves eligible sets, role-0 sorts before role-1 for detail limits, and comparison-only injection keeps its synthetic builder. Saved logs show5 focused tests and fmt/clippy hooks passed; no suite rerun by reviewer.


---

# Task 4 integration review

Reviewer /root/review_994_metrics_integration; range f5f1a1f4..7ea07bbf. Spec compliant, quality Approved. No Critical, Important or Minor findings.

Checked lifecycle: inventory failure precedes snapshot, preflight maps CopyLimit to EffectFailed, controlled failure reports/cleans normally; final core budget reservation error remains outer Rust Err and metrics.incomplete. Failed-run test checks exit2, incomplete report, no baseline, valid zero-work metrics and same run ID. Destination authorization precedes inventory/session and collision yields no Ready capability. Source/session/owned-lock cases preserve bytes/cleanup; actual transition-error and shutdown-expiry incomplete tests remain. Existing 29+29 passing logs inspected; no test rerun.


---

# Task 2 heap fixture review

Reviewer /root/review_992_heap_integration; range7ea07bbf..60dab172. Spec compliant, quality Approved. No Critical, Important or Minor findings.

Verified IDs assigned after fields are finalized; production unchanged. Named-risk inspection of helper and omitted measurement boundaries confirmed payload clones happen only in setup, history setup drops before tracking, and body/history dimensions, semantic assertions, independent memory ceilings and eager control unchanged. Saved focused tests/fmt/hooks passed with no warnings. No tests rerun or shared files modified.


---

# Final whole-branch review — issues #992–#998

Reviewer: `/root/final_review_992_998`

Base: `8ebcea20d2e3a46301bd4d90b13e59c9b5361b8b`  
Head: `c887b843`

Read-only review against the accepted design, implementation plan and original seven issue bodies. Used the supplied aggregate diff in bounded slices, inspected surrounding implementation where needed, and checked recorded validation evidence. No source, index, HEAD or branch mutations; this report is the only new file. No nested agents or test reruns.

### Strengths

- **Identity has one implementation and one report boundary.** `CandidateIdentityRef` borrows identity-bearing payloads and delegates to the existing hash encoder. Both JSON and JSONL reach common validation before progress accepts usable/unusable reports. Review retains its independent descriptor/source checks. The production change does not add display-field equality or workspace access to progress.
- **Admission and reservation remain distinct responsibilities.** Preflight inventory and each snapshot chunk use immutable configured limits and shared checked copy arithmetic. Copy equality and owned-workspace equality retain their different policies. The ledger alone issues the final grant; partial snapshot ownership still rolls back on rejection. Admission tests cover zero content I/O, worker multiplication, duplicate includes, source growth, overflow and the admitted two-content-pass control.
- **The analyzer extraction establishes a useful boundary.** CLI, dedicated corpus/heap tests and fuzz consume the same Cargo API. Payload leases move through the existing charged values; CLI retains source acquisition, fingerprinting and spooling. Development observations are feature-gated, and both workspace roots retain their parser patch. The loaded-name exact/prefix search preserves exact-before-descendant ordering, alias offsets, cancellation and visibility limits.
- **Selection owns the invariant rather than merely grouping fields.** Constructors validate mode, deduplicated count and raw Sample ID/order agreement. Shell passes the checked value directly from `VerifiedPlan`. Budget gating, membership, missing-ID handling and output provenance share that owner, while Collecting/Ready still owns candidate movement. Stop rank preparation borrows one local map from the owned request order and retains stable sequence fallback. Pending effects are retired before stopped collection changes phase.
- **Native names remain I/O authority.** Unix stat/open operations and Windows child opens continue using the original native names. Lossy bounded text is confined to diagnostics; native files are counted and symlinks remain excluded from traversal.
- **Distribution checks exercise artifacts.** The parser-source correction is confined to explicit sdist inputs, and real archive tests compare source bytes independently of the include list. SBOM cleanup/version checks include the new crate without weakening parser provenance or graph validation.

### Issues

#### Critical (Must Fix)

None found.

#### Important (Should Fix)

None found.

#### Minor (Nice to Have)

1. **Release documentation overstates the files rewritten by CI.**
   - **File:** `docs/releases.md:72`.
   - **Trigger:** A maintainer follows the updated statement that CI embeds the reserved version into “those five files,” which includes the preceding `fuzz/Cargo.lock` reference.
   - **Observed implementation:** `tools/release.py:187` still rewrites four distinct files: root `Cargo.toml`, `pyproject.toml`, root `Cargo.lock`, and `uv.lock`. This branch adds a package-entry edit to root `Cargo.lock`; it does not add a fifth file. The release workflow invokes that function and does not separately rewrite the fuzz lock.
   - **Impact:** The guide incorrectly implies that release-build checkout versioning synchronizes the separate fuzz lock automatically. Shipping versioning itself is unchanged and this is not a release blocker.
   - **Correction:** State the four release-build files explicitly, preserving the separate instruction to update both internal entries in `fuzz/Cargo.lock` when raising the repository minimum. No production expansion is required.

### Architecture assessment

Using the software-design-philosophy diagnostic, the scoped ownership design scores **10/10 (8/8 rows satisfied)**: the changed modules have describable responsibilities; their interfaces hide meaningful implementation work; callers no longer depend on analyzer private namespaces or assemble selection provenance independently; interface comments explain ownership and invariants; this review checks design explicitly; important decisions have identifiable owners; the new boundaries are understandable from their API documentation; and the work makes the requested strategic ownership improvements. No failed diagnostic row or architectural correction is required to reach 10/10. This score concerns the changed ownership boundaries, not a claim that every repository module or document is perfect; the minor release-documentation error is recorded above.

### Validation evidence and limits

The aggregate summary records 2879 passed, 2 failed, 33 ignored and exit 101. I checked the two later heap-target pass summaries and the intervening changed-file history: the repairs are fixture-only, and no production Rust code changed after that aggregate. This supports aggregate-plus-focused evidence, not an exit-zero full-suite claim. The reported Python, performance, strict-boundary, lock/license/purity, artifact and fuzz results are consistent with the task-8 record; I inspected the seven-target fuzz summary (38 fixed inputs plus 896 bounded runs, no sanitizer errors/warnings) and successful extracted-sdist build evidence. I did not rerun broad tests to manufacture another evidence set.

### Recommendations

- Correct the one release-documentation sentence in the combined fix wave; a scoped source/document comparison is sufficient validation.
- Complete the already-planned native Linux invalid-byte and Windows accepted-surrogate CI evidence before merge. Inspect the explicit Windows output so fixture rejection is recorded as SKIP rather than claimed as native acceptance. Continue to label host-only and existing report-only gaps separately.

### Declined to judge

- **Native Linux/Windows execution success:** the code and tests were reviewed, but this macOS review cannot establish the required native filesystem behavior. This remains an in-scope controller CI gate, not an inferred pass or a newly requested product change.
- **Existing core/CLI missing-description warnings:** considered the deferred task-8 minor and did not promote it to a branch finding. Both omissions predate this branch; the new crate supplies a description, and the inspected archive/build evidence shows no functional consequence. Optional metadata cleanup can remain separate.
- **Existing hard-enforcement/PID-fault and preparation-cancellation evidence gaps:** these are explicitly reported pre-existing boundary gaps, not new guarantees supplied by these seven changes. I did not certify those environments or expand this review into their implementation audit.
- **Merge, release publication and remote CI completion:** not performed or certified by this read-only review; the controller owns the authorized PR/CI steps, and merge/release is outside the requested work.

### Assessment

**Ready to merge? With fixes.**

The production changes align with the seven accepted requirements, and I found no Critical or Important defect. Correct the minor documentation statement and satisfy the existing native CI gate; no production correction is requested by this review.


---

- **Release documentation overstates the files rewritten by CI — ADDRESSED.** `docs/releases.md:72` now explicitly names root `Cargo.toml`, `pyproject.toml`, root `Cargo.lock` and `uv.lock`. These match the four distinct output paths in `tools/release.py:190`; `.github/workflows/release.yml:157` invokes that function through `set-version`. The separate fuzz-lock minimum-version maintenance instruction remains at `docs/releases.md:68`.
- **New breakage in the fix diff:** None. The supplied `review-c887b843..9f1c97da.diff` changes only the corrected release-guide sentence and a maintenance evidence record; it introduces no code, runtime, manifest or workflow changes.
- **Out-of-scope observations:** None.
- **Checks:** Read the supplied fix diff once, compared the corrected prose with `tools/release.py:187` and the workflow call sites, and confirmed the separate fuzz-lock instruction. The fix report records a passing `git diff --check`; no tests were needed or run for this prose-only fix.
- **Fix round:** All findings addressed, no new Critical/Important breakage. Scoped quality verdict: approved.


---

# PR preparation review — issues #992–#998

1. Scope: re-read all seven stored live issue bodies; each implementation task maps to accepted conditions and no compatibility adapter, schema bump, new CLI option or unrelated language architecture was added. Source-distribution correction is limited to build inputs required by the existing patched-parser contract; its scope ruling is in the SDD ledger.
2. Tests/evidence: read Task8 report and actual aggregate summary; retain original2879pass/2fail/33ignored exit101, combine with independently reviewed fixture-only focused2pass. No runtime Rust changes since aggregate. Full Python1069pass/8tool skips followed by8real-tool passes; final focused211includes27artifact cases. Gate/performance/policy/wheel/fuzz evidence and ignored/native limits explicitly described in draft. Independent task8/final reviews pending.
3. Safety/resources: one task worktree, one active implementation and sequential Cargo jobs capped2; shared task target. No user roots/cache cleanup performed. SBOM mutations exact temporary dirs cleaned; source-preservation/safe-destination/copygrant owners retained. Cleanup after final evidence will target only owned build artifacts, keeping PR worktree and concise logs.
4. Documentation/compatibility: inspected report and updated-path inventory. Current guides/6OKF routes updated;49pages/14newsource checks, historical revisions preserved. Valid writer schema8/IDv1/SQLite unchanged; invalid arbitrary IDs rejected; floor0.13.0 retained. Native evidence cannot be inferred from APFS. Draft PR follows all template sections and lists exact7 closing refs.
5. Remote/commit readiness: branch includes issue topics and no codex prefix; main still8ebcea20 when rechecked; no existing branch PR; current clean HEADc887b843. Final review/archive commit and actual push/PR head verification remain required before reporting completion.
