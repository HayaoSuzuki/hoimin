# cgroup Memory Page Rounding Implementation Plan

> **For Codex:** Use the `superpowers:executing-plans` workflow to implement this plan task by task and preserve red/green regression evidence.

**Goal:** Keep cgroup v2 hard memory enforcement available when the requested byte limit is not aligned to the kernel page size.

**Architecture:** Normalize only `memory.max` to the host base page size before writing, rounding down so the effective hard limit never exceeds the requested limit. Continue using byte-exact readback for the normalized value and for `pids.max`/`memory.oom.group`. Record a diagnostic whenever normalization changes the requested value so the effective enforcement remains observable.

**Tech Stack:** Rust, libc `sysconf(_SC_PAGESIZE)`, Linux cgroup v2, Cargo tests.

---

### Task 1: Define and test page normalization

**Files:**
- Modify: `crates/hoimin-cli/src/resource/linux.rs`

1. Add a host-testable helper that rounds a byte limit down to a supplied nonzero page size.
2. Cover decimal `1GB` with 4096-byte pages (`999,997,440`), already aligned limits, larger base pages, sub-page values, and invalid zero page size.
3. Run the focused helper tests and confirm the decimal-byte expectation fails before the production setup path uses normalization.

### Task 2: Normalize `memory.max` before strict verification

**Files:**
- Modify: `crates/hoimin-cli/src/resource/linux.rs`

1. Read the Linux base page size with `sysconf(_SC_PAGESIZE)` and validate the result.
2. Add a memory-specific writer that computes the effective limit, writes that value, and reuses `write_exact_limit` for strict readback verification.
3. Add an explanatory diagnostic when requested and effective values differ.
4. Leave the exact writers for `pids.max` and `memory.oom.group` unchanged.

### Task 3: Cover a delegated cgroup probe

**Files:**
- Modify: `crates/hoimin-cli/tests/process_handler.rs`

1. Add a Linux cgroup-v2 test that probes with decimal `1GB`.
2. When delegation is available, assert hard backend selection, page-aligned `memory.max` readback, and an effective-limit diagnostic; close and clean the backend.
3. Retain the suite's established skip behavior when the host does not delegate writable cgroups. The gated main-branch cgroup job will exercise the real kernel path.

### Task 4: Verify and review

1. Run the focused page-normalization tests.
2. Run `cargo fmt --all -- --check`.
3. Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
4. Run `cargo test --workspace --all-targets --all-features`.
5. Run `git diff --check origin/main...HEAD` and request an independent review.
6. Open the PR and require all PR checks to pass before merge; after merge, verify the gated cgroup job if repository configuration enables it.
