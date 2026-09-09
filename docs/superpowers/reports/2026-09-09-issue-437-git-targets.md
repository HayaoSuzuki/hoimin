# Issue #437 implementation report

## Change

TargetHandler now supplies its resolved target slices to an internal scoped Git resolver. Current-file paths from unborn indexes and untracked discovery are filtered through `intersect_changed` before content I/O, including an explicitly empty scope. The public `handle_git` entry stays unscoped.

Current-file reads use `WorkerRoot` in `spawn_blocking`. Unsupported links and nonregular entries are skipped through `WorkspaceError::InvalidPath`; disappeared files are detected through `is_missing`; other I/O failures remain Git failures. Unix `WorkerRoot::read` rejects a nonregular final entry before opening it and retains the no-follow open-and-metadata boundary for replacement races.

## Regression coverage

The RED runs showed the old ambient reads fail on self-links with `Too many levels of symbolic links`. The FIFO regression uses an untracked symlink to a FIFO because Git does not list a direct FIFO; it asserts Git lists the `.py` symlink, runs in a subprocess with a five-second deadline, and its cleanup guard kills and reaps a child on panic or timeout. The child exercises both standalone `handle_git` and `plan --changed`.

Focused tests cover standalone external links, unborn indexed replacement links, excluded unreadable regular files, empty scopes, missing parents, and direct `WorkerRoot` FIFO rejection. Existing WorkerRoot tests cover linked parents.

## Focused verification

Executed with `CARGO_TARGET_DIR=/private/tmp/hoimin-python-operator-target`:

- `cargo test -p hoimin-cli --test target_handler standalone_git_skips_an_untracked_python_self_link` — RED before implementation, GREEN after.
- `cargo test -p hoimin-cli --test target_handler changed_target_skips_an_excluded_untracked_python_self_link` — RED before implementation, GREEN after.
- `cargo test -p hoimin-cli --test target_handler changed_target_skips_an_excluded_unreadable_regular_file`
- `cargo test -p hoimin-cli --test target_handler standalone_git_skips_an_untracked_link_to_an_external_regular_python_file`
- `cargo test -p hoimin-cli --test target_handler standalone_git_skips_an_indexed_unborn_path_replaced_by_an_external_link`
- `cargo test -p hoimin-cli --test plan changed_plan_skips_an_untracked_fifo_with_a_bounded_subprocess`
- `cargo test -p hoimin-cli workspace::root::tests::read_rejects_a_fifo`
- `cargo test -p hoimin-cli --test target_handler` — 34 passed.
- `cargo test -p hoimin-cli target::git::tests` — 12 passed.
- `cargo test -p hoimin-cli --test lean_changed_target_oracle` — 7 passed.
- `cargo clippy -p hoimin-cli --tests -- -D warnings`

The controller owns workspace-wide validation, independent review, push, and PR creation.

## Review fixes

An explicit empty scope now exits current-file collection only after Git path decoding, so it cannot fall through core intersection semantics that treat an empty explicit target set as unscoped. A regression uses an all-excluded unreadable regular file to prove that no current-file read occurs.

`WorkerRoot::read` now applies the final nonregular capability check before both Unix and Windows reads. A second structural check after a failed open classifies a replacement directory as `InvalidPath`, while failures for still-regular files remain I/O errors. The portable unborn-index replacement-directory regression protects that behavior. Unix-only plan fixture imports are cfg-gated for Windows Clippy.

The review-fix validation reran `cargo test -p hoimin-cli --test target_handler` (36 passed), the bounded FIFO plan test, the Lean changed-target oracle (7 passed), and `cargo clippy -p hoimin-cli --tests -- -D warnings`. A Windows target check could not complete because this macOS host lacks `ml64.exe` and the Windows C build environment; the source uses the shared capability boundary and the portable directory regression covers the corrected classification path.

## Independent controller checks

The real CLI was exercised with a normal changed Python file and an excluded `unused.py` entry. With the prior Git implementation, an ordinary project succeeded, a self-link failed with exit 2, and a FIFO link exceeded a three-second external deadline. With this branch, all three cases returned exit 0 and the same `ok.py` `binary_add_sub` candidate. The external harness kills and waits for a timed-out process. Artifacts: `/private/tmp/hoimin-437-cli-check.py` and `/private/tmp/hoimin-437-{before,after}.jsonl`. The before binary is a saved earlier build whose Git target implementation is unchanged from the issue baseline; timing is not used as a performance claim.

- `cargo +1.88 check --offline --workspace --all-targets --all-features --locked` — passed.
- `cargo clippy --offline --workspace --all-targets --all-features -- -D warnings` — passed.
