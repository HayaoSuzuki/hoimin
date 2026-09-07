# Issue #348: portable path colon boundaries

Issue: https://github.com/tokyogas-tech/hoimin/issues/348

Branch: `fix/issue-348-portable-paths`, based on main `fdf8150`.
Worktree: `.worktrees/issue-348-portable-paths`.

## Outcome

Normalized root-relative paths now reject a colon in every component. The same
policy applies to candidate and diagnostic records, exact fingerprint files,
fingerprint include patterns, and the Windows final-name UTF-16 guard. Paths
such as `pkg/file.py:stream` can no longer cross these boundaries as an NTFS
alternate data stream.

This change does not add the optional Windows trailing-dot or trailing-space
policy; that behavior is outside issue #348.

## Root cause and design

`normalized_relative_path`, exact fingerprint normalization, and fingerprint
pattern validation checked only the first slash-separated component for a
colon. A drive-prefixed path was rejected, but a nested alternate-stream name
was accepted. The Windows final-name guard rejected separators, NUL, `.` and
`..`, but did not reject the UTF-16 code unit for `:`.

The core validator now makes colon rejection part of `valid_path_part`, so the
existing all-components traversal enforces one rule for every component. The
two fingerprint validators likewise test every component. The final-name guard
rejects `:` alongside the other forbidden code units before any Windows file
operation receives the name.

## TDD evidence

Before the production changes, the focused regressions failed at the expected
boundaries:

- Candidate validation accepted `pkg/calc.py:stream`.
- Exact fingerprint resolution classified `nested/file.toml:stream` as missing
  instead of invalid.
- Fingerprint pattern resolution classified the same path as unmatched instead
  of an invalid glob.
- The platform-neutral UTF-16 final-name test accepted `name:stream`.

After the fix, the complete candidate-policy, fingerprint-input, analyzer
protocol, and final-name focused suites passed. The analyzer protocol test was
updated because it previously asserted that a nested-colon candidate path was
accepted.

## Verification

| Check | Result |
| --- | --- |
| Candidate-policy integration suite | 11 passed |
| Fingerprint-input integration suite | 20 passed |
| Analyzer-protocol integration suite | 40 passed |
| Platform-neutral Windows UTF-16 guard test | 1 passed |
| macOS serial workspace suite, all targets | 1,453 passed; 12 ignored; 0 failed |
| Workspace Clippy, all targets and features, warnings denied | Exit 0 |
| Formatting and whitespace checks | Exit 0 |

The final macOS commands used Rust 1.98 and the separate target directory
`/private/tmp/hoimin-issue-356-target`:

```sh
CARGO_TARGET_DIR=/private/tmp/hoimin-issue-356-target cargo test -p hoimin-core --test candidate_policy
CARGO_TARGET_DIR=/private/tmp/hoimin-issue-356-target cargo test -p hoimin-cli --test fingerprint_inputs
CARGO_TARGET_DIR=/private/tmp/hoimin-issue-356-target cargo test -p hoimin-cli --test analyzer_handler
CARGO_TARGET_DIR=/private/tmp/hoimin-issue-356-target cargo test -p hoimin-cli workspace::root::tests::windows_final_names_are_single_components --lib
CARGO_TARGET_DIR=/private/tmp/hoimin-issue-356-target cargo test --workspace --all-targets -- --test-threads=1
CARGO_TARGET_DIR=/private/tmp/hoimin-issue-356-target cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
git diff --check
```

Python-backed integration tests used the main repository's frozen Python
3.14.7 environment through a temporary worktree-local `.venv` symlink. The
symlink was removed after the workspace run.

Native Windows test execution was not available on the macOS host. An attempted
`x86_64-pc-windows-msvc` compile check reached the dependency build but could
not run because the host does not provide the MSVC assembler `ml64.exe`. The
UTF-16 name guard remains compiled and executed by its platform-neutral unit
test on macOS.

Independent review found no blocking issues. The coordinator reran the 11
candidate-policy tests, 20 fingerprint-input tests, formatting, and whitespace
checks before committing. Changed files do not overlap #349 or #368.
