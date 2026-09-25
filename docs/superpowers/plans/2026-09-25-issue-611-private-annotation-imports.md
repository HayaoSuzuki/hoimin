# Private annotation imports Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans inline. Parent handles independent review and publication.

**Goal:** Exclude unsafe class-private annotation import provenance at both endpoints.

**Architecture:** Inherit compiler class context in name scopes, reject mangled roots, and conservatively taint reverse-spelling writes at their actual destination. Preserve the issue's runtime model and add independent static eligibility.

**Tech Stack:** Rust, Lean 4.32.2, CPython 3.14.

**Spec:** `docs/superpowers/specs/2026-09-25-issue-611-private-annotation-imports-design.md`

## Global constraints

Design/plan precede code. No new subagents/worktrees/pushes. Cargo uses `target/batch-analyzer`, jobs1, debug0, incremental0. Reserve Lean slot before each group; 20seconds/2048MiB/heartbeat10000. Preserve artifacts and model runtime permission. User authorizes execution through publication without another approval pause.

## Review focus

- Leading class underscores, trailing alias dunders and underscore-only classes.
- Compiler private context persists into methods/functions despite lexical class skipping.
- Reverse spelling and global/module writes must taint the actual imported key.
- An unrelated class must not taint another scope's canonical alias.
- Source, destination, qualified aliases and all shared annotation operators.

## Task 1: Regressions and conservative provenance

Files: `crates/hoimin-cli/src/analyzer/rust_tests.rs`, `rust.rs`.
Consumes: `analyze_types`, name-scope builder and shared `annotation_import_stable`.
Produces: optional inherited private prefix plus conservative canonical-write tracking.

- [ ] Add table regressions for both issue examples, private clean imports, boundary spellings, reverse spellings, globals, nonlocals, nested methods/functions, unrelated classes, and qualified roots. Example source:
  ```python
  from typing import Iterable
  class C:
      from typing import Sequence as __Seq
      _C__Seq = tuple
      value: __Seq[int]
  ```
  Select `type_sequence_iterable`; require zero for this case, exact positive pair for ordinary/dunder/underscore-only controls.
- [ ] Run `cargo test -p hoimin-cli --lib private_annotation_import`; expected RED on the issue's class-private source/destination and reverse-write cases.
- [ ] Add `private_prefix: Option<String>` to `NameScope`, inherited in `new_scope`, reset at class creation. Add a small helper for `__name` without trailing `__`; effective key is prefix+name.
- [ ] In `annotation_import_stable`, reject roots transformed by the occurrence's private prefix before fast paths. In builder tracking and import writes, recognize transformed imported names and conservatively write their `usize::MAX` instability at the existing global/nonlocal owner. Record both directive spellings. Keep raw tracking unchanged.
- [ ] Focused tests GREEN, then all library tests. Expected: no candidate for ambiguous roots, ordinary spelling positives preserved.

## Task 2: Formal corpus and public adapter

Files: `HoiminOracle/PrivateAnnotationImportModel.lean`, `PrivateAnnotationImportAuditMain.lean`, generated `corpus/private-annotation-import.jsonl`, `crates/hoimin-cli/tests/lean_private_annotation_import_oracle.rs`; update formal root/lakefile, CI lists, `tests/test_ci_workflow.py` registry in lake order.
Consumes: issue's mangle/lookup model, public CLI and CPython.
Produces: versioned deterministic 36-case corpus and public safety/precision/false-kill checks.

- [ ] Preserve `mangle`, `lookup`, `envFor`, `allowed`, `lastWriteShadows` and deliberate broken rules from issue. Add static projection:
  ```lean
  def eligible (cls alias : String) (overwrite : Bool) : Bool :=
    allowed cls alias overwrite && mangle cls alias == alias
  ```
  Prove eligible implies allowed; retain runtime expectations even for suppressed clean private aliases.
- [ ] Reserve slot; guarded build/generate/check/sensitivity/stats. Expected 36 cases and all three sensitivity witnesses; no timeout/OOM.
- [ ] Adapter reconstructs sources from corpus fields, observes keys/probe identity in a separate Python process, and executes public plan with target-line selection. Compare exact count/operator/text/line/span/symbol, enforce runtime allowed for emitted candidates. Validate closed coordinate set, schema/mode/duplicates/missing cases.
- [ ] Run source and destination original fixtures through explicit Python baseline and public run; expected complete/zero killed/no mutants. Observe public RED against baseline before production fix (or temporarily revert only owned changes), then GREEN.
- [ ] Add exact executable registry and CI freshness/sensitivity entries; run `.venv/bin/python -m unittest tests.test_ci_workflow -q`, expected 40 tests pass.

## Task 3: Review, documentation and gates

Files: README and `docs/superpowers/reports/2026-09-25-issue-611-private-annotation-imports.md`.
Consumes: completed implementation and maintained corpus.
Produces: precise conservatism contract and self-contained validation record.

- [ ] Three implementation reviews: compiler context, write ownership/directives, shared annotation consumers. Three test reviews: runtime observations, boundary/sensitivity, integration/schema/CI. Fix actual findings and record them.
- [ ] Run `cargo test --workspace`, workspace/vendor fmt and exact CI Clippy (`--workspace --all-targets --all-features -- -D warnings`, locked parser `--lib --no-deps -- -D warnings`), full Ruff and CI contract tests. Expected all pass.
- [ ] Record RED/GREEN, finite bounds/cost, exclusions, commands and outcomes; commit implementation and evidence. Return clean branch to parent for independent review.
