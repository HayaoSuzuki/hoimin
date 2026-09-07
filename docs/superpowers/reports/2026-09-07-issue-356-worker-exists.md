# Issue #356: worker existence below a missing parent

Issue: https://github.com/tokyogas-tech/hoimin/issues/356

Branch: `fix/issue-356-worker-exists`, based on main `8aab791`.
Worktree: `.worktrees/issue-356-worker-exists`.

## Outcome

`WorkerRoot::try_exists` now returns `Ok(false)` when any intermediate parent
component is absent. It still rejects linked and non-normal paths, returns I/O
errors for non-directory parents, reports `Ok(false)` for an absent final entry,
and reports `Ok(true)` for a regular file.

## Root cause and design

`try_exists` called `open_parent(path, false)` before inspecting the final entry.
`open_parent` treats a missing parent as an I/O error. That contract serves read,
write, and mutation operations, but it prevented `try_exists` from reaching its
final-entry `NotFound` branch.

The private `open_parent_if_present` helper keeps traversal relative to the
retained worker-root handle. It validates normalized components, rejects link or
reparse parents, and uses no-follow directory opens. The helper returns
`Ok(None)` only when `open_dir_nofollow` reports `NotFound`; it maps every other
open failure through the existing parent-error path. Both `is_missing` and
`try_exists` now use this helper, so they share one missing-parent rule without
changing `open_parent` for mutation and read callers.

`try_exists` retains its final no-follow `stat`. A final link or reparse entry
remains `InvalidPath`, an absent final entry remains `Ok(false)`, and another
inspection error remains an I/O failure.

## TDD evidence

The first regression test was
`try_exists_returns_false_when_an_intermediate_parent_is_missing`. Before the
production change, this command failed at the expected branch:

```sh
CARGO_TARGET_DIR=/private/tmp/hoimin-issue-356-target cargo test -p hoimin-cli workspace::root::tests::try_exists_returns_false_when_an_intermediate_parent_is_missing -- --exact --nocapture
```

The failure returned `WorkspaceError::Io` with operation `open worker parent`
and OS error 2. After the fix, the six focused tests passed on macOS and Linux.
They cover a regular file, missing final entry, missing intermediate parent,
regular-file parent, linked parent, and non-normal path.

## Verification

| Check | Result |
| --- | --- |
| macOS focused `try_exists` tests | 6 passed |
| macOS serial `hoimin-cli` package, all features | Exit 0; every test target passed |
| macOS serial workspace suite, all features | Exit 0; existing ignored tests remained ignored |
| Linux aarch64 non-root focused `try_exists` tests | 6 passed |
| macOS workspace Clippy, all targets and features, warnings denied | Exit 0 |
| Formatting and whitespace checks | Exit 0 |
| Independent review | No findings; READY |

The final macOS commands used Rust 1.98 and the separate target directory
`/private/tmp/hoimin-issue-356-target`:

```sh
CARGO_TARGET_DIR=/private/tmp/hoimin-issue-356-target cargo test -p hoimin-cli workspace::root::tests::try_exists_ -- --nocapture
CARGO_TARGET_DIR=/private/tmp/hoimin-issue-356-target cargo test -p hoimin-cli --all-features --no-fail-fast -- --test-threads=1
CARGO_TARGET_DIR=/private/tmp/hoimin-issue-356-target cargo test --workspace --all-features --quiet -- --test-threads=1
CARGO_TARGET_DIR=/private/tmp/hoimin-issue-356-target cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
git diff --check
```

The integration tests resolve Python through `<worktree>/.venv/bin/python`. A
test-only symlink pointed that path to the main repository's frozen Python
3.14.7 environment during the package and workspace runs. We removed the
untracked symlink before handoff.

The Linux command used `rust:1.98-bookworm`, UID/GID `501:20`, read-only source,
and separate writable Cargo and target mounts:

```sh
docker run --rm --user 501:20 -e CARGO_HOME=/cargo-home -e CARGO_TARGET_DIR=/target -v /Users/hayao/RustroverProjects/hoimin/.worktrees/issue-356-worker-exists:/workspace:ro -v /private/tmp/hoimin-issue-356-linux-cargo:/cargo-home -v /private/tmp/hoimin-issue-356-linux-target:/target -w /workspace rust:1.98-bookworm cargo test -p hoimin-cli --lib workspace::root::tests::try_exists_ -- --nocapture --test-threads=1
```

An initial non-serial package run exited 101. Six integration targets required
the absent worktree-local `.venv` path. Two public CLI tests in the same run
reported a missing temporary worker directory at process spawn. The serial run
with the test environment supplied passed every package target; this change did
not modify those tests or their setup.

Windows execution, the full Linux workspace suite, and standalone Python tests
were outside this Rust path-handling change and were not run.

The coordinator reran the six focused tests, formatting, and whitespace checks
before committing. This branch's source and report do not overlap #360 or #367.
