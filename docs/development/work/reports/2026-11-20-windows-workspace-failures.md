# Windows workspace failure repair

This follow-up resolves the native Windows failures left by the research mutation work. Its base is `b3a0f62ea2aabbf226f8b1465fc58c4c4411dedf` on `codex/research-mutations`. The user explicitly included these failures after the earlier report; the earlier scope exclusion no longer applies.

## Reproduction and changes

The native `hoimin-cli --features contracts --lib workspace::` run reproduced exactly 29 failures, with 658 passes and 8 existing ignored subprocess fixtures. The separate missing-parent matrix reproduced one write failure and one passing cleanup control. An initial AppContainer sandbox run failed earlier at staging publication with invalid-handle error 6, including a previously passing control. Native host execution is used for this Windows capability suite.

| Cause | Correction | Preserved guarantee |
| --- | --- | --- |
| Missing-file write skips parent validation | Call the shared missing-parent validator before returning absence | Parent inspection failure prevents file creation and retains its diagnostic |
| Independent reads conflict with owned byte-range locks | Read fixture bytes through the lock-owning handle, saving and restoring its file position | Original lock stays held and exact bytes remain the oracle |
| Pathname rename conflicts with pinned handles | Use retained native rename authority on Windows | Original and replacement identities and primary/rollback errors stay checked |
| Fixture handles keep a mutation barrier or block deletion authority | Release the fixture's root or ordinary directory before janitor observation | The intended poisoned-coordinator or malformed-marker path is reached |
| Retry uses inspection-only directory handle | Reopen with the existing deletion-capable helper | Exact-object deletion and cancellation checkpoints remain enforced |
| A second capability transfer expects Unix empty-slot behavior | Require the Windows unavailable-authority error and empty owner slots, then restore the original authority | No fallback authority is manufactured; cleanup retry succeeds |
| Native paths lack equivalent fault hooks | Place existing hook names around native identity, rename and unlink boundaries | Injection is one-shot, unreachable in normal production, and existing rollback/retry assertions remain |
| Missing-heartbeat setup retains a pinned heartbeat | Release the owned heartbeat capability before deleting that fixture marker | Lease remains locked and unchanged |

## Implementation self-review

1. **Absence and mutation.** Traced Windows `write_with` through optional inspection and creation. The shared parent validator now runs in the NotFound branch. Existing relative-open and no-reparse rules remain in place.
2. **Lock and cursor ownership.** Verified fixture reads use the handle that actually owns the lock. Coordinator contention uses the independent lock owner's handle, and poisoned lease tests retain an Arc before poisoning rather than recovering poisoned state. The byte reader restores the shared cursor. Final review additionally required comparing the no-follow identity of the named entry with the retained handle; this restores and strengthens the original named-byte oracle.
3. **Deletion and replacement authority.** Checked native creation includes DELETE while denying delete sharing. Rename fixtures use that existing handle; general shared opens are unchanged. Retry acquires deletion authority, and a consumed owner slot continues to fail closed.
4. **Fixture lifetimes.** Followed janitor guards in order: an active mutation barrier precedes coordinator inspection; ordinary cap-std directory handles can block deletion-capability acquisition. Explicit fixture drops make the intended later invariant observable. Heartbeat setup also releases its own pinned capability.
5. **Fault boundaries and error context.** Matched native metadata and final-identity checks to bootstrap hooks, and native rename/unlink to their existing semantic hooks. Windows retains its `secure ...` error context; Unix inspect/verify expectations are unchanged. Hooks do not replace operations or change normal production access masks.

## Test self-review

1. **RED provenance.** The same native workspace target reproduced all 29 original cases before the changes; sandbox failures are recorded separately and are not used as evidence of application regressions.
2. **Independent oracles.** Retained exact before/after bytes, object identities, namespace contents, primary and rollback diagnostics, and retry success. A passing test cannot substitute a weaker sharing mode or release the lock being tested.
3. **Case granularity and AAA.** Existing rstest invocations each exercise one input, failure boundary or cancellation point. Setup and evidence capture precede the target action; assertions follow observations and explicit retry. Platform-specific empty-slot assertions express the native contract in one invocation.
4. **Generated invariants.** The whole workspace group includes the existing bounded proptest state/lock checks, plus real large-namespace tests and cancellation matrices. No test is skipped or narrowed to avoid an original failure.
5. **Regression scope.** Verification includes neighboring file creation, identity mismatch, consumed-authority, cleanup retry, published mutation contracts, and strict whole-workspace lint. Full-suite results and independent review are recorded after those gates finish.

## Software design diagnostics

