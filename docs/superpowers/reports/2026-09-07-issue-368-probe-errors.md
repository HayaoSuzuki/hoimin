# Issue #368: preserve repository probe failures

Issue: https://github.com/tokyogas-tech/hoimin/issues/368

Branch: `fix/issue-368-probe-errors`, from main `fdf8150`.
Worktree: `.worktrees/issue-368-probe-errors`.

## Scope and cause

This batch prioritizes existing behavior: #348's portable path validation,
#349's published config JSON, and #368's failed-command diagnostics. Their
implementation and report files do not overlap. The P2 memory classification
issue #338 already has its production fix in main; #369 concerns CI expansion.

The current workflow already routes repository probes through its bounded
command runner and spool. On failure, `SubprocessProbe.text` constructed a
`CalledProcessError` without reading stderr, then discarded the spool. The
workflow grouped that exception with missing-tool errors. Users therefore saw
`tool_unavailable` after Git rejected a missing base ref.

## Implementation

The probe reads at most 32 KiB of retained stderr before cleanup and attaches
UTF-8 text to the exception. Invalid UTF-8 bytes use replacement characters.
Stderr-read and spool-cleanup failures remain secondary notes, preserving the
command's original exit code. Existing truncation metadata also supplies a note.

The workflow handles `CalledProcessError` as `command_failed`, while missing
executables remain `tool_unavailable`. Timeout handling retains its existing
branch. `_cli_error_detail` includes stderr within its existing 20 KiB cap.

The Markdown report did not render `RunRecord.error`. It now displays that
field beside the run state, joining lines and applying the existing inline-code
escaping. The JSON report retains the multiline diagnostic. We added no source
comments or new exception hierarchy.

## Regression evidence

The baseline reporting suite passed 122 tests with one skip. The first new
tests reproduced missing stderr (`None`) and the wrong `tool_unavailable`
classification. After those fixes, the real-Git test exposed the missing
Markdown error field. Its final assertions cover `run.json`, `report.md`, and
the failed command's deleted spool.

The five added tests exercise:

- A real child process exiting 128 with Japanese stderr, retained after cleanup.
- A real Git repository with no `origin/main`, producing `bad revision`.
- Oversized, non-UTF-8 stderr with bounded error output.
- Injected stderr-read and cleanup errors that remain secondary to exit 128.
- A missing executable that remains `tool_unavailable`.

## Verification

On macOS with Python 3.14.7:

```sh
python -m unittest discover -s tests -p 'test_*.py' -q
ty check tools/focused_mutation.py tools/focused_mutation_support/reporting.py
git diff --check
```

The full Python suite passed 855 tests with 56 platform/optional skips. The
production type check and whitespace check passed. The existing reporting test
file has 130 `ty` diagnostics on both base and this branch, with none in the new
test region after type narrowing. We did not change unrelated typing debt.
Independent review found no issues and reran the five regressions successfully.

## Targeted mutation check

The local debug binary stopped during plan discovery with
`duplicate unary-not operator start` in `analyzer/rust/fact_index.rs`. It had
not started a baseline or mutant. We left that separate analyzer assertion
unchanged and removed the temporary plan directory.

The installed binary at the main development environment's `.venv/bin/hoimin`
completed the same bounded plan and verification. Candidate
`m1_7739f7dc7951704bcb7fe074518cb410fc4714029b8383900d928bc3f1da586b`
changes `record.exit_code != 0` to `== 0` at line 532. The two relevant regression
tests passed the baseline, then killed the mutant (test exit 1). Verification
reported one killed mutant, zero survivors, complete, and exit 0.

Both attempts used one worker, an 8 GiB workspace limit, a 10 GiB free-space
reserve, and explicit best-effort memory approval on macOS. The successful run
recorded clean execution-workspace cleanup. Exit traps removed the exact
temporary plan/report directories. This check uses the installed release
binary; it does not certify the debug analyzer or a full-file mutation score.

Windows and Linux execution were not part of this Python diagnostic change.
We did not poll Actions or dispatch extra workflows.
