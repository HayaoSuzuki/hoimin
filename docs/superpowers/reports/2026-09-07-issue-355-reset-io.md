# Issue #355: avoid reads of size-mismatched worker files

## Scope and decision

We selected P2 [#355](https://github.com/tokyogas-tech/hoimin/issues/355) as an
improvement to existing mutation execution. The user requested bug fixes and
existing-feature improvements ahead of new research features. P2 #338 already
has its OOM classification fix on main; #369 concerns CI audit coverage.

This change implements #355's suggested option (b). During reset, a size mismatch
proves that a worker file needs restoration. We use the existing no-follow stat
to reject the comparison before reading changed worker bytes. On Unix we also
check the opened file's existing metadata before `read_to_end`, so a size change
between stat and open can take the same path. Matching sizes still require full
byte and permission comparison. We added no timestamp/identity-based equality
cache, metadata-only acceptance, or opt-out flag.

The work lives in `.worktrees/issue-355-reset-io`, branch
`perf/issue-355-reset-io`, based on `5f48e8a`. This report and README travel with
the implementation. Main's untracked files and other retained branches remain
untouched.

## Limits and compatibility

We retain full reads of snapshot contents and same-size worker files. This does
not reduce unchanged-tree read volume, change asymptotic reset cost, or establish
an end-to-end speedup. The benefit is avoiding reads and temporary buffers for
worker files whose lengths differ, particularly large generated contents.
Contracts still perform the existing post-reset comparison.

We also removed the unused `ManifestEntry.modified` field and its metadata
lookup from manifest collection, as suggested in the issue. Content hashes and
size still come from the bytes that we read. This removes a public Rust struct
field: downstream Rust code reading or initializing `modified` must change.
The field did not participate in content matching and has no serialized CLI
report or plan representation, so this change does not alter those schemas.

The optimization only returns `false` earlier. The caller uses the existing
capability-relative restore path; symlink/reparse and regular-file checks remain
before the size comparison. Concurrent writers remain subject to the existing
workspace ownership assumptions; this change does not introduce snapshot
isolation for arbitrary concurrent writes.

## Regression and I/O evidence

Baseline reset tests passed 6 tests with 1 manual benchmark ignored. We added
real-filesystem tests before implementation. Grow/shrink tests failed because
they observed the unwanted worker reads; empty-file and preserved-mtime cases
characterized behavior that we must retain.

The fixture has a pristine 9-byte target and an unchanged 1 MiB padding file.
These are bytes consumed by instrumented reset reads, not physical device I/O.

| Changed target | Worker bytes before | Worker bytes after | Snapshot bytes, both |
| --- | ---: | ---: | ---: |
| Shrunk to 4 bytes | 1,048,580 | 1,048,576 | 1,048,585 |
| Grown to 2 MiB | 3,145,728 | 1,048,576 | 1,048,585 |

With contracts enabled, one verification pass adds 1,048,585 bytes on each side
after restoration. The tests assert those counts, the restored bytes, and the
original permissions. Empty-file restoration also passes. A same-size change
with its original mtime restored still causes a full read and correct reset.
Existing same-size I/O and unchanged-file inode tests remain in place.

Independent review found no blocking correctness issue. The reviewer confirmed
the restricted performance claim and called out the Rust struct compatibility
change documented above.

## Reproduction

Validation on macOS (Rust 1.98, Python 3.14.7 frozen environment): the reset
tests passed 10 tests with 1 ignored; `cargo test --workspace --all-features`
passed, including the existing ignored-test exclusions. Workspace Clippy with
all targets/features and warnings denied, formatting, and whitespace checks
passed.

Linux aarch64 workspace library tests passed 141 tests with 3 ignored, both with
and without contracts. We used a nonroot UID/GID `501:20` container from
`rust:1.98-bookworm`, read-only source, and a separate writable Cargo mount.
Linux workspace Clippy with all targets/features and warnings denied also passed.
Windows runtime tests, the full Linux workspace suite, and standalone Python
tests were not run. The Windows-only manifest fixture was updated for the
removed field; Windows execution remains unverified.

From this issue's worktree, with the repository toolchain (Rust 1.98):

```sh
uv sync --frozen
cargo test -p hoimin-cli --lib workspace::reset::tests --quiet
cargo test -p hoimin-cli --features contracts --lib workspace:: --quiet
cargo test --workspace --all-features --quiet
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
git diff --check
```

The two red/green tests are `reset_skips_reading_shorter_worker_contents` and
`reset_skips_reading_larger_worker_contents` in
`crates/hoimin-cli/src/workspace/reset.rs`.

We did not poll GitHub Actions or dispatch extra workflows.