| Diagnostic | Assessment |
| --- | --- |
| Each module has one describable responsibility | Pass: worker I/O owns absence interpretation; managed cleanup owns authority; test helpers own fixture observations |
| Interfaces are simpler than implementations | Pass: shared parent validation and two small private test helpers hide native details |
| Internal changes stay behind interfaces | Pass: no public CLI, serialized format, share policy or deletion-authority API change |
| Interface comments describe guarantees | Pass: the fixture byte reader documents owning-handle and cursor guarantees; existing native capability contracts remain |
| Reviews discuss design | Pass: read-only diagnoses and five passes distinguish inspection from deletion authority and active from abandoned roots |
| Important decisions remain encapsulated | Pass: native handles and checks stay inside existing Windows modules |
| Boundaries are understandable from contracts | Pass: existing named failure and retry matrices express the invariants |
| Design improvement consumes 10–20 percent of effort | Unverified: no contemporaneous time split was measured |

Seven supported diagnostics yield 8.75/10. The remaining row needs actual time measurement, not a retrospective estimate.

## Verification status

| Gate | Result |
| --- | --- |
| Native workspace-group RED | 658 passed, 29 failed, 8 ignored; exit 101 |
| Missing-parent RED | 1 passed, 1 failed; exit 101 |
| Native workspace-group GREEN | 687 passed, 0 failed, 8 ignored; exit 0 |
| Native workspace group after review correction | 689 passed, 0 failed, 8 ignored; exit 0; includes both added regressions |
| Strict workspace Clippy, all targets and features | Final exit 0 on the reviewed source; two earlier test argument lint findings were corrected |
| Workspace formatting and diff whitespace | Exit 0 |
| Documentation site build (`uv run --frozen --no-sync python -m tools.docs_site --output site`) | Exit 0 after preserving the review records |
| Full workspace with all features, including CLI/core contracts | Exit 0: 17,424 passed, 0 failed, 45 ignored across 200 unfiltered harnesses |
| Independent review | One Important finding corrected; scoped re-review approved with no further findings |

Commands use `--locked -j 2 --quiet` and `--test-threads=2`. Builds use `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`, and `CARGO_INCREMENTAL=0`, preserving debug assertions. The full `cargo test --workspace --all-features --no-fail-fast` run includes both CLI and core contract features, so a second equivalent CLI-contract run is unnecessary. The first Clippy correction only changed test-helper argument names and explicitly accounted for the platform-unused argument. The full suite runs again on the final reviewed source, including the namespace-oracle correction described below.

## Independent review correction

Review of `3514ecdb` found one Important issue: reading through the owned handle preserved the lock but no longer proved that the entry name still referred to that file. A detached or replaced file could satisfy an assertion about the named lease on Unix. No other findings were reported.

Two named regression cases keep the original handle readable while removing its old name or installing a different file with identical bytes. Both failed on the old helper. The shared helper now compares no-follow named metadata with the handle identity before reading and restoring its position. All eleven converted observations supply the correct parent, name and owning handle. Scoped source review accepts the correction and found no new breakage; the affected suite passed 689 tests with 8 existing ignored fixtures and no failures. The final all-features workspace run and strict Clippy both exited 0.

The full-suite build started before this review was interrupted during compilation, before test execution, to avoid mixing revisions. Its empty log is retained as incomplete evidence. The final full-suite run uses the corrected source. Native Unix runtime behavior was not executed on this Windows host; shared fixture code was reviewed and the two namespace regressions also execute on Windows through retained rename authority.

## Final result and evidence

All 29 original Windows failures are resolved. The final source commit is `ca1db19d02acd7617c8271cf631ad96d67e6c664`.
The complete `cargo test --locked -j 2 --quiet --workspace --all-features --no-fail-fast -- --test-threads=2` run passed on the native Windows host.
Its 200 unfiltered harness summaries contain 17,424 passes, no failures and 45 existing ignored cases.
Four filtered child-process harness summaries are excluded from that aggregate to avoid recounting fixture executions.
The CLI library passed 2,690 cases with 27 existing ignored fixtures, and the analyzer passed 5,150 with five existing ignores.
No skip or ignored test was added by this follow-up.
The final strict whole-workspace Clippy run also exited 0.

The [initial review](windows-workspace/final-review.md), [scoped approval](windows-workspace/scoped-review.md), and [execution ledger](windows-workspace/progress.md) retain the findings and their disposition.
`windows-workspace/evidence.zip` preserves the original failure inventory, reproduction and final command logs, and command status/aggregate receipts.
`windows-workspace/evidence-manifest.json` records each archive member's size and SHA-256.
Review diffs can be regenerated from the recorded Git revisions.
The original research reports retain their historical counts and now link to this correction.
