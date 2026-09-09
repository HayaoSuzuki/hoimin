# Issue #447 fingerprint input memory report

## Task 1 implementation

Fingerprint resolution now stores `Option<blake3::Hash>` in its ordered selection map. Each exact
root-relative read is hashed immediately, allowing its byte buffer to drop before the next exact
file is read. Glob-selected files keep the existing final traversal and safe reader; that read is
hashed before the unchanged hex string is emitted. The resolver API, sorted records, duplicate
handling, read order, error categories, and root-relative safety checks remain unchanged.

## Compatibility coverage

The resolver tests cover binary bytes including `0xff`, normalized exact aliases and duplicates,
glob/exact overlap, exact selection order, and a second resolve after changing a file. Existing
missing, directory, symlink, symlink-parent, non-UTF-8, unreadable, unsafe-path, and error-order
tests remain green.

## Performance RED

The issue's reproduced before-binary CLI measurement is the performance RED for this change. With
three samples per condition and the same 16 MiB files, exact-file peak RSS grew from 30.8 MB for
one file to 148.3 MB for eight files, while the equivalent glob input stayed around 31 MB. Hashes
and candidates matched. This is retained-input memory evidence, rather than an OOM or process
resource-limit claim. The controller owns the paired after measurement.

## Validation

All Cargo commands used `CARGO_TARGET_DIR=/private/tmp/hoimin-python-operator-target` and
`--offline` where applicable.

| Command | Result |
| --- | --- |
| `cargo test --offline -p hoimin-cli --test fingerprint_inputs` before change | 20 passed |
| Same command after change | 23 passed |
| `cargo test --offline -p hoimin-cli --test cli_config fingerprint` | 1 passed, 54 filtered |
| `cargo test --offline -p hoimin-cli --test plan fingerprint` | 3 passed, 43 filtered |
| `cargo fmt --all` and `git diff --check` | passed |

No new dependency, public API, schema, fingerprint algorithm, unsafe code, or CI memory/timing
threshold was added.

## Task report

Status: DONE

Concerns: none. Whole-workspace, MSRV, Clippy, paired RSS, independent review, and PR checks are
owned by the controller.
