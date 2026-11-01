# Issue 475 range normalization implementation plan

**Goal:** Normalize accumulated line and symbol selectors once per affected file.

**Architecture:** Keep selector validation and insertion order unchanged, then canonicalize per-target vectors in one final pass before the existing postcondition.

### Task 1: Add a failing operation-count regression

**File:** `crates/hoimin-core/src/target.rs`

1. Add a private resolver statistics result used only by same-module tests.
2. Resolve one file with many sparse line ranges and symbols, assert canonical output, and require one normalization of each vector.
3. Run against the current implementation and record the repeated-call failure.

### Task 2: Defer normalization

**File:** `crates/hoimin-core/src/target.rs`

1. Remove normalization from the line and symbol insertion loops.
2. Normalize nonempty line vectors and sort/deduplicate nonempty symbol vectors once in a final `values_mut` pass.
3. Keep invalid-range and path errors at their current selector positions.
4. Run focused red-green tests and existing target-policy tests.

### Task 3: Pin boundaries and measure release behavior

**Files:** `crates/hoimin-core/src/target.rs`, existing target-policy tests if needed

1. Cover reverse, overlap, adjacency, duplicate, `u32::MAX`, multiple file, invalid range, whole-file narrowing, and repeated symbols.
2. Add an ignored five-sample release measurement for 2k/4k/8k sparse ranges that asserts output size.
3. Record environment and medians without converting elapsed time into a blocking assertion.

### Task 4: Update knowledge and evidence

**Files:** selection OKF contract, design/report indexes, issue report

1. Record the normalization boundary and complexity scope with source provenance.
2. Run YAML, reserved-file, footnote, link, hash, and Japanese prose reviews.
3. Record three concrete reviews each for OKF, design, plan, implementation, tests, and PR.

### Task 5: Verify and publish

1. Run formatting, clippy, core/contracts, workspace, Python, wheel, OKF, and diff gates with two Cargo jobs.
2. Commit only issue #475 files, push, and create a PR against `main`.
