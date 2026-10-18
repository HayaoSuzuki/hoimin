# Final whole-branch review — Issues #971–#979

Reviewed base `5d3aee95ca330e5fe3d3a6ccc5c5b1c633492531` through head `28fdbf32dbd1b090a43058f61088f560aa79f76a` against the approved nine issue bodies, `docs/superpowers/plans/2026-10-18-issues-971-979.md`, final-review guidance, and the code-reviewer template.

## Spec compliance and findings

The composed implementation meets the reviewed requirements. No open Critical, Important, or Minor finding was identified. The prior subscript continuation finding, current preview-schema correction, and incorrect OKF success receipt have concrete fixes and retain their original failure evidence. No compatibility adapter, silent partial-success path, or new runtime discovery-timeout authority was found.

- **Critical:** none.
- **Important:** none.
- **Minor:** none requiring a change.

The remaining controller archive, fresh checker/site receipts, cleanup, and PR work are subsequent delivery steps, not missing product implementation. This review does not claim those steps have happened.

## Five final review passes

### 1. Requirements and cross-task composition

Compared all nine approved issue bodies with the implementation and the 50-condition acceptance map. The composed CLI now has one raw mutation-options owner (`crates/hoimin-cli/src/cli.rs:150`, `:273`), while plan conversion returns execution and discovery configuration explicitly (`:433`). Task5's preserved shared syntax/native argv contract remains compatible with Task6's intentional removal of the run-only-invalid discovery flag: the latter is documented as an explicit versioned change, rather than accidentally preserving a second owner.

Traced discovery settings from plan parsing into required saved `DiscoveryConfig`, plan creation, and verify rediscovery; execution config and fingerprint encoding exclude them. Plan/report/fingerprint versions are 10/8/12 and package floor is 0.12.0; session storage 5, ranking 4, candidate schema 1, and mutant-ID v1 remain unchanged. Strict unknown-field rejection covers obsolete nested runtime timeout fields and the removed effect-level limit. Historical golden files are retained and current schema-8 fixtures are added. The five binary-labeled fuzz seed changes were inspected as bytes: valid-intent versions become 8 and the intentionally unsupported version becomes 999.

Checked review validation ordering across the faster source index and the new regular-file opener: header validation still precedes indexing and candidate-ID validation precedes source lookup (`crates/hoimin-cli/src/plan.rs:49`). The borrowed index is built once, without a per-candidate fallback scan or source/hash copies. Source-table permutation and ID-before-source tests exercise the public behavior. No conflict between the input-liveness change and store alias protection was found.

### 2. Type/API/state ownership and complexity

`AnalyzerHandler` owns private operators, profile and a positive candidate cap at construction (`crates/hoimin-cli/src/analyzer/mod.rs:108`, `:535`). Requests contain file intent only; store creation and remaining-count calculations read the same setting. Construction remains free of project/store I/O. Empty final files, final/truncated restart, cross-file ordering, pre-transfer failure retention, and post-transfer worker ownership are represented by actual behavioral tests. Keeping `Option<CandidateStore>` as transfer state is appropriate; a new session-state framework would add no needed invariant here.

The removed `RunState::with_fingerprint` was redundant, while the live fingerprint field is necessary: preflight assigns it (`crates/hoimin-core/src/machine.rs:1490`), resume loads with it (`:1498`), and fresh-session creation reads it (`:764`). Inspected fallback and missing-fingerprint tests; no old constructor value can override preflight and no valid enabled-session path loses the value.

Payload arithmetic and release are hidden in a small budget/lease module (`crates/hoimin-cli/src/analyzer/payload.rs:14`, `:139`, `:174`). Allocation knowledge remains beside its owner: validation scratch in core (`crates/hoimin-core/src/candidate.rs:452`) and encoded-record sizing/reservation in the store (`crates/hoimin-cli/src/analyzer/store.rs:131`). Non-Clone leases, value-before-lease field order and the terminal-only plain projection make the ownership rule inspectable. Existing large analyzer modules remain a broader maintenance cost, but this change does not need an unrelated rewrite or general deferred-edit abstraction.

### 3. Filesystem, platform and protection boundaries

Inspected mutation opening, writable handoff, retained-parent/identity checks, and Unix/Windows cleanup changes. Link-count rejection uses the opened descriptor before chmod and is repeated after writable reopening (`crates/hoimin-cli/src/workspace/root.rs:174`, `:211`, `:216`). Cleanup does not clear shared referent permissions to make deletion succeed. Existing content/hash checks remain independently necessary; matching source bytes cannot excuse a shared inode. Direct-original, sentinel, read-only, reset and alias tests target these consequences.

The common artifact opener uses Unix nonblocking open followed by descriptor regular-file validation (`crates/hoimin-cli/src/artifacts/mod.rs:44`). Bounded reads keep their byte/length checks. Regular-file input symlinks remain supported, while review-store symlinks remain rejected. FIFO tests use file-backed output, bounded waiting and rescue/reaping, avoiding a test that would itself hang indefinitely. No new claim that these controls sandbox arbitrary test programs appears in current documentation. Windows implementation was reviewed statically; native execution is explicitly limited below.

