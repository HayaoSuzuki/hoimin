# Prepared annotation imports Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans to implement this plan inline. Parent owns independent review/publication; do not spawn or publish.

**Goal:** Prevent prepared class mappings from being mistaken for stable annotation imports.

**Architecture:** Place a conservative namespace guard in the shared endpoint stability check. Keep runtime identity distinct from static eligibility in a Lean corpus replayed through CPython and public CLI.

**Tech Stack:** Rust, Lean 4.32.2, CPython 3.14.

**Spec:** `docs/superpowers/specs/2026-09-25-issue-605-prepared-annotation-imports-design.md`

## Global constraints

Commit this plan and the design before code. Use the assigned exclusive Cargo target, one build job, no debug/incremental. Request root's Lean slot before any Lean command; 20 seconds / 2048 MiB / heartbeat 10000. Preserve artifacts. No approval pause is required: user authorized implementation through publication.

## Review focus

- A global source alone must not authorize a class-visible destination.
- An explicit class import cannot certify arbitrary custom mapping reads.
- Qualified aliases require the same root-name checks as direct imports.
- Nonlocal must not bypass class mapping lookup; lexical functions must bypass it.
- A runtime-safe empty mapping must not force unsafe static inference.

## Task 1: Regression and shared resolver

Files: `crates/hoimin-cli/src/analyzer/rust_tests.rs`, `rust.rs`.
Consumes: `analyze_types(source)` and existing `may_have_prepared_namespace`.
Produces: guarded `annotation_import_stable(offset, name, source_order_known)`.

- [ ] Add a table test selecting only `type_sequence_iterable`, with plain/module/lexical/both-global positives and prepared class/method/generic method/one-global/nonlocal/explicit-import/qualified-alias negatives. Assert exact count and endpoint text.
- [ ] Run `cargo test -p hoimin-cli --lib prepared_annotation_import`; expected RED: prepared class still emits a pair.
- [ ] Insert after global redirect and before nonlocal handling:
  ```rust
  if scope.may_have_prepared_namespace {
      return false;
  }
  ```
- [ ] Run focused tests; expected GREEN, existing builtin tests unchanged.

## Task 2: Maintained public oracle

Files: new `formal/HoiminOracle/HoiminOracle/PreparedAnnotationImportModel.lean`, `PreparedAnnotationImportAuditMain.lean`, generated `corpus/prepared-annotation-import.jsonl`, new `crates/hoimin-cli/tests/lean_prepared_annotation_import_oracle.rs`; update `HoiminOracle.lean`, `lakefile.toml`, `.github/workflows/ci.yml`.
Consumes: existing public `run_with_io`, `PlanManifest` and guarded CI conventions.
Produces: deterministic schema-1 corpus with identities, runtime allowed, static count and optional run check.

- [ ] Preserve all 16 issue matrix inputs, add boundary controls and original integer-destination reproducer. Model formulas:
  ```lean
  def identity (visible injected : Bool) : Bool := !(visible && injected)
  def allowed (sv dv a b : Bool) : Bool := identity sv a && identity dv b
  def eligible (sv dv ordinary : Bool) : Bool := (!sv || ordinary) && (!dv || ordinary)
  ```
- [ ] Prove soundness under ordinary => no injection; preserve fixed source-only, destination-only, and lexical-class-capture witnesses. Run guarded generation/freshness/sensitivity after root grants the slot.
- [ ] Adapter validates schema/mode/closed IDs/unique markers. For each isolated case run Python identity probe then public plan; compare target count, pair, byte span, line and symbol. Assert any emitted pair satisfies Lean runtime allowed. Run baseline and public run for the original integer-destination example; assert complete, zero killed and no mutants.
- [ ] Observe public adapter RED against baseline resolver (temporarily revert only guard if Task 1 is already green), restore guard and observe GREEN. Record infrastructure errors separately.
- [ ] Register model, executable and generated-corpus checks in existing Lean CI lists; generated values never edited by hand.

## Task 3: Review and final gates

Files: `docs/superpowers/reports/2026-09-25-issue-605-prepared-annotation-imports.md`, README contract clarification if needed.
Consumes: Task 1 shared predicate and Task 2 corpus/adapter.
Produces: self-contained evidence and final implementation commit.

- [ ] Review implementation three times: lookup precedence, shared source/destination and alias consumers, scope construction/compatibility. Review tests three times: runtime premises, adapter rejection/sensitivity, CI/freshness/coverage. Record actual findings and fixes.
- [ ] Run `cargo test --workspace`; expected all nonignored tests pass. Run `cargo fmt --all -- --check`, vendor fmt, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, vendor parser clippy; expected exit 0.
- [ ] Record exact commands, RED/GREEN results, Lean cost/bounds, exclusions and correspondence status. Commit all implementation and evidence; return commit IDs to root for independent review.
