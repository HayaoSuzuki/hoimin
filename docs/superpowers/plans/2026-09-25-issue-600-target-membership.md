# Verify Target Membership Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. The assigned implementer works inline; do not delegate.

**Goal:** Remove candidate×target traversal during verify descriptor validation while preserving path equality and requested-ID diagnostics.

**Architecture:** Build a borrowed `HashSet<&Utf8Path>` once. Replace only the existing membership expression; retain per-file context/result sharing and candidate-ID iteration.

**Tech Stack:** Rust, camino, std collections, Tokio tests, release public CLI/API harnesses.

**Spec:** `docs/superpowers/specs/2026-09-25-issue-600-target-membership-design.md`

## Global Constraints

- Keep plan schema 4 and ranking rule 4.
- No raw-string path keys, case folding, canonicalization or filesystem identity.
- Preserve ID lookup → membership → read/context → descriptor/stable-ID error order.
- Preserve issue #456 file-level preprocessing and subsequent rediscovery.
- No runtime thresholds in CI; no added dependencies.
- Own worktree target only; `CARGO_BUILD_JOBS=2` for Cargo commands.

## Review Focus

1. Equivalent path component spellings must be accepted by membership even when raw strings differ; exercise canonical candidate paths with equivalent target spellings.
2. Case, leading dot and parent traversal are not newly normalized; assert rejected membership before file reads.
3. Multiple invalid files must produce the earliest requested-ID error; test both request orders for all five failure classes.
4. A later cached descriptor/stable-ID failure must not outrank an earlier error in another file; include interleaved valid-prefix cases.
5. Empty requests and duplicated target paths must preserve results and source-read counts; check the existing empty/unknown tests and add duplicate targets.

### Task 1: Pin membership cost and compatibility, then index targets

**Files:** Modify `crates/hoimin-cli/src/plan.rs` and `crates/hoimin-cli/src/plan/validation_tests.rs`.

**Interfaces:** Preserve `validate_requested_descriptors` and its return type `Result<BTreeSet<Utf8PathBuf>, PlanError>`. Extend test-only `ValidationStats` with `target_path_visits` and `membership_queries`.

- [ ] Add counters to existing code: increment `membership_queries` after successful candidate lookup; increment `target_path_visits` inside `targets.iter().any(...)` before `target.path == candidate.path`.
- [ ] Add the cost regression in `validation_tests.rs`: for `count in [1,4,16]` and `target_count in [1,8,64]`, create valid candidates with the existing `candidates("src/z.py", count, 0)` fixture, write its source, prepend `target_count-1` unused targets, then call `validate_requested_descriptors`. Assert `stats.target_path_visits == target_count`, `stats.membership_queries == count`, `stats.contexts == 1`, and returned paths equal `{src/z.py}`. This is the expected RED assertion for count > 1.
- [ ] Run `CARGO_BUILD_JOBS=2 cargo test -p hoimin-cli --lib target_membership --offline --locked`; save the failing assertion in `/tmp/hoimin-issue-600-validation-red.log`.
- [ ] Add equality matrix: candidate `src/calc.py`; accepted targets `src/calc.py`, `src//calc.py`, `src/./calc.py`, `src/calc.py/`; rejected targets `src/Calc.py`, `./src/calc.py`, `src/other/../calc.py`. For accepted cases assert the original candidate path and one context; rejected cases assert exact unselected message and zero contexts. Include duplicate equivalent targets.
- [ ] Add pairwise error-order matrix across unknown ID, unselected, absent selected source, bad original descriptor, and bad stable ID. Assign both the candidate IDs and their map keys `a` and `b` to control request order (the descriptor failure runs before stable-ID validation; the stable-ID case intentionally uses the changed ID); assert exact diagnostics except OS-dependent read-error suffix, whose path prefix is checked. Repeat each pair in both orders. Include a valid prefix sharing a later invalid file using existing ID-based interleaving fixture.
- [ ] Run the compatibility tests against the old scan to establish unchanged baseline semantics.
- [ ] Implement the index:

