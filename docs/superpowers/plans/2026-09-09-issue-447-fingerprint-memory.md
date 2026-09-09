# Issue 447 fingerprint memory Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Hash exact fingerprint files as they are read so their bytes are not retained together.

**Architecture:** Keep the existing ordered path map and safe read sequence; replace optional byte buffers with optional BLAKE3 digests.

**Tech Stack:** Rust1.88+, existing BLAKE3/Camino/workspace safe-read API.

**Spec:** docs/superpowers/specs/2026-09-09-issue-447-fingerprint-memory-design.md

## Global Constraints

- Work only in .worktrees/issue-447-fingerprint-memory, branch perf/issue-447-fingerprint-memory, independently based on58817cf.
- No new dependency, unsafe code, public API, schema or fingerprint algorithm changes.
- Rust MSRV1.88; preserve exact read order, error categories and safe root-relative read checks.
- Memory must not retain all exact file contents; minimize source comments.
- Do not add timing or OS RSS thresholds to CI.

### Task 1: Retain fingerprint digests instead of file bytes

**Files:**
- Modify: crates/hoimin-cli/src/fingerprint_inputs.rs
- Test: crates/hoimin-cli/tests/fingerprint_inputs.rs
- Document: docs/superpowers/reports/2026-09-09-issue-447-fingerprint-memory.md

**Interfaces:** resolve and recheck/recheck_manifest signatures remain unchanged. The local selection map stores Option<blake3::Hash> instead of Option<Vec<u8>>. workspace::read_root_relative remains the reader.

- [ ] Before production edits, run existing fingerprint_inputs tests and add compatibility cases with binary content, normalized duplicate exact paths, mixed glob/exact selections and repeat resolve after file modification. Example expected digest uses the actual byte content:

```rust
let bytes = [0, 0xff, b'\n', b'x'];
std::fs::write(root.join("data.bin"), bytes).unwrap();
let records = resolve(root, &["*.bin".into()], &["./data.bin".into(), "data.bin".into()]).unwrap();
assert_eq!(records.len(), 1);
assert_eq!(records[0].path, "data.bin");
assert_eq!(records[0].hash, blake3::hash(&bytes).to_hex().to_string());
```

- [ ] Record performance RED using the existing reproduced before-binary RSS evidence in the Issue; correctness compatibility tests should pass before the optimization. Do not distort a behavior test to manufacture failure for a performance-only change.
- [ ] Keep the safe exact read/error mapping, but retain the digest:

```rust
let digest = blake3::hash(&bytes);
selected.insert(path, Some(digest));
```

In the final map traversal, use the existing digest or read/hash the glob file with unchanged include-error mapping; only then convert the digest to the emitted hex string. Ensure no Vec of exact bytes is stored in map or closure state across iterations.
- [ ] Run `cargo test --offline -p hoimin-cli --test fingerprint_inputs`, relevant CLI config fingerprint tests and plan_verify fingerprint tests. Preserve existing failure ordering, symlink and update detection tests. Inspect the actual test names before choosing filters and confirm nonzero counts.
- [ ] Self-review lifetime, duplicate read behavior and errors. Write tracked report, commit implementation/tests/docs and report back. Controller owns paired actual CLI memory measurement, whole-workspace/MSRV/Clippy gates, independent reviews and PR; no publishing or duplicate broad suites.

## Controller validation and delivery

- Use /private/tmp/hoimin-445-before as immutable main baseline CLI (it is main58817cf) and the #447 binary for paired RSS/hash/candidate comparisons.
- Full workspace/all-features tests; MSRV1.88 and Clippy all-targets/all-features warnings denied; fmt/diff checks.
- Complete task review, scoped fix reviews if required, final whole-branch review, report final evidence, then PR closing #447. Keep Issue-specific worktree.

## Plan self-review 1 — behavior preservation

Checked the plan against sorting, normalization, overlap and error criteria. Test expected hashes from binary bytes, not UTF-8 decoding. Compatibility cases cover read updates across calls, and existing safe-read/error tests remain in the focused gate. The performance RED is the actual before measurement, not a fabricated correctness assertion.

## Plan self-review 2 — validation mechanics

The actual before/after CLI comparison must reuse one fixture root and compare full fingerprint records and candidates, while treating selector configuration differences separately. The test filters must return nonzero tests. Existing fixtures exercise symlink parents and stale file detection; the optimization must not bypass their reader or reorder errors.

## Plan self-review 3 — delivery coverage

Mapped every acceptance condition to code, existing/new compatibility tests, or paired RSS evidence. Task and controller responsibilities avoid duplicate broad runs. The code step makes the lifetime change explicit, all affected interfaces are identified, and both review stages and final report/PR are included. No placeholder, missing file or contradictory instruction remains.