### 4. Resource, performance, cancellation and destruction

Traced one plan budget across all producers, prefix identities, merge, candidate conversion and prior converted files, and one handler-owned budget across serial calls and detached workers. Duplicate/nonwinning identity lookup borrows strings; winning eviction drops old charges before admitting replacements. Merge borrows deduplication keys. Conversion grows the retained lease, temporarily reserves conservative encoding/hash/path/ID scratch, and preserves the actual effect ID. Store serialization reserves its encoded buffer before allocation while retaining the independent 2 MiB record check. Errors unwind charges and partial spools; no successful shortened result is returned for payload exhaustion.

Checked the scratch formula against unchanged source encoding, candidate identity cloning, stable-ID creation, lowercase hash, and path normalization. Checked terminal untracking occurs only after producer/conversion work ends. The real detached-worker test retains a charged candidate while a later request sees the same budget, so await cancellation cannot falsely release work still running.

Selection-before-generation uses the existing start-line/symbol rule (`crates/hoimin-cli/src/analyzer/rust.rs:3098`) without pruning child traversal. Scalable container/keyword fragments and enum/hierarchy alternatives borrow source/index data. Decimal neighbors are lazy. Failure continuation checks include the fixed neighbor-to-mapping boundary (`:3577`); its regression observes actual mapping-helper entry/allocation, not merely accepted candidates. Exact child construction and builder-byte controls preserve selection semantics.

The complete-target discovery deadline remains one deadline with completion-time comparison. The paused-clock two-target test (`crates/hoimin-cli/src/analyzer/mod.rs:1208`) would fail if a fresh per-file timer were introduced. Returning on timeout does not destroy a running blocking worker; resources remain owned until it exits. Documentation accurately distinguishes this cooperative behavior from interrupting a blocked syscall.

Structurally compared archived before/after observations: all 27 normal pairs match source hash, candidate hash, count, truncation, final string bytes and candidate JSON bytes, and all report success with empty stderr. The aligned release headers identify `8d58c121` and `e4e3a1d4`, not final HEAD. Large cases explicitly fail at the payload boundary. Charged peaks, final strings and RSS are kept distinct; the measured generated/full wall overhead is retained rather than hidden by a universal speedup claim.

### 5. Tests, documentation, schemas and provenance

Examined all changed production/test/schema/formal/documentation hunks, including direct-inclusion analyzer harnesses, new fixtures and changed memory tests. Finite scenarios are named and properties bounded; helper assertions exercise behavior rather than reproducing implementation arithmetic without independent totals. The timeout Lean model/corpus is correctly plan-scoped; actual Rust correspondence adapters remain part of recorded verification.

The smaller manifest fixture still contains 24 candidates and exceeds 6 MiB; top-one/top-five cases retain actual input-allocation release before queued source reading, API/subprocess preview agreement and no test execution (`crates/hoimin-cli/tests/manifest_buffer_lifetime.rs:268`, `:273`). Ranking setup is outside observation; the three named sizes retain the 64 KiB ceiling, and real cloning exceeds 8 MiB. Whole-case mutexes protect global allocator tracking (`crates/hoimin-cli/tests/ranking_heap.rs:20`, `crates/hoimin-cli/tests/manifest_buffer_lifetime.rs:24`). These reductions allow the intended observations under the approved budget without weakening their positive controls.

Independently parsed existing raw per-executable summaries, excluding nested child summaries: workspace **3304 passed / 0 failed / 39 ignored**, core contracts **492 / 0 / 2**, CLI contracts **2807 / 0 / 37**, and focused corrected fixtures **2 / 0 / 0** plus **4 / 0 / 0**. Initial failures remain separate. Python logs support 1054/6/11 initially, 24/1 in the environment rerun and 2/0 without the overlay; the maintained cumulative 1063 passing identities/8 workflow-tool skips is not represented as a single clean run.

All 60 archive checksums match and all 20 archived prior reports/reviews match the current authoritative SDD originals byte-for-byte. The baseline/current OKF finding lists are identical, with 86 records and no introduced findings. Current preview schema 2 is separate from historical schema 1. The corrected checker normalizes only outer tuple representation and still compares the full ordered findings. Original failed checker logs and the original bad receipt are preserved; current evidence explicitly retracts the earlier success. Audited receipt metadata distinguishes actual returns, terminal outcomes, and unknown historical numeric returns. The six unrecoverable returns are not used here as independent proof of their exact numeric values.

## Four ledger Rulings assessed on merits

