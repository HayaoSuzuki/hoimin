# Git/Python changed-line implementation plan

> **For agentic workers:** Use superpowers:executing-plans inline. Root arranges independent review and owns CI/merge; publish using the installed gh stack extension.

**Goal:** Preserve changed-source candidates for universal Python newlines without changing Git context or selection contracts.

**Architecture:** Translate normalized Git LF intervals by scanning raw bytes before Python-line intersections; whole-file new paths use physical-line counts. Keep patch parsing and raw source identity unchanged.

**Tech Stack:** Rust/Tokio, real Git repositories, CPython, Lean-generated JSONL, public CLI adapters.

**Spec:** docs/superpowers/specs/2026-09-25-issue-612-python-changed-lines-design.md

## Constraints and review focus

One issue per branch, based on 632 until merged. Assigned batch-analyzer Cargo cache; one job, no incremental/debug information. Ask root for the global Lean slot before any Lean command; one process, 20-second/2048-MiB guard, theorem heartbeat 10000. No weakened runtime expectations or hand-edited corpus. No 620/621 scope expansion.

Test CRLF end-byte/EOF behavior; sparse disjoint Git ranges and numeric overflow; unborn physical-range double conversion; Windows path equality and excluded unreadable files; delete-only context zero versus positive context, rename/binary, line/symbol intersections, BOM/Latin-1 and saved-plan identity.

## Task 1: baseline, model, and public RED

Files: formal/HoiminOracle/HoiminOracle/ChangedLinesModel.lean; formal/HoiminOracle/ChangedLinesAuditMain.lean; corpus/changed-lines.jsonl; crates/hoimin-cli/tests/lean_changed_lines_oracle.rs; lakefile.toml, HoiminOracle.lean, CI generator list, tests/test_ci_workflow.py registry, formal README.

- [ ] Run existing target_handler and lean_changed_context_oracle tests on the 632 base.
- [ ] Preserve the issue's 36-case domain and original model equations in an imported small model. Keep generated cases/JSON serialization and --stats/--sensitivity/--check/--output handling in an executable. Add stable case IDs and exact schema validation, but do not change eligibility to match the old implementation.
- [ ] Under the granted guard, compile proofs/witnesses, generate corpus, regenerate/check freshness, and record time/RSS/domain statistics. Register generator in all four places, including Python CI contract map in executable order.
- [ ] Public adapter creates isolated Git repositories in all four states, checks CPython AST plus plain-plan identity/location, checks actual Git LF hunks for staged/tracked states, then compares --changed identity fields only (ranking intentionally differs). Aggregate all mismatches to preserve evidence of 20 failures.

```rust
assert_eq!(observed_identity_fields, expected_identity_fields, "case {id}");
```

- [ ] Add public run regression for original CR counterexamples; baseline must pass and one mutant must be killed. Observe failures before production edits. Add strict schema/duplicate/case-matrix checks for the corpus.

## Task 2: byte-coordinate conversion and regression tests

Files: crates/hoimin-cli/src/target/git.rs (small private mapping module if this keeps the scanner self-contained); crates/hoimin-cli/tests/target_handler.rs.

- [ ] Add focused real-Git tests for mixed newline tracked/untracked/unborn ranges, non-UTF-8 byte bodies, context zero/positive deletion, explicit Python-line/symbol intersections, rename and binary preservation. Confirm the new CR cases fail on baseline.
- [ ] Add a pure monotone conversion helper with normalized positive Git intervals. Scan bytes with separate Git/Python row counters; capture first/last Python row covered by each Git interval, excluding trailing empty EOF row. Reject out-of-range positive intervals and checked-output overflow.

```rust
let physical_end = byte == b'\n' || (byte == b'\r' && next != Some(b'\n'));
```

- [ ] Filter eligible patch paths using logical_path_equality_key, then read with WorkerRoot and translate only patch-derived ranges after binary/deletion exclusions. Maintain whole-file Python counting for new/untracked paths; union and normalize only after both sides use physical coordinates.
- [ ] Run pure boundary/differential tests and all 36 adapter cases; require strict matches with no infrastructure errors. Run the original public run regression again.
- [ ] Perform three implementation reviews: coordinate off-by-one/EOF/overflow; caller unit flow/path/read errors; Git context and existing selection semantics. Record actual findings and fixes.

## Task 3: cross-contract checks and completion

Files: public adapter/regression tests, formal README and docs/superpowers/reports/2026-09-25-issue-612-python-changed-lines.md.

- [ ] Exercise public plan/run for LF/CRLF/CR/mixed cases, saved-plan verify, raw span/hash/ID preservation, explicit --line/--symbol intersections, and at least Latin-1 and UTF-8 BOM newline controls. Check standard text/eol attributes preserve row correspondence.
- [ ] Perform three test reviews: independent model/public observation; positive/negative and legacy contract coverage; source bytes/process isolation/infra-error classification. Preserve any setup failure separately from semantic mismatches.
- [ ] Run focused target/parser and all changed-target/context/lines oracle tests, Python CI registry tests, full workspace tests, exact two Clippy gates, workspace/vendor formatting, and corpus freshness under the unchanged guard.
- [ ] Obtain independent root review. After 632 merges, rebase onto latest main without retaining the merged 632 commits, rerun relevant integration checks, commit report, publish via gh stack and hand off PR/head/evidence. Root owns CI polling/merge/cleanup.

## Plan self-reviews before implementation

1. Requirement coverage: fixing only untracked line counts would miss staged/tracked failures. Include both paths and preserve all four Git states in the original 36-case corpus; run must verify actual killed mutants rather than only candidate counts.
2. Oracle/adapter independence: ranking differs intentionally under --changed and must not cause false mismatches. Compare identity/location fields, record actual hunk LF numbers, and use CPython AST to corroborate model rows. Add all registry surfaces so CI freshness registration cannot silently diverge.
3. Boundary and integration review: preserve 632's changed-context API on the base branch; do not reintroduce delete-only exclusions for positive context. The scanner uses raw bytes and no per-line table; test ending boundaries and non-ASCII bytes separately. Ask for Lean slot only when ready, and retain the original 20-second/2-GiB bounds.
