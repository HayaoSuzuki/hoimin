# Issue #347: consistent sub-file target selectors

## Cause and decision

Target resolution added source roots, then line and symbol restrictions, and
then explicit files. That last step replaced an existing target slice for
the same normalized path with a whole-file slice. As a result,
`--file pkg/a.py --line pkg/a.py:10-12` selected all of `pkg/a.py`, while the
equivalent `--source pkg --line pkg/a.py:10-12` retained the requested range.
Adding `--file` could also erase a symbol restriction on a file selected
through its required source root.

Resolution now establishes whole-file source and file targets before applying
line and symbol restrictions. This follows the existing source-selector model:
a sub-file selector narrows the matching file, while other files selected by a
source root remain whole-file targets. File paths still undergo the same root,
source-containment, Python-file, platform-equality, normalization, and sorting
checks. We also removed a dead `symbols.clear()` call from the line loop.
The resolver processes symbols after lines, so the call could not clear one.

## Compatibility

Valid commands change only when they name one file through both `--file` and a
sub-file selector:

| Invocation shape | Before | After |
| --- | --- | --- |
| `--file pkg/a.py --line pkg/a.py:10-12` | whole file | lines 10-12 |
| `--source pkg --file pkg/a.py --symbol a:run` | whole `a.py` | symbol `run` in `a.py` |

`--file` alone and `--source` alone still select whole files. Whole-file
selectors on different paths remain combined. Repeated and path-normalized file
selectors still collapse to one target. Multiple line ranges are normalized and
merged. If a target has both line and symbol restrictions, the analyzer retains
its existing intersection semantics: a candidate must satisfy both. `--symbol`
and `--changed` still require `--source`, and changed lines still intersect the
resolved explicit slices.

This compatibility correction prevents accidental mutations outside a requested
sub-file region. Remove the sub-file selector to request the former whole-file
behavior. A command with multiple invalid selectors may report a different
first error because the resolver now validates explicit files before line
selectors. The resolver still rejects the command.

## Regression evidence

The first regression compared file-plus-line resolution with source-plus-line
resolution for one real discovered path. Before the implementation change it
failed because the file form returned an empty line list, which denotes a whole
file, while the source form retained lines 4-7.

Four core cases and one CLI filesystem case exercise the corrected boundary:

- file-plus-line and source-plus-line produce the same normalized slice;
- a redundant file selector no longer erases a source-backed symbol selector;
- duplicate normalized file paths retain sorted, merged line ranges;
- changed lines intersect the resolved file-plus-line range and ignore another
  changed file; and
- real filesystem discovery selects only the named file and its requested
  range.

During the RED check, we restored the old files-last ordering. All four core
regressions and the CLI regression failed with whole-file slices. We then
restored the files-first ordering and reran the focused tests.

## Verification

Run from `.worktrees/issue-347-selector-consistency` with the repository
toolchain and isolated target directory:

```sh
CARGO_TARGET_DIR=/private/tmp/hoimin-issue-356-target cargo test -p hoimin-core --test target_policy
CARGO_TARGET_DIR=/private/tmp/hoimin-issue-356-target cargo test -p hoimin-cli --test target_handler
CARGO_TARGET_DIR=/private/tmp/hoimin-issue-356-target cargo test --workspace --all-features
CARGO_TARGET_DIR=/private/tmp/hoimin-issue-356-target cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
git diff --check
```

The implementation uses platform-neutral Rust. We ran the listed commands on
macOS. The focused core target-policy command exited 0 with 30 tests passed,
and the CLI target-handler command exited 0 with 27 tests passed. The full
workspace all-features command exited 0; its main CLI library run passed 535
tests with 9 ignored, and each integration, core, and documentation test binary
completed without a failure. Clippy exited 0 across all workspace targets and
features with warnings denied. Formatting and whitespace checks exited 0. We
removed the temporary `.venv` link after the full test run. We did not run
Windows or Linux tests.
