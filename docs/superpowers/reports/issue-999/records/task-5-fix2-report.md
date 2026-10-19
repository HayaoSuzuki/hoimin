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
