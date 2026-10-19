# Task 5 follow-up: controlled Windows test scheduling

Base: `46ef6b9ef240ce644a11a5ab03f82c69b79f2afd`.
Status: local checks passed; actual Windows full-suite acceptance pending.

## Observations and working hypothesis

Non-Linux run `39198000067`, Windows Rust job `117884829867`, failed the two
existing CLI library cases
`abnormal_runtime_root_crosses_real_job_notification_and_cleans_up_within_six_seconds`
and `exited_root_is_observed_before_assigned_descendant_cleanup`. The first did
not publish atomic readiness before its shared deadline, reported two active
Job Object processes, and completed emergency close successfully. The second
did not publish root/descendant identities before its deadline. That library
binary reported 761 passed, two failed and 23 ignored. The full log remains
`task-5-evidence/native-ci/windows-rust-117884829867.log`.

The fixture/backend file is unchanged relative to main `f4460fc0`. Both tests
start their shared six-second deadline before starting/importing the base Python
interpreter and publishing readiness. The helper reads the base interpreter
from pyvenv.cfg, avoiding a virtualenv redirector; the sibling fixture is
ping.exe. The failed full workspace command used default harness concurrency.
The same source's separate Windows resource job passed its 37 tests.

The retained `docs/maintenance/2026-10-17-issue-967.md:281` records these same two
bounded test cases failing in parallel on a normal Windows host, followed by
unchanged success serially (1.24 seconds) and with two threads (0.71 seconds).
Its abnormal test failed readiness; its exited-root test failed descendant exit,
whereas the current failure is earlier, at identity publication. That record
explicitly leaves the exact cause unconfirmed. Together these observations
support test-resource contention as a working hypothesis, not a proven mechanism.

## Minimal repair and limits

Only the manual Windows workspace test invocation changes to
`cargo test --workspace -- --test-threads=2`. The existing macOS invocation
remains selected by the complementary condition. Every workspace test still
runs exactly once for each platform. Production code, Python fixture code,
source/tool pins, proof runner, test timeouts and all six-second assertions
remain unchanged. No test is skipped, no retry is added and no semantic deadline
moves to a later phase. This controls harness scheduling; it does not establish
production timing under arbitrary host load. The controller's seventh Ruling
records the tradeoff: a longer CI run and possible remaining failure if another
cause dominates. Actual Windows execution is the remaining hypothesis test.

The Task 5 reviewer approved the local implementation and raised two Minor
metadata/documentation findings. The October 14 218/93 graph counts now reflect
the current October 19 213/111 inventory. Both OKF references to the adoption
report now use consistent source revision/state/fingerprint for this follow-up
working tree, rather than disagreeing about its introduction at `5033e1bb`.

## Local verification

A new parameterized workflow contract checks one execution per platform and
Windows-only two-thread scheduling. Against the old workflow it failed the
Windows case (unbounded command) while the macOS control passed: one failed,
one passed, 176 deselected. The first full workflow check then identified two
existing equality/allowed-condition contracts that needed the new explicit
platform branch. Those contracts now compare the unchanged macOS path with
automatic CI and permit the complementary condition; the new contract owns
both platform command selections.

Commands use the isolated `/tmp/hoimin-issue999-task5-venv` and
`UV_CACHE_DIR=/tmp/hoimin-issue999-uv-cache`:

```sh
python -m pytest -q tests/test_ci_workflow.py -k controlled_windows_concurrency
python -m pytest -q tests/test_ci_workflow.py tests/test_docs_site.py
ruff check tests/test_ci_workflow.py
ruff format --check tests/test_ci_workflow.py
ty check tests/test_ci_workflow.py
UV_PROJECT_ENVIRONMENT=/tmp/hoimin-issue999-task5-venv uv sync --frozen --group fuzz --group workflow --no-install-project
python -m tools.workflow_lint
python -m pytest -q tests/test_workflow_lint.py
```

Final workflow/docs checks: 194 passed in 7.62 seconds. Ruff lint/format and ty
passed. Pinned Actionlint, Zizmor and ShellCheck returned success; Zizmor retains
three ignored findings and its existing unsupported Python-shell warnings for
release.yml. Eight real-linter fixtures passed in 0.51 seconds. No new lint
finding appeared in the scheduling change. The isolated workflow-group sync removes the editable hoimin install;
its Python dependencies and newly pinned lint tools remain. Main's environment
is untouched. No local Windows runtime result is claimed.

## Five fix self-reviews

1. **Failure evidence:** compared exact failure phases, unchanged source and the
   historical native record. Kept the mechanism as a hypothesis; prior
   descendant-exit failure is not rewritten as a readiness failure.
2. **Requirements:** retained every original shared six-second bound, full test
   selection and production settings. The change affects only Windows harness
   scheduling; macOS keeps its default command.
3. **Workflow selection:** complementary OS conditions select exactly one full
   suite. The parameterized regression fails absent/duplicate/wrong commands;
   existing parity and allowed-condition contracts still check the other steps.
4. **Evidence limits:** local Python checks validate configuration, not Windows
   timing. Only a fresh native Windows run can close external acceptance. The
   actual failed run and earlier passing resource slice remain separate records.
5. **Documentation/provenance:** corrected both scoped-review minors, retained
   the seventh Ruling and cost, and preserved fix logs/review records durably.
   Current native proof results remain tied to their unchanged math/runner
   source checkpoint; no pending CI is described as passing.
