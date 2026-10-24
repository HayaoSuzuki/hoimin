# Python 3.15 registry dependencies Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Keep the patched parser vendored and replace eight unmodified Ruff snapshots with crates.io dependencies.

**Architecture:** Pin direct Ruff dependencies to =0.0.19. Retain parser source and safety fixes byte-for-byte; update distribution and SBOM provenance to describe the resulting sources.

**Tech Stack:** Cargo, Maturin, pytest, CPython 3.14.9/3.15.1.

**Spec:** `docs/superpowers/specs/2026-10-24-python-315-syntax-design.md`

## Global Constraints

- Preserve Python >=3.14,<3.16, every parser safety patch, and existing mutation behavior.
- Keep the dedicated python-315-syntax branch/worktree and update PR #1109.
- At least five self-review passes at each stage; two build jobs, incremental disabled, reuse caches.
- No global Git/Windows setting changes or Git dependencies.

## Review Focus

- AST/parser type identity: exactly one aligned version in both locked graphs.
- Source archive excludes removed vendor directories but retains patched parser bytes.
- Registry SBOM components retain their package identities and captured metadata.
- Fuzz lock and dependency policy continue to resolve without relaxed rules.
- The rebuilt wheel behaves on both supported Python versions.

## Task 1: Dependency and provenance migration

**Files:** Cargo.toml, Cargo.lock, fuzz/Cargo.lock, vendor/, pyproject.toml, tools/sbom.py, tests/test_sbom.py, tests/test_sdist.py.

**Interfaces:** Consumes existing patched parser; produces a locked graph with only the parser as a local Ruff crate.

- [x] Update the SBOM test to assert that all eight external identities remain unchanged; add the archive exclusion assertion.
- [x] Run the affected tests and observe the expected failures.
- [x] Pin direct dependencies, remove eight vendor snapshots and their archive entries, and restrict SBOM vendor rewriting to the parser.
- [x] Refresh both lockfiles without unrelated upgrades; run metadata, archive and SBOM tests, formatting and Clippy.
- [x] Record five review passes and commit the dependency migration.

## Task 2: Distribution, regression and PR update

**Files:** vendor/ruff_python_parser/README.hoimin.md, documentation/provenance references, maintenance record, PR body.

**Interfaces:** Consumes Task 1 graph; produces verified distributions and accurate source records.

- [x] Update current documentation and retain prior validation as historical evidence.
- [x] Run complete Rust and Python suites on 3.14.9 and 3.15.1.
- [x] Build an sdist and a locked wheel from its extracted source; smoke the same wheel with both interpreters.
- [x] Run one fresh review, fix findings, check documentation and source pins, and record five review passes.
- [x] Clean only obsolete task outputs, commit, push, update PR #1109, and inspect CI.

## Plan self-review

1. The approved eight-crate reduction and retained parser safety fixes are covered by Task 1.
2. Each task has explicit inputs, outputs, files and verifiable completion steps.
3. The SBOM test preserves captured external metadata rather than manufacturing vendor provenance.
4. The five failure modes above have metadata, archive, SBOM, policy or runtime checks.
5. Two tasks keep implementation and end-to-end evidence together without changing runtime semantics.
