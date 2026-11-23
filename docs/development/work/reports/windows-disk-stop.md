# Windows disk measurement and stop reporting

## Reproduction and design

A synthetic Python command repeatedly replaces a small cache file with `os.replace`.
On Windows, directory enumeration can retain the previous file ID while a subsequent
handle-relative open returns the replacement. The meter currently treats this normal
regular-file replacement as a fatal measurement error. With a larger synthetic tree,
shutdown can exhaust its grace before the final report carries the stop reason.

Measure regular files from the opened, non-following handle. Keep identity validation
for directories, root capabilities, reparse-point exclusions, depth/entry bounds and
scan deadlines. Do not increase shutdown grace or weaken configured disk limits.

The core owns ordered disk-stop diagnostics. Emit the first reason before draining
candidate reports or beginning cleanup, then resume the existing stop path after its
acknowledgement. The shell cancels processes immediately but lets these outputs run
before waiting for process/I/O drain, within the original shutdown deadline.

### Design reviews

1. Reproduction: distinguish a normal file replacement from directory authority changes;
   only the former may be accepted.
2. Boundaries: keep Windows handle semantics in the meter and output order in the core;
   do not expose a new CLI switch or platform detail to callers.
3. Lifecycle: an early diagnostic must continue candidate draining and original-file
   verification, rather than reuse the ordinary failure continuation that skips them.
4. Reentrancy: repeated stop events must neither retire the in-flight diagnostic nor
   duplicate it; a stop before `run_started` must preserve event ordering.
5. Bounded shutdown: cancel immediately, retain the original grace deadline, and report
   before potentially blocking drain. Failed output cannot justify unbounded retries.

Using `software-design-philosophy`, the proposed design scores 8/10: the meter hides
filesystem races and the machine hides report sequencing. The failed diagnostic is
temporal coupling between reporting and shutdown in the shell. Address it by allowing
output effects before the already-required drain, without introducing a second report
serializer or a public lifecycle API. Reassess this after implementation.

## Implementation and verification plan

1. Add deterministic Windows replacement tests at ordinary and maximum depth, plus
   generated sizes. Observe their failure before changing file measurement.
2. Add core tests for early diagnostics, stop repetition, pre-start ordering and
   continued finalization. Observe their failure before changing the state machine.
3. Add a shell regression with a controlled outstanding I/O operation: the diagnostic
   must be visible even when drain exhausts grace.
4. Implement the two fixes, replay the independent synthetic command and run native
   Windows tests, formatting, lint and the repository's required checks.
5. Review the complete diff, commit, open a PR and follow CI through completion.

### Plan reviews

1. Isolation: use a clean branch based on main; synthetic fixtures have no external
   repository or interpreter-path dependency.
2. Red/green evidence: inject replacement after enumeration, rather than rely on a
   probabilistic stress loop as the only regression test.
3. Test structure: each test has one scenario and Arrange/Act/Assert phases; use
   parameterization for boundary cases and property tests for generated sizes/order.
4. Security regression: retain directory identity, no-follow and bounded-handle tests;
   test replacement at the depth boundary separately from ordinary traversal.
5. Delivery: verify both `run`/`verify` shared execution behavior and failure reporting;
   no final-success report may be invented when shutdown is incomplete.

## Implementation and test review results

### Implementation reviews

1. Filesystem authority: the regular-file exception requires both an enumerated
   non-directory and an opened ordinary file. Directory-to-file, file-to-directory
   and directory-to-directory replacement still fail. The deepest frame uses the
   existing typed, non-reparse open and retains the 129-directory-handle bound.
2. Stop sequencing: the initial pending `run_started` is still retired as before;
   repeated stops preserve its replacement and the subsequent disk diagnostic.
   A dedicated continuation resumes the original candidate/finalization path.
3. Failed delivery: review found that the new continuation could retry a failed
   diagnostic. It now shares ordinary diagnostic failure recovery and requests
   cleanup directly. Six parameterized cases cover both output stages and all
   report-failure variants. A final interleaving review also found that an earlier
   preparation failure could be lost behind the new disk diagnostic; its pending
   diagnostic is now resumed, with a red/green regression test.
4. Shutdown ownership: cancellation remains immediate. The shell drains pending
   owners before any subsequent non-output effect, using the first shutdown budget.
   External stop events cannot fall through to an unbounded completion join.
5. Interface and diff review: no CLI option, schema field, filesystem limit or grace
   duration changes. An independent read-only review found no remaining actionable
   defects after the failed-delivery correction.

The final scoped design scores 9/10 against `software-design-philosophy`: filesystem
identity decisions remain in the meter, ordered output remains in the core, and task
ownership remains in the shell. No failed diagnostic rows remain for the changed
interfaces. The shell still has temporal coupling between reporting and shutdown;
the added gate documents and enforces it without exposing a new public protocol.

### Test reviews

1. Red/green: all three original replacement regressions failed with identity-change
   errors before the meter fix. The shell regression failed with empty diagnostics
   before deferred drain was implemented, then passed.
2. Windows boundaries: 38 disk tests passed, including generated replacement sizes,
   root/maximum-depth replacement, directory substitutions and native filenames.
3. State transitions: 580 core unit tests and 244 machine integration tests passed.
   Generated repeated-stop sequences cover both pending output stages; cancellation
   retains its existing exit-code precedence. Existing tests now acknowledge the
   early diagnostic before checking the unchanged continuation.
4. Failure behavior: the blocked-materialization fixture retains its I/O owner past
   grace, observes the reason on stderr and requires a bounded error without a final
   success event. Failed-report tests require direct cleanup and retain the disk cause.
5. End-to-end oracle: the independent cache-replacement command with 2,000 static
   fixture files now produces `run_finished`, `complete: true`, exit code 0 and no
   disk stop. Broader workspace checks and CI are recorded in the accompanying PR.

Native Windows workspace library/binary tests passed: 8,429 passed and 42 ignored.
With all core features enabled, 587 unit tests and 244 machine integration tests
passed. Workspace formatting and all-target/all-feature Clippy checks also passed.

Each added test has one scenario with Arrange/Act/Assert phases. Parameterized tests
cover fixed boundaries and property tests cover generated sizes and stop sequences.
If the report writer itself remains blocked, another diagnostic cannot be delivered
through that owner within the same grace; the fix does not add a concurrent writer
or extend the deadline to conceal that failure.
