# Rust Quality and Property Testing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enforce strict, stable Clippy quality rules across the Rust workspace and add property-based tests for fingerprint and analyzer invariants.

**Architecture:** The workspace manifest is the single source of truth for `clippy::all` and `clippy::pedantic`; both crates inherit it.  Property tests stay beside the existing integration tests: `hoimin-core` tests deterministic fingerprint canonicalization and `hoimin-cli` tests parser/analyzer safety over generated source.

**Tech Stack:** Rust 2024, Cargo workspace lints, Clippy, Proptest, GitHub Actions, Maturin, uv.

## Global Constraints

- Support Rust as configured by `workspace.package.rust-version = "1.85"`.
- Deny `clippy::all` and `clippy::pedantic`; do not enable `clippy::nursery` or `clippy::cargo` globally.
- Put every necessary lint exception on the smallest item that requires it and explain it with a comment.
- Keep the Rust-only analyzer and the wheel's runtime dependency set empty.
- Run Clippy over all workspace targets and features.

---

## File Structure

- `Cargo.toml`: workspace-owned Clippy policy.
- `crates/hoimin-core/Cargo.toml` and `crates/hoimin-cli/Cargo.toml`: inherit the workspace lint policy; CLI gains the test-only `proptest` dependency.
- `crates/hoimin-core/tests/resume_policy.rs`: property tests for fingerprint determinism and set-like canonicalization.
- `crates/hoimin-cli/src/analyzer/rust_tests.rs`: property tests for generated Python input and candidate span/order invariants.
- `.github/workflows/ci.yml`, `README.md`, `docs/development.md`: invoke the manifest-owned quality gate.

### Task 1: Enforce the Workspace Clippy Policy

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/hoimin-core/Cargo.toml`
- Modify: `crates/hoimin-cli/Cargo.toml`
- Modify: Rust files reported by `cargo clippy` under `crates/`
- Modify: `.github/workflows/ci.yml`
- Modify: `README.md`
- Modify: `docs/development.md`

**Interfaces:**
- Consumes: Cargo workspace manifest lint inheritance.
- Produces: `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes with all and pedantic enforced by the manifest.

- [ ] **Step 1: Add the workspace lint policy and crate inheritance**

```toml
[workspace.lints.clippy]
all = "deny"
pedantic = "deny"

# In each crate manifest:
[lints]
workspace = true
```

- [ ] **Step 2: Run Clippy to verify the strict gate initially fails**

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`

Expected: FAIL with concrete `clippy::pedantic` diagnostics such as integer
conversion, `map_or_else`, or lifetime-elision findings.

- [ ] **Step 3: Fix each diagnostic or add a narrow documented exception**

Use checked conversion where a conversion can truncate, prefer lazy defaults
when Clippy identifies eager computation, and elide lifetimes when this does
not obscure an external API.  For a domain-mandated conversion or an external
type constraint, place a targeted attribute immediately on the relevant item:

```rust
// Parser offsets cannot exceed the source buffer length, which is represented
// as `u32` by the public analyzer protocol.
#[allow(clippy::cast_possible_truncation)]
fn line_and_column(source: &str, offset: usize) -> (u32, u32) {
    let prefix = &source[..offset];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() as u32 + 1;
    let column = prefix
        .rsplit_once('\n')
        .map_or_else(|| prefix.chars().count(), |(_, tail)| tail.chars().count()) as u32;
    (line, column)
}
```

- [ ] **Step 4: Update all documented and CI commands to rely on the manifest policy**

```yaml
- run: cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Use the same command in `README.md` and `docs/development.md`.

- [ ] **Step 5: Run the quality gate and test suite**

Run: `cargo fmt --all -- --check; cargo clippy --workspace --all-targets --all-features -- -D warnings; cargo test --workspace`

Expected: all commands exit 0.

- [ ] **Step 6: Commit the strict quality gate**

```text
git add Cargo.toml crates .github/workflows/ci.yml README.md docs/development.md
git commit -m "build: enforce strict Clippy policy"
```

### Task 2: Add Fingerprint Property Tests

**Files:**
- Modify: `crates/hoimin-core/tests/resume_policy.rs`

**Interfaces:**
- Consumes: `hoimin_core::fingerprint(&FingerprintInput) -> RunFingerprint`.
- Produces: generated tests proving canonical order is ignored and unchanged input always yields the same fingerprint.

