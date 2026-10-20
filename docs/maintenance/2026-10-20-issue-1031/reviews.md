# Issue #1031 review record

The comparison base is `3054427a1b1b9a7d2f5cdd1b6ce62053e6c417ac`.
These reviews concern the working branch, not a new formal proof or a whole-CLI
memory limit.

## Design reviews

1. **Ownership:** core knows requested IDs, accepted discovery and worker state.
   Physical positions and sequential progress belong to the I/O replay owner.
2. **Admission:** one pending candidate read preserves request order. An idle
   worker admits a body; no speculative prefetch is needed.
3. **Discovery:** reaching all requested IDs does not end discovery. EOF detects
   trailing errors, and later duplicate IDs replace earlier bodies.
4. **Stopping:** unloaded in-flight IDs remain worker-owned until completion or
   stop retirement. Stops report accepted discovery only.
5. **Evidence:** vary payload, selection count and jobs independently. Count real
   record reads, measure live ownership before teardown, and separate plan/report
   costs from the candidate queue.

## Implementation reviews

1. **Core representation:** inspected `OrderedCandidatePhase`, `ReadCandidate`
   and `CandidateLoaded`. Queues now own strings; cursor/offset no longer cross
   the core boundary. Existing jobs-bounded worker/effect body clones remain.
2. **Scheduling:** inspected EOF conversion and indexed completion handling.
   Reads are serialized and require an idle worker. Indexed absence or a wrong
   ID is rejected; ordinary Next EOF retains its existing meaning.
3. **Stop transfer:** inspected `begin_stopped_mutant_drain` and its continuation.
   Reading an ID transfers that ID into the stopped queue; repeated stop
   reconstructs request order. Failed reads clear their worker before draining,
   avoiding an endless retry of unavailable data.
4. **Spool replay:** inspected `CandidateReplay::read` and `replay_one`. The
   selected-only map stores positions, updates after successful discovery and
   seeks directly for indexed reads. The bounded JSONL parser, sequence and
   offset validation remain the primitive.
5. **Shell ownership:** inspected blocking preparation/completion and shutdown
   draining. Concrete replay ownership returns even on read errors. Retired
   completions restore I/O state before stop effects execute; core membership
   remains the authority for which ordered IDs receive stopped results.
   Independent review found that streaming Next needed to restore its unaccepted
   position too. The replay owner now retains one position checkpoint, and the
   central discarded-completion path rewinds it before returning ownership.

## Independent review correction

The separate reviewer `/root/review_issue_1031` found an acceptance race: stopping
after a Next read completed but before core accepted it advanced the new I/O
cursor past an unreported streaming candidate. Three real-spool/core-transition
regressions reproduced the defect before the fix: All omitted its first NotRun,
Explicit raised `SelectedCandidateMissing`, and repeated stop omitted both
results.

`CandidateReplay` now records the start of its latest successful Next read.
Discarding that completion rewinds the position. Every read first clears the
checkpoint, so Id reads, errors and EOF cannot rewind a previously accepted
record. The index remains I/O-owned and core still controls ordered accepted
membership. This adds one optional position, no body cache and no public read
API. The reviewer traced the fix and found no remaining source-level issue.

## Verification reviews

1. **Payload sensitivity:** the old implementation failed all three heap
   ceilings with bodies allocated inside the window. The changed implementation
   passes in both serial and default parallel test execution. Thread-local
   metering fixes libtest interference without weakening the body bounds.
2. **State-machine contracts:** the 121 machine cases and existing stop/rank
   properties pass with contracts. EOF-tail failures, duplicate discovery,
   pending indexed reads, repeated stop, wrong/absent bodies and failed-body
   drain are covered. The added shell regressions connect real spool progress
   to core acceptance instead of assuming an atomic read/completion.
3. **I/O work:** 25 store tests retain parser validation and exercise selected
   positions, duplicates, malformed indexed records and retirement checkpoints.
   The counter is inside the actual bounded reader. Release cases independently
   assert exact N + S visits and logical record bytes.
4. **Measurement isolation:** 120 release executions compare the same harness
   and immutable baseline, with only protocol adaptation and the same reader
   counter hook. Fixtures are outside the window; selection/index setup is
   inside. Live state and effects survive snapshots, outputs are checked and
   dropped, and plan/report/process costs are explicitly excluded.
5. **Independent evidence audit:** `/root/review_issue_1031` checked source,
   linked library and binary hashes, raw stdout/commands for all 120 executions,
   all 60 pairs, every summary statistic and README table value. The reviewer
   confirmed the stopped-read correction and its GREEN logs. No Critical,
   Important or Minor findings remained; final whole-suite execution is recorded
   separately in the integration table.

The reviewer's final follow-up independently recounted the complete contracts
log (3,851 passed, 33 ignored, excluding nested filtered subprocess results),
checked the log hashes and integration table, and verified all five new OKF
source hashes. Its final verdict was **Ready to merge: Yes**, with no remaining
Critical, Important or Minor findings.

## Final design diagnostic

The seven technical/review rows of the software-design-philosophy diagnostic
pass: responsibilities fit one sentence, the logical interface is smaller than
the implementation, physical replay changes are hidden from core, comments
describe ownership, review addressed complexity, each module owns its decision,
and the boundary is documented. The eighth row asks for a measured design-time
share; this task did not maintain a time-allocation ledger. Conservatively the
score is **7/8, or 8.75/10**. Establishing that process metric would require
recording design time in a future task; it does not justify additional production
abstractions here.
