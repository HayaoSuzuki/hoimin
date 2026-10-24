# Python 3.15 syntax implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Analyze Python 3.15 syntax using Ruff 0.17.2 while preserving Python 3.14 compatibility.
**Architecture:** Update the vendored parser from upstream, align AST dependencies at the same immutable upstream source revision, retain hoimin safety patches and adapt hoimin visitors. Treat lazy import bindings conservatively.
**Tech Stack:** Rust 1.99, CPython 3.14.9 and 3.15.1, uv, Cargo, Ruff.
**Spec:** ../specs/2026-10-24-python-315-syntax-design.md

The dependency-source follow-up is recorded in [the registry migration plan](2026-10-24-python-315-registry.md). Only the patched parser remains vendored; the completed steps below record the initial syntax implementation.

## Global constraints

- Python 3.14 and 3.15 remain supported.
- Upstream revision: `30516b42ab972ffa2569e5efcfee81135da466d3`.
- Depth 128; parser red zone 128 KiB; growth segments 1 MiB; at most 2 build jobs.
- Five self-review passes per stage, resource checks, isolated branch, PR delivery.

## Review focus

- Invalid mixed f/t-string recovery must not crash (existing depth/recovery regressions).
- Lazy aliases must not produce proven standard-library mutations (new alias tests).
- Unpacking comprehensions must retain evaluation scope and byte spans (new analyzer tests).
- A 3.14 eager import must retain existing candidates (existing corpus and new control).
- Source distributions must rebuild with pinned upstream dependencies (sdist/wheel and SBOM tests).

## Task 1: Upstream parser and AST migration

Files: Cargo.toml, Cargo.lock, crates/hoimin-python/Cargo.toml, crates/hoimin-cli/Cargo.toml, vendor/ruff_python_parser, fuzz/Cargo.toml, crates/hoimin-python/src/analyzer, crates/hoimin-python/src/depth_guard.rs.

- [x] Add parser acceptance and mutation-span regressions for lazy imports and all four unpacking forms; run `cargo test -p hoimin-python python315` and observe rejection.
- [x] Update parser sources and aligned AST dependencies from the fixed revision; retain cleanup and recursion patches; adapt changed AST interfaces.
- [x] Run the new tests, existing analysis_depth tests, and the analyzer corpus.
- [x] Review provenance, grammar, AST traversals, recovery, and old syntax in five separate passes.

## Task 2: Lazy semantics and standard-library inventory

Files: crates/hoimin-python/src/analyzer/rust.rs, related import resolvers and tests, python_stdlib_names.txt.

- [x] Add failing tests for explicit and implicit lazy alias provenance and an eager control.
- [x] Invalidate lazy bindings in proven-import resolvers; reserve the union of interpreter standard-library roots.
- [x] Run focused alias, exception, comprehension, annotation, and scope tests.
- [x] Review binding timing, declarations, scopes, exceptions, and candidate validity in five passes.

## Task 3: Distribution and release integration

Files: pyproject.toml, .github/workflows, tools/sbom.py, tests/test_sdist.py, tests/test_ci_workflow.py, tests/test_sbom.py and fixture helpers.

- [x] Update provenance/packaging test expectations before changing release integration; observe old expectations fail.
- [x] Include the complete parser source set, pin the new provenance, update Cargo package checks and fuzz wiring.
- [x] Run packaging, release/SBOM and workflow tests; build sdist and wheel and smoke the same wheel on both interpreters.
- [x] Review licenses, source inclusion, dependency identity, CI and offline validation in five passes.

## Task 4: Complete validation and PR

Files: README.md, relevant developer docs and OKF concepts, docs/maintenance/2026-10-24-python-315-syntax.md.

- [x] Run format/lint/type/workflow/docs checks and complete Rust/Python suites on both interpreters.
- [x] Update current documentation and OKF source references; catalog the design.
- [x] Review public claims, test evidence, resources, diff and PR content in five passes; obtain a fresh whole-branch review.
- [x] Clean only verified temporary targets; commit, push the dedicated branch, create and attach the PR.

Completed with [PR #1109](https://github.com/HayaoSuzuki/hoimin/pull/1109). Final validation and per-stage reviews: ../../maintenance/2026-10-24-python-315-syntax.md.