1. **Shared-inode cleanup protection — sound.** Rejecting mutation but then clearing the external referent's read-only attribute during cleanup would defeat #974. Safe unlink or an explicit cleanup error is the correct tradeoff. The cost is possible cleanup failure on Windows/filesystems that cannot unlink read-only entries without changing shared attributes; native behavior still needs platform evidence, not a permissive chmod fallback.
2. **Required `normalized_config.discovery.analyzer_timeout` — sound.** A small required positive `DiscoveryConfig` represents the plan-owned setting without a generic wrapper or runtime duplicate. Explicit parameters and strict schemas make the owner clear. The cost is deliberate schema/API migration and regeneration of unsupported old artifacts, already covered by the release notice.
3. **Mixed-invalid plan diagnostic ordering — sound within its stated boundary.** Independent discovery validation may choose a different first error when multiple unrelated options are invalid. Mandatory selectors/argv, each individual invalid-value error, run rejection, and validation before project I/O remain covered. Reconstructing incidental historical ordering would duplicate or split normalization solely for compatibility. Scripts asserting a particular first error on multiply invalid input bear the documented cost; this ruling does not excuse changing review ID-before-source precedence or successful behavior.
4. **Internal 16 MiB logical payload scope — sound.** One owner across retained populations addresses the measured multiplicative retention; checked leases, borrowing and adjacent scratch reservations fit the existing design. Normal charged peaks and aligned overhead are recorded. Earlier explicit failure, including before global selection or record validation, is a real user-visible cost and is documented. Source/parser/AST/facts, diagnostics, capacities, prior disk records, one in-flight candidate and the fixed annotation batch remain exclusions, so this is not an RSS limit. No evidence justifies adding a generic lazy-edit layer or an undocumented extra exclusion.

## Strengths

- Configuration authority is reduced structurally at CLI, plan/runtime and analyzer-session boundaries; no repeated equality checks or compatibility forwarding replace the old duplication.
- Resource accounting follows ownership through conversion, serialization, cancellation and detached work, with deterministic exact-boundary and real-allocation controls.
- Filesystem changes protect descriptor-bound mutations and cleanup while retaining intentional input/store policy differences.
- Historical failures, measurement revisions, skipped coverage and the corrected evidence defect remain visible instead of being overwritten by a clean narrative.

## Explicitly declined to judge — controller adjudication required

- **Native Windows/Linux behavior:** no native execution was performed in this review or established by local macOS runs. Windows link-count/cleanup and native argv branches were reviewed as code; their OS behavior and Windows-target compilation are not certified here.
- **Adversarial concurrent link creation or arbitrary child writes outside the workspace:** descriptor checks protect Hoimin's own observed mutation/cleanup path; they do not establish a sandbox against hostile concurrently running code. The documented non-security-boundary scope is appropriate, so I did not demand a new isolation mechanism.
- **Total analyzer RSS, parser/source/fact memory, allocation capacities, and syscall preemption:** these are explicitly outside the logical retained-payload/cooperative-timeout contract. I judged the documented exclusions and ownership implementation, not a stronger memory or hard-deadline guarantee.
- **Unexecuted fuzzing, a new aggregate Lean proof run, and ignored manual benchmark workloads:** compiled/linted fuzz targets and the recorded targeted Lean model/corpus plus Rust correspondence evidence are narrower claims. I did not infer the omitted executions from them or rerun passed suites.
- **The pinned Django debug-only duplicate-unary-not assertion:** original observations reproduce this pre-existing issue and release probes pass. I did not diagnose or fix the unrelated fact-index assertion or declare debug qualification successful.
- **Eight optional workflow-linter Python skips:** missing actionlint/shellcheck/zizmor executables leave those checks unexecuted. I verified the limitation is disclosed; I did not infer their results from other Python tests.
- **Whole-catalog OKF completeness:** 86 unchanged finding records remain (12 missing footnote IDs grouped into one record, plus 85 uncatalogued documents). I judged the changed concepts, source/provenance checks and unchanged baseline comparison, not resolution of these unrelated catalog omissions.
- **Exact six historical numeric process returns and independent replay of benchmark results:** terminal outcomes/counts and corrected receipts are available, but those original returncodes cannot be recovered. Measurements remain evidence of their recorded revisions and host, not performance measurements of final HEAD or reproducibility guarantees on other hosts.
- **Final archive/receipt regeneration, cleanup, publication and PR/CI state:** these are assigned to the controller after this review; no future result is asserted or treated as a current code defect.

## Method and readiness

Consumed the supplied full package in bounded portions; revisited only truncated output or focused supporting code needed to resolve concrete ownership/allocation risks. Repetitive JSON/log archives were parsed structurally and compared with originals. No Cargo/test/probe/checker/site execution, subagent, worktree, index or HEAD mutation occurred. Only this requested report was written.

**Verdict: Approved. Ready for the controller's final evidence and PR steps.** No code or documentation fix is requested by this whole-branch review. The explicit limits above must remain in the controller's final adjudication and handoff; this approval is not a claim that every platform, benchmark or future delivery step has been executed.
