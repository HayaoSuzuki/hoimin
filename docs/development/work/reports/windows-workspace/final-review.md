# Windows workspace repair review

Reviewed `b3a0f62e..3514ecdb`. The affected Windows group exercised all 695 baseline invocations. It reported 687 passes and eight existing ignored fixtures after the fix.

## Strengths

- The Windows write path validates the retained parent after a missing final-component open. Inspection errors propagate before create or truncate. The existing missing-parent fault test distinguishes this from successful creation.
- Native inspect/verify injections surround metadata inspection and the final identity lookup on the secured handle. Native claim and unlink injections wrap the existing guarded operation. With injection disabled, both operations still execute their identity checks and cancellation callbacks with the previously required handles and access masks. The publication and failed-reclamation tests retain their staging rollback and payload assertions.
- Replacement fixtures use an already authorized rename handle. The janitor fixtures release their interfering handles before inspection. Cancellation retry reacquires deletion authority. The second capability transfer checks the precise Windows error and the empty authority slots, then completes cleanup using the restored original capabilities.
- The plan and report each document five distinct design/plan and implementation/test checks respectively. Parameterized invocation boundaries were retained for invalid prefixes, readiness, fault points and cancellation points.

## Issues

### Critical

None found.

### Important

`crates/hoimin-cli/src/workspace/owned.rs:9144` and its callers now read through an owning file handle. This fixes Windows lock contention, but allows a detached file to satisfy an assertion about the named marker. For example, a Unix cleanup that unlinks the lease while leaving the active root and payload would pass the last lease assertion in `owned_coverage_tests.rs:4240`. The original `root.dir.read(LEASE_FILE)` failed in this case. The same distinction affects coordinator cursor assertions. Have the shared observation compare the no-follow identity of the named entry with the identity of the owning handle and read its bytes through that handle. Include missing and replaced entry cases.

### Minor

None found.

## Software design diagnostics

| Diagnostic | Assessment at `3514ecdb` |
| --- | --- |
| Single describable responsibility | Pass. Worker writes, managed cleanup and fixture evidence each have a distinct owner. |
| Interface simpler than implementation | Pass. Guarded native operations own validation and mutation behind one call. |
| Implementation can change independently | Pass. No public signatures or persisted data changed. |
| Interface comments describe the abstraction | Pass. The owning-handle helper states its lock and file-position guarantees. |
| Design discussed during review | Pass. The plan and review separate inspection, pinning and deletion authority. |
| Important decisions encapsulated | Fix required. The test observation must own the association between an entry name and its retained handle for callers asserting named bytes. |
| Boundaries understandable without inspecting implementations | Pass. Existing names and contracts distinguish inspection handles from deletion-capable handles. |
| Measured design investment of 10–20% | Unverified. A contemporaneous time breakdown would be needed to score this row. |

Six supported rows give **7.5/10**. Restoring the named-entry observation would bring the evidence-supported score to **8.75/10**.

## Declined to judge

- Native Unix runtime results. This review has Windows receipts and inspected source for shared fixture changes.
- The final all-features workspace run. The coordinator owns its execution and completion gate.

## Assessment

**Ready with fixes.** The production fix preserves the native authority and cancellation paths. Restore the named-entry test oracle before signing off on the final revision.
