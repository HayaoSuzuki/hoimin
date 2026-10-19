# Task 5 fix round 2: native fixture diagnostic checkpoint

Base: `70ea57b2517777dea8e1445063f9cfd96e877128`.
Status: diagnosis only; no fixed/accepted claim. The final documentation draft
remains uncommitted pending actual native acceptance and whole-branch review.

## Repeated RED and comparison

Non-Linux follow-up run `39199825814`, Windows Rust job `117890288481`, failed
both existing bounded readiness cases even with `--test-threads=2`:
`abnormal_runtime_root_crosses_real_job_notification_and_cleans_up_within_six_seconds`
and `exited_root_is_observed_before_assigned_descendant_cleanup`. The library
reported 761 passed, two failed, 23 ignored in 57.69 seconds. Abnormal readiness
was absent, two processes remained active and emergency close succeeded. The
second fixture did not publish root/descendant identities. This is actual RED
for the scheduling mitigation; it was insufficient and is not a confirmed fix.
Full log: `task-5-evidence/native-ci/windows-rust-117890288481.log`.

The same-source resource job `117890288608` passed both cases and its other
checks (`task-5-evidence/native-ci/windows-resource-117890288608.log`). It uses
`--all-features` and filters the library to the Windows resource module after
Clippy; the failed job runs the full default-feature library. `contracts` is the
CLI's only optional feature and forwards to core contracts. Global-state and
feature differences remain candidates alongside process startup/resumption.

The metrics cwd-changing test finished at 04:39:13.242; abnormal readiness failed
at 04:39:21.100, after a six-second scope beginning about 04:39:15.1. These actual
timestamps do not support overlap with the cwd-mutating test. Effective cwd is
still logged to avoid substituting this inference for direct evidence.

The current suspension helper resumes the first enumerated thread belonging to
the PID and accepts every non-error return. Its comment assumes a sole primary
thread. A zero prior suspend count would mean the chosen thread was not suspended;
a value greater than one would mean it remains suspended. Neither condition has
been observed yet. Thread selection is a concrete candidate, not an established
cause. The checkpoint does not change selection, suspend/resume behavior or any
error handling outcome.

## Bounded, test-only instrumentation

- Both Python fixtures emit an initial marker before imports, then flush a sidecar
  at import, descendant-spawn, readiness-publication and release boundaries.
- A test guard records selected base interpreter, effective cwd, requested limits,
  active/registered process information, and at most eight files of 4096 bytes
  each before temporary-directory destruction. Environment logging is restricted
  to PYTHONHOME, PYTHONPATH, VIRTUAL_ENV and PYTHONDONTWRITEBYTECODE, each limited
  to 1024 characters. No general environment dump or credential logging is added.
- The direct root fixture captures stdout/stderr in files; the handler fixture
  retains its existing bounded output spool. Both retain all existing six-second
  deadlines, process/cleanup assertions and fixture limits.
- `cfg(test)` traces native queried Job limits, chosen thread ID and ResumeThread's
  previous suspend count. The OS error is captured before diagnostic output can
  affect it. Production builds emit no diagnostic trace.

The controller will push this diagnostic checkpoint and run the existing
non-Linux workflow. Default-feature full-library output will be compared with
the all-features resource-only output on native Windows. Diagnostic code will be
reassessed after the cause is measured: retain only maintainable, useful failure
context or remove temporary instrumentation with explicit follow-up verification.
No additional concurrency or timeout tweak is proposed.

## Local validation and limitations

`cargo fmt --all -- --check` passed. Python `ast.parse` accepted both embedded
raw-string diagnostic scripts. `git diff --check` passed. These are syntax and
format checks only; the macOS host cannot execute the Windows backend. No native
Windows GREEN is claimed, and no unrelated broad suite was repeated.

Exact script validation: extract the two Rust raw strings beginning
`print('fixture entered before imports'` from `windows.rs`, require two matches,
then call `ast.parse` on each. Both retain `time.sleep(0.005)` readiness polling.

## Five diagnostic self-reviews

1. **Evidence:** retained second native RED and successful same-source resource
   job; explicitly rejected a claim that two harness threads fixed the failure.
2. **Boundaries:** added observations before Python imports and across actual
   attach/resume, import, spawn and file-publication stages, rather than assuming
   that a live assigned process has executed fixture code.
3. **Semantics:** kept all timeouts/assertions, limits, thread selection and return
   handling. Stored the original OS error before logging. No production output,
   alternate arithmetic or proof changes were introduced.
4. **Safety and scope:** logs are test-only, file reads and whitelisted environment
   values are bounded, temporary fixture files are removed by existing ownership,
   and no whole-environment dump is used.
5. **Validation:** formatted Rust and parsed both scripts; distinguished these
   checks from missing Windows compilation/runtime evidence. The diagnostic run
   must establish the failure stage before any further repair or acceptance claim.

## Diagnostic outcome and lifecycle adapter repair

