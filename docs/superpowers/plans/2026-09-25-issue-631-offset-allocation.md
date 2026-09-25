# Issue #631 Offset Allocation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Clone only returned IDs for strict and diverse pages while preserving policy order and diagnostics.

**Architecture:** A borrowed diverse iterator retains score-tier and active-file state. Paging skips references and constructs a page-sized owned result. A bounded Lean corpus checks public behavior; an isolated allocator test guards the resource improvement.

**Tech Stack:** Rust, Cargo, Lean 4, existing guarded formal CI; no new dependencies.

**Spec:** `docs/superpowers/specs/2026-09-25-issue-631-offset-allocation-design.md`

## Global Constraints

- No public API changes, manifest changes, ranking changes, Python production changes, new dependencies, or changes to other issues.
- Use one guarded Lean command at a time, with 20-second wall time, 2048 MiB RSS, and 10000 heartbeats per theorem.
- Strict order is saved rank order; diverse order finishes contiguous score tiers and preserves active-file round robin.
- Offset is applied to the full policy order; result capacity and owned IDs correspond to the returned page.

## Review Focus

- Offset inside a diverse round-robin cycle must continue the existing cycle.
- A page crossing score tiers must complete the higher tier first.
- Exhausted singleton files must not restart or duplicate the dense file.
- `usize::MAX` counts and offsets must avoid arithmetic overflow.
- Discarded ID clones must fail the allocation guard even if the final vector is shrunk.

### Task 1: selection and allocation regression

**Files:** modify `crates/hoimin-cli/src/plan/selection.rs` and `selection_tests.rs`; create `crates/hoimin-cli/tests/selection_heap.rs`.

**Interfaces:** retain `select_top_candidate_ids_at(&[RankedPlanCandidate], NonZeroUsize, TopSelectionPolicy, usize) -> Vec<String>` and prefix wrapper. Add private borrowed `DiverseCandidates<'a>: Iterator<Item = &'a RankedPlanCandidate>`.

- [ ] Run baseline: `cargo test -p hoimin-cli --lib plan::selection_tests`. Expected: existing tests pass.
- [ ] Add literal page assertions for strict `[A1,A2,B1,A3,B2,B3]` and diverse `[A1,B1,A2,A3,B2,B3]`, including page boundaries and empty inputs. Add an isolated heap test over 1024 candidates with 4096-byte IDs; for both policies measure offsets 0, 512, 1023 with count 1. Require page capacity 1 and peak below 512 KiB. A test-local eager cloned prefix must exceed the guard at the final offset.
- [ ] Run `cargo test -p hoimin-cli --test selection_heap`. Expected: failure on current discarded prefix allocation/capacity, with semantic IDs unchanged.
- [ ] Implement the borrowed iterator and page collection:
  ```rust
  let limit = count.get().min(candidates.len() - offset);
  let mut selected = Vec::with_capacity(limit);
  selected.extend(ordered.skip(offset).take(limit).map(|candidate| candidate.id.clone()));
  ```
  Guard offset before subtraction. Strict `ordered` is `candidates.iter()`. Diverse owns only reference queues. Prefix selection delegates to offset zero.
- [ ] Run selection unit tests and heap integration test. Expected: all pass, including dense-file existing tests and broken allocation control.
- [ ] Review implementation and tests separately in three passes: ordering/ownership, boundaries/sensitivity, integration/maintenance. Record findings and actions in the report and commit the change.

### Task 2: durable public paging oracle

**Files:** create `formal/HoiminOracle/HoiminOracle/PagingModel.lean`, `PagingAuditMain.lean`, `corpus/paging.jsonl`, `crates/hoimin-cli/tests/lean_paging_oracle.rs`; update library imports, `lakefile.toml`, `.github/workflows/ci.yml`; write `docs/superpowers/reports/2026-09-25-issue-631-offset-allocation.md`.

**Interfaces:** consumes unchanged public plan/verify CLI and emits schema-1 cases with identity, strict correspondence mode, policy, offset/count, accepted/exit, expected IDs and complete order. Adapter maps stable IDs to source file/line roles only; it does not calculate expected order.

- [ ] Promote `/tmp/hoimin-round13-paging-oracle/Paging.lean` into the existing formal library layout. Preserve `slice_prefix`, three broken witnesses, asymmetric ranking fixture and 59 cases. Generator supports `--output`, `--check`, `--sensitivity` and `--stats`, following existing executables.
- [ ] Add public Rust adapter: create `a.py` with `True/False/+` and `b.py` with `True/+/+`, plan six retained candidates, save unchanged manifest, invoke dry-run verify per corpus row, compare IDs, errors, rank, selection order and requested/retained counts. Reject malformed schema/mode/policy, duplicate identities and missing matrix cases. Count-zero must identify parser rejection; out-of-range must identify offset rejection.
- [ ] Add generator/library modules to existing guarded CI and register `generate_paging|corpus/paging.jsonl|sensitivity`.
- [ ] Ask parent for the exclusive Lean slot. Run guarded module build and generator output/check/sensitivity commands one at a time. Expected: theorem checks pass, 59 generated cases, exact corpus freshness and all broken controls detected. Record elapsed/RSS limits; no unbounded search.
- [ ] Run `cargo test -p hoimin-cli --test lean_paging_oracle`. Expected: valid corpus and 59 public CLI matches (64-bit runner).
- [ ] Run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and one final `cargo test --workspace` with the shared Python venv available. Expected: all checks pass; record any environmental failures explicitly.
- [ ] Complete third-round reviews with actual final evidence, update checked steps, commit all work and hand branch to parent for independent review and PR publication.

## Plan self-review

1. **Spec coverage:** mapped both policies and each offset/count boundary to Task 1; mapped public diagnostics and Lean CI freshness to Task 2. Added page-crossing-tier and dense-file focus explicitly.
2. **Sensitivity review:** capacity-only evidence missed transient ownership. Added allocator peak with long IDs and retained broken eager-prefix control; this guards shrinking-after-cloning too.
3. **Interface and execution review:** existing CI requires `--check`/`--sensitivity` rather than bare JSON output. Added compatible generator interface and schema validation; explicit exclusive Lean coordination prevents concurrent resource-heavy commands. Worktree-local tests need the root Python environment linked or on PATH, with no environment installation.