- [ ] **Step 1: Add a failing property test for canonical reordering**

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn fingerprint_ignores_order_of_set_like_fields(
        reverse_sources in any::<bool>(),
        reverse_targets in any::<bool>(),
        reverse_operators in any::<bool>(),
    ) {
        let original = fixture_input();
        let mut reordered = original.clone();
        if reverse_sources { reordered.sources.reverse(); }
        if reverse_targets { reordered.targets.reverse(); }
        if reverse_operators { reordered.operators.reverse(); }
        prop_assert_eq!(fingerprint(&original), fingerprint(&reordered));
    }
}
```

- [ ] **Step 2: Run the new property test**

Run: `cargo test -p hoimin-core --test resume_policy fingerprint_ignores_order_of_set_like_fields`

Expected: PASS, documenting the existing canonicalization contract over many
orders rather than a single hand-written reversal.

- [ ] **Step 3: Add a property test for deterministic fingerprint evaluation**

```rust
proptest! {
    #[test]
    fn fingerprint_is_deterministic_for_generated_operators(
        operators in prop::collection::vec("[a-z]{0,12}", 0..32),
    ) {
        let mut input = fixture_input();
        input.operators = operators;
        prop_assert_eq!(fingerprint(&input), fingerprint(&input));
    }
}
```

- [ ] **Step 4: Run the focused and full core tests**

Run: `cargo test -p hoimin-core --test resume_policy; cargo test -p hoimin-core`

Expected: all tests pass; Proptest prints no regression seed.

- [ ] **Step 5: Commit the core properties**

```text
git add crates/hoimin-core/tests/resume_policy.rs
git commit -m "test: add fingerprint property coverage"
```

### Task 3: Add Rust Analyzer Property Tests

**Files:**
- Modify: `crates/hoimin-cli/Cargo.toml`
- Modify: `crates/hoimin-cli/src/analyzer/rust_tests.rs`

**Interfaces:**
- Consumes: `analyze_source(&AnalyzeRequest<'_>, &str) -> AnalyzerOutput`.
- Produces: generated-input tests proving analysis is non-panicking and returned byte spans are valid and sorted.

- [ ] **Step 1: Add `proptest` as a CLI development dependency and write the generated-source test**

```toml
[dev-dependencies]
proptest = "1.7"
```

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn analysis_returns_ordered_in_bounds_candidates(source in ".{0,4096}") {
        let output = analyze(&source);
        let mut previous_end = 0_u64;
        for candidate in output.candidates {
            prop_assert!(candidate.span.start >= previous_end);
            prop_assert!(candidate.span.start + candidate.span.length <= source.len() as u64);
            previous_end = candidate.span.start + candidate.span.length;
        }
    }
}
```

- [ ] **Step 2: Run the test to verify the generated source space is handled**

Run: `cargo test -p hoimin-cli --bin hoimin analyzer::rust::rust_tests::analysis_returns_ordered_in_bounds_candidates`

Expected: PASS with no panic and no Proptest regression file required.

- [ ] **Step 3: Add an operator-rich source generator**

Generate combinations of known mutation tokens within valid simple statements
and assert every candidate's `original` bytes equal the source slice at its
reported span.  This distinguishes byte-span correctness from merely staying
within bounds.

```rust
prop_assert_eq!(
    candidate.original.as_bytes(),
    &source[start..end].as_bytes(),
);
```

- [ ] **Step 4: Run CLI analyzer and workspace tests**

Run: `cargo test -p hoimin-cli --bin hoimin analyzer::rust::rust_tests; cargo test --workspace`

Expected: all tests pass.

- [ ] **Step 5: Commit analyzer property coverage**

```text
git add crates/hoimin-cli/Cargo.toml crates/hoimin-cli/src/analyzer/rust_tests.rs Cargo.lock
git commit -m "test: add analyzer property coverage"
```

### Task 4: Remove Superseded Planning Artifacts and Verify Distribution

**Files:**
- Delete: `docs/superpowers/plans/2026-07-19-python-314-ty.md`
- Delete: `docs/superpowers/plans/2026-07-19-python-quality.md`
- Delete: `docs/superpowers/plans/2026-07-19-remove-python-libcst.md`
- Delete: `docs/superpowers/plans/2026-07-19-rust-analyzer-stage-1.md`
- Delete: `docs/superpowers/plans/ty-task-1-report.md`
- Create: `docs/superpowers/plans/2026-07-19-rust-quality-and-property-testing.md`

**Interfaces:**
- Consumes: committed design at `docs/superpowers/specs/2026-07-19-rust-quality-and-property-testing-design.md`.
- Produces: only the current design and plan remain as committed Superpowers documentation for this work.

- [ ] **Step 1: Verify the older untracked files describe completed work**

Run: `git status --short docs/superpowers/plans`

Expected: only historical, untracked plans/reports are listed besides this
current implementation plan.

- [ ] **Step 2: Remove only the specified superseded untracked artifacts**

Run: `Remove-Item -LiteralPath <each listed historical path>`

Expected: `.coverage` and `.idea/` remain untouched.

- [ ] **Step 3: Commit the active implementation plan**

```text
git add docs/superpowers/plans/2026-07-19-rust-quality-and-property-testing.md
git commit -m "docs: plan Rust quality improvements"
```

- [ ] **Step 4: Run full release verification**

Run: `cargo fmt --all -- --check; cargo clippy --workspace --all-targets --all-features -- -D warnings; cargo test --workspace; uv run maturin build --release; uv run --frozen python tests/wheel_smoke.py`

Expected: every command exits 0 and the built wheel has no runtime Python
dependencies.
