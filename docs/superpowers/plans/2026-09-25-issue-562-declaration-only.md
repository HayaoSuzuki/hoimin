# Issue #562 Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans to implement these steps inline. The user authorized implementation after committed design/plan and requires three self-reviews of each artifact category.

**Goal:** Retain known builtin/operator references across declaration-only module/class annotations.

**Architecture:** Keep runtime binding history unchanged for bare no-value module/class names. Preserve lexical function locals and every evaluated complex target; update the independent operator import scanner as well.

**Tech Stack:** Rust, Ruff Python AST, Cargo tests, existing Lean corpus.

**Spec:** `docs/superpowers/specs/2026-09-25-issue-562-declaration-only-design.md`

## Global constraints

- No public schema, dependency or CLI changes; function-local imports remain out of scope.
- Use `CARGO_TARGET_DIR=/private/tmp/hoimin-issue-562-target`, `CARGO_BUILD_JOBS=2`, `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`, `CARGO_INCREMENTAL=0`, `RUST_TEST_THREADS=2`.
- Record three substantive design, plan, implementation and test reviews in `docs/superpowers/reviews/2026-09-25-issue-562.md`.

## Review focus

1. Function declarations after use must still shadow the whole function.
2. Class scope nested in functions must use class semantics; methods return to function semantics.
3. Existing shadows and RHS values must not become trusted.
4. Attribute/subscript target bases and indexes can change resolution.
5. Operator imports inside compound statements must not gain unconditional trust.

## Task 1: Pin the declaration-only contract

Files: add `crates/hoimin-cli/tests/declaration_only_annotations.rs`; extend `crates/hoimin-cli/src/analyzer/rust_tests.rs`.

- [ ] Deserialize each existing Lean corpus row (`schema`, `id`, `mode`, `source`, `operator`, `pairs`) using deny_unknown_fields. Run `hoimin_cli::run_with_io` with `plan`, a temporary `subject.py`, the selected operator, and resource limits. Assert exit 0, exact sorted `(original,replacement)` pairs, and original span bytes.
- [ ] Add table-driven analyzer tests. Minimal positive fixture: `any: int\nobserved=any([])\n` expects `("any", "all")`. Negative: `def f():\n any:int\n return any([])\n` expects no collection candidate. Cover the five review-focus cases, repeated declarations and collection source/destination families in the same tests.
- [ ] Run `cargo test --offline -p hoimin-cli --test declaration_only_annotations` and `cargo test --offline -p hoimin-cli --lib declaration_only`; observe failures from missing candidates before editing production code.

## Task 2: Preserve binding identity

Files: modify `crates/hoimin-cli/src/analyzer/rust.rs` and `crates/hoimin-cli/src/analyzer/rust/operator_functions.rs`.

- [ ] In the bare valueless-name branch of `NameResolutionBuilder`, change `if tracked_resolution_name(name.id.as_str()) || function_local` to `if function_local`, and document the runtime/lexical distinction.
- [ ] Add `function_scope: bool` to `ImportScan`, initially false. Save it before `visit_stmt`, set true on FunctionDef and false on ClassDef, restore after walking. For `AnnAssign` with no value and Name target outside function scope, call `visit_annotation(&assign.annotation)` and return. Preserve generic walking for RHS and non-name targets.
- [ ] Run the two focused commands again; all cases must pass. Inspect the candidate replacement text and span assertions for strict corpus cases.

## Task 3: Review and verify

- [ ] Run `cargo fmt --all -- --check` and `git diff --check`.
- [ ] Run `cargo test --offline -p hoimin-cli --lib analyzer::` and related integration suites `operator_function_contracts`, `collection_annotation_builtins`, `lean_annotation_scope_oracle`, and `builtin_evaluation_order`.
- [ ] Run `python3 docs/audits/2026-09-15-declaration-only/verify_lean.py --output /private/tmp/hoimin-issue-562-lean` under its existing 20-second, 2-GiB guard; no model/corpus expectation changes.
- [ ] Run `cargo test --offline --workspace` with the global environment above. Record failures explicitly; distinguish infrastructure from semantic failures.
- [ ] Complete three implementation and three test self-review passes with findings/remediation. Re-run impacted tests after remediation, update checkboxes and evidence, and commit all implementation/test/review changes. Root agent performs final review and PR publication.