```rust
let selected_paths: HashSet<&Utf8Path> = targets.iter().map(|target| {
    #[cfg(test)]
    if let Some(stats) = stats.as_deref_mut() {
        stats.target_path_visits += 1;
    }
    target.path.as_path()
}).collect();
// At the existing check, after candidate lookup and query counter:
if !selected_paths.contains(candidate.path.as_path()) {
    // Keep the existing error unchanged.
}
```

- [ ] Run `CARGO_BUILD_JOBS=2 cargo test -p hoimin-cli --lib plan:: --offline --locked` and `CARGO_BUILD_JOBS=2 cargo test -p hoimin-cli --test plan --offline --locked`; retain GREEN evidence.
- [ ] Review implementation three separate times: ordering/equality, lifetime/allocation/cost, and final complete diff. Review tests three separate times: independent expected outcomes, counter sensitivity and path edges, then cross-file and empty/duplicate coverage. Fix actual findings and record each pass in the report.
- [ ] Commit implementation and tests after required verification.

### Task 2: Observe public performance and publish evidence

**Files:** Create `docs/superpowers/reports/2026-09-25-issue-600-target-membership-review.md`; update `docs/knowledge/design/selection-plan-verify.md`, `docs/knowledge/references/design-documents.md`, and `docs/knowledge/references/audit-documents.md`.

**Interfaces:** Existing `/tmp/hoimin-perf-audit` fixtures and public `prepare_verify_selection`/CLI only; new outputs under `/tmp/hoimin-issue-600-benchmark`.

- [ ] Build with `CARGO_BUILD_JOBS=2 cargo build --release -p hoimin-cli --offline --locked`.
- [ ] Adapt the existing public preparation harness only to link this worktree's library and save outputs into the new directory. Reuse all six old plans, three alternating repetitions, `TopSelectionPolicy::Strict`, count 10,000. Assert every call succeeds.
- [ ] Run public `verify PLAN --top 10000` for the same alternating repetitions. Assert exit 3, baseline `{"Exit":1}`, zero mutants, and selected count 10,000. Save JSON/stderr, elapsed values, platform, Rust version and SHA-256 of the binary. Compare medians with historical observations without claiming controlled before/after ratios.
- [ ] Run `cargo fmt --check`, `CARGO_BUILD_JOBS=2 cargo test --workspace --offline --locked`, `CARGO_BUILD_JOBS=2 cargo test -p hoimin-core --features contracts --offline --locked`, and `CARGO_BUILD_JOBS=2 cargo test -p hoimin-cli --features contracts --offline --locked` with logs in `/tmp`. Investigate failures rather than counting them as passes.
- [ ] Record all 12 review passes and RED/GREEN evidence, measured operation counts, benchmark results and limitations. Do not infer Linux/Windows runtime guarantees from macOS.
- [ ] Add source records (revision/status/hash), matching footnotes and source-list entries for design/report; update the existing selection concept rather than creating a duplicate topic.
- [ ] Validate OKF YAML/reserved-file structure, local links, source IDs/footnotes, new hashes and index coverage. Commit documentation, send root commits/results; root owns push and PR.

## Plan self-review

1. Mapped all issue acceptance criteria to Task 1 or Task 2. Found performance-only tests would miss early failures, so added both-order failure pairs and source-read counts; retained public plan/rediscovery regression suite.
2. Checked test fixture and production types against existing code. Found that assigning map keys independently of candidate IDs would panic when retrieving cached validation results. Corrected the matrix to change both IDs and keys for invalid cases, retaining real stable IDs for the valid-prefix interleaving case.
3. Checked reproducibility and scope: isolated outputs preserve historical measurements; all Cargo calls use own target and bounded jobs. Clarified that counters cover actual traversal plus query calls, not hashing internals. Baseline compatibility tests run before index replacement; the failing cost gate supplies RED evidence.
