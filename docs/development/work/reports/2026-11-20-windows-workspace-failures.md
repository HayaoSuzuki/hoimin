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
2. **Lock and cursor ownership.** Verified fixture reads use the handle that actually owns the lock. Coordinator contention uses the independent lock owner's handle, and poisoned lease tests retain an Arc before poisoning rather than recovering poisoned state. The byte reader restores the shared cursor.
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
| Strict workspace Clippy, all targets and features | Exit 0 after correcting two underscore-prefixed test argument references |
| Workspace formatting and diff whitespace | Exit 0 |
| Full workspace with all features, including CLI/core contracts | In progress; no completion claim yet |
| Independent review | Pending |

Commands use `--locked -j 2 --quiet` and `--test-threads=2`. Builds use `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`, and `CARGO_INCREMENTAL=0`, preserving debug assertions. The full `cargo test --workspace --all-features --no-fail-fast` run includes both CLI and core contract features, so a second equivalent CLI-contract run is unnecessary. The final Clippy correction only changes test-helper argument names and explicitly accounts for the platform-unused argument; the full suite exercises that final source.
