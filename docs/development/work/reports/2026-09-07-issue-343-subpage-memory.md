# Issue #343: reject subpage cgroup memory limits

## Scope

We selected [#343](https://github.com/tokyogas-tech/hoimin/issues/343) because a
small positive `--max-memory` could become `memory.max = 0` and cause resource
failures in place of meaningful test execution. We prioritized this production
bug over the P2 CI expansion #369. P2 #338's OOM classification fix already exists
on main; the preceding #355 improvement has merged.

The user authorized autonomous implementation through PR creation. Work lives
in `.worktrees/issue-343-subpage-memory`, branch
`fix/issue-343-subpage-memory`, based on main `3fe3693`. This report is committed
with implementation and README in that worktree.

## Contract

The Linux hard backend rounds byte limits down to host page granularity and
caps them below the kernel's unlimited sentinel. Requests smaller than one page
must fail with a diagnostic naming `--max-memory`, requested bytes, and minimum
bytes. A successful normalized value must be positive. A page-counter range
that cannot represent even one finite page is invalid kernel data.

Requests of one page or more retain the existing rounding and finite cap. We do
not round upward beyond the requested budget. Validation must precede root
cgroup creation and any write of `memory.max`.

This is Linux hard-backend validation, not a platform-independent CLI parser
minimum. macOS's documented best-effort behavior and other portable backends
remain unchanged. One page is a representability boundary, not a recommendation
for a practical Python memory budget. We do not promise that a one-page process
can run Python successfully.

The backend may already have created its run cgroup and launched a migration
probe during capability detection. The new guard precedes the requested test
process's root cgroup and command, not those earlier probe operations.

## Implementation and regression evidence

`normalized_memory_limit` now rejects subpage requests with
`InvalidCgroupMemoryLimit`, carrying the requested byte count and host page size.
It also rejects a page-counter range containing no positive finite page.
`system_memory_limit` shares host page/sentinel calculation between the root
preparation guard and the writer. The second check keeps the writer safe for
direct calls without creating a separate unchecked writing path.

The baseline pure Linux-resource tests passed 14 cases on macOS. New tests
cover 4 KiB, 16 KiB, and 64 KiB pages, requests of zero, one byte, 100 bytes,
page minus one, exactly one page, and page plus one. Before the fix, subpage
requests and the zero-only page-counter range returned `Ok(0)`; the regressions
failed as intended. Existing large-limit rounding and sentinel tests remain.

Two Linux-only regressions call the real private writer and root preparation
using temporary ordinary directories. Before the fix the writer succeeded and
overwrote the sentinel file; root preparation progressed to event-file reads.
After the fix both must produce the `--max-memory` diagnostic, preserving the
sentinel file or empty parent directory and root registry. A positive writer
test checks exact-page write/readback and page-plus-one rounding diagnostics.
These tests establish rejection before cgroup I/O without needing delegation;
they do not simulate kernel enforcement.

Independent review found no blocking implementation issue and prompted the
probe-versus-test-process clarification above.

## Reproduction

From the dedicated issue worktree:

```sh
uv sync --frozen
cargo test -p hoimin-cli --lib resource::linux::tests --quiet
cargo test --workspace --all-features --quiet
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
git diff --check
```

On Linux, also run:

```sh
cargo test -p hoimin-cli --lib resource:: --quiet
cargo test -p hoimin-cli --features contracts --lib resource:: --quiet
```

## Final validation

- macOS: pure Linux-resource normalization tests passed all 16 cases.
- macOS: `cargo test --workspace --all-features --quiet` passed, including
  521 passing library tests with 9 ignored and the workspace integration tests.
- Linux Docker (`rust:1.98-bookworm`): resource tests passed 28 cases with
  1 ignored, both with default features and with `contracts` enabled.
- Workspace Clippy, all targets and all features with warnings denied, passed
  on macOS and Linux.
- Formatting and `git diff --check` passed.

The Linux writer/root tests use ordinary temporary directories. A live delegated
cgroup OOM end-to-end test, the full Linux workspace test suite, Windows tests,
and standalone Python tests were not run for this change. Existing ignored
tests remain ignored.

No GitHub Actions polling or extra workflow dispatch is part of this delivery.