Diagnostic checkpoint `dd2674dda9fc7db0aa420683cb8ef3f9cf690174` ran in native
workflow `39201076428`. Eight jobs passed, including actual Windows all-target/
all-feature Clippy. Windows Rust job `117893978366` failed later in the lifecycle
oracle, but both six-second Job Object cases passed. The comparison resource job
`117893978598` also passed all 30 library + five handler + two scope tests. Its
five recorded ResumeThread calls returned prior suspend count 1; both fixture
phase traces reached publication and cleanup in about 0.77/0.80 seconds. The full
workspace harness captures passing-test diagnostics, so absence of those details
in its log is not an observed thread count. The original two intermittent
readiness failures remain unexplained. Neither instrumentation nor scheduling
is described as a proven root-cause repair.

The new Windows failure was `stop_preserves_accepted: InfrastructureError`:
`hoimin CLI did not stop after its total timeout`. The helper already starts
independent Tokio stdout/stderr reader tasks before waiting for the durable
premise; an undrained output pipe is not supported by the code. Decoding the
5,950-byte stdout from the actual failure produced complete JSON with one killed
result, one not_run result, exit_code 4 and clean execution cleanup; stderr was
empty. The baseline took 44 ms and first mutant 58 ms. A complete report does not
prove that the process exited before forced cleanup.

The adapter's separate 8-second durable-premise phase remains unchanged. After
that premise it waited only 12 seconds for process exit, equal to the configured
run limit. This gives a fast first result's latency as the only allowance for
shutdown. That is inconsistent with the existing product contract:

- `docs/development.md`, "Shutdown deadline invariants", anchors timeout cleanup
  at the run deadline plus a fixed two-second grace.
- `crates/hoimin-cli/src/shell.rs`: `SHUTDOWN_GRACE = Duration::from_secs(2)` and
  `ShutdownBudget::for_total_timeout_with_grace` implement that deadline.
- `crates/hoimin-cli/src/telemetry.rs`: `FLUSH_BUDGET = Duration::from_millis(250)`
  bounds `Guard::drop`; `main.rs` drops that guard after `run_from` returns and
  before process exit.

The repair derives the fixture exit wait from the unchanged 12-second run limit
plus those existing 2-second and 250-ms allowances: 14.25 seconds **after the
premise is observed**. This is not a whole-process wall-clock cap. Premise setup,
the earlier 8-second check, and bounded failure cleanup/output draining retain
their separate limits. No product limit, shutdown rule, total-timeout stop cause,
Lean model, corpus, accepted outcome, or six-second Job Object assertion changes.

## Deterministic RED and final focused checks

Three paused-time tests exposed the old guard: an exit at 13 seconds within
shutdown grace and one at 14.125 seconds during binary flushing were both rejected;
a process that never exits hit the guard at 12 instead of the contract-derived
14.25-second boundary. Each finite acceptance case is a separate macro-generated
test; the nonexit test verifies that the new guard remains bounded. These tests
exercise the adapter timing policy, not a substitute lifecycle oracle.

Commands from the issue worktree:

```sh
CARGO_TARGET_DIR=/tmp/hoimin-issue999-target CARGO_BUILD_JOBS=2 cargo test --locked -p hoimin-cli --test lean_result_lifecycle_oracle timeout_fixture_ -- --nocapture
CARGO_TARGET_DIR=/tmp/hoimin-issue999-target CARGO_BUILD_JOBS=2 cargo test --locked -p hoimin-cli --test lean_result_lifecycle_oracle -- --nocapture
CARGO_TARGET_DIR=/tmp/hoimin-issue999-target CARGO_BUILD_JOBS=2 cargo clippy --locked -p hoimin-cli --test lean_result_lifecycle_oracle -- -D warnings
cargo fmt --all -- --check
git diff --check
```

RED: first command exited 101, zero passed/three failed/six filtered, 2.34-second
build and 0.00-second paused-time execution. GREEN after the composed-bound repair:
all nine focused tests passed in 13.52 seconds, including all six strict corpus
scenarios and actual bounded descendant cleanup. Focused Clippy passed in 7.33
seconds; formatting/whitespace checks passed. Local free disk before the build
was 95 GiB. These local results do not establish native Windows runtime acceptance;
the controller must run the new checkpoint there.

`evidence/task-5-fix2.tar.gz`, top-level `task-5-fix2/`, retains the two diagnostic
native logs, decoded failing JSON, RED terminal-result summary and actual GREEN/
Clippy logs. Original full CI metadata remains in the final CI evidence collection.
The broader acceptance documentation draft is still pending/uncommitted.

## Five repair self-reviews

1. **Cause and limits:** distinguished confirmed guard/contract mismatch from the
   unproven original Job Object timing mechanism. Did not infer process exit from
   JSON or reinterpret failed native runs as passed.
2. **Model correspondence:** preserved the durable killed-result premise, strict
   Lean expectations and all current scenarios. An exceeded guard still triggers
   bounded cleanup and an infrastructure error rather than semantic comparison.
3. **Timing composition:** used one 12-second run constant for CLI arguments and
   the outer guard, then added only documented 2-second shutdown and existing 250-ms
   telemetry budgets. Kept the 8-second premise stage distinct from the exit wait.
4. **Regression sensitivity:** observed all three boundary tests fail on the old
   guard, then pass with the real lifecycle suite; a never-exiting future still
   expires at the finite composed bound. AAA and one finite case per test remain.
5. **Scope and verification:** only the test adapter changes in this repair; the
   previous diagnostic checkpoint remains test-only instrumentation. Focused
   Clippy/format checks passed; native CI and independent review remain required.
