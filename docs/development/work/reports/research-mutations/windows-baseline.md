# Windows baseline verification

The managed worktree starts at `4cf630f977dc266c6633969b57d0f9f251bb91d8`. The original dirty checkout was not used for implementation. Rust 1.100.0 and CPython 3.14.9 were used on Windows.

## Successful baseline

`cargo test --locked -p hoimin-core -p hoimin-python --lib -j 2 -- --test-threads=2` exited 0. Core ran 577 tests. The Python analyzer result was 4,788 passed, 0 failed and 5 ignored.

`uv sync --frozen --no-install-project --python <managed CPython 3.14.9>` installed the locked development dependencies in the worktree virtual environment.

## Python test infrastructure findings

The first full pytest run exited 15 without a summary. A verbose diagnostic run identified `test_repeated_sigterm_during_cleanup_preserves_deadline_error`: it sends SIGTERM to its own process without the Windows exclusion used by neighboring POSIX tests. Windows terminates that process instead of delivering a POSIX signal handler.

A run with just that test deselected completed: **2,664 passed, 77 skipped, 8 failed, 1 error** in 276.84 seconds. These failures occur in files unchanged by the mutation implementation.

Command: `uv run --frozen --no-sync pytest -vv --tb=short --randomly-seed=2367152648 -k 'not test_repeated_sigterm_during_cleanup_preserves_deadline_error'`.

| Area | Evidence | Required correction |
| --- | --- | --- |
| Boundary contract test doubles | Two tests autospec a Mock fallback for unavailable os.killpg | Supply a real callable signature for the simulated protocol |
| Boundary platform setup | Spawn-failure test exits as unexecuted before reaching its mocked spawn | Simulate the prerequisite platform/features explicitly |
| Boundary teardown | Removing an attribute introduced with create=True causes a second deletion during teardown | Use safe attribute deletion that also works when naturally absent |
| Creusot executable fixture | Nonexecutable-tool test uses shell scripts and chmod on Windows | Apply the established POSIX_SHELL contract |
| Lean guard fixtures | Two tests assume POSIX process-group and signal support | Apply or simulate the actual tested platform prerequisites |
| Documentation fixture | str(Path(relpath)) inserts Windows backslashes into Markdown URLs | Use as_posix() in the fixture; production rewrite_url already does so |

These are test-platform corrections, not changes to mutation semantics. Keep them in a separate commit and verify the affected tests before the final full run. Detailed diagnostic logs are in the ignored `.superpowers/sdd/research-mutations/` workspace.
