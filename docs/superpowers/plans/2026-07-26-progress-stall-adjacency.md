# Progress Stall Adjacency Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make progress saturation depend only on an adjacent suffix of stalled comparisons.

**Architecture:** Keep the existing comparison classifier and output schema. Change only the counter transition in `compare_reports`: `Stalled` increments, while `Improving`, `Regressing`, and `Indeterminate` reset; `Saturated` remains a derived latest decision.

**Tech Stack:** Rust 2024, Cargo test harness, Clippy `all` and `pedantic` at deny, cargo-mutants 27.1.0.

## Global Constraints

- Keep the public `consecutive_stalls` and human `stalls` field names.
- Do not change candidate-ID-set mismatch or empty-common eligibility.
- Do not change mutation result classification.
- Do not redesign the progress report schema.
- Keep strict lint settings unchanged.
- Keep all design, plan, implementation, and verification documentation in this worktree.

---

## File Structure

- `crates/hoimin-cli/tests/progress.rs` — behavioral histories and output-level progress tests.
- `crates/hoimin-cli/src/progress/compare.rs` — comparison state classification and adjacent-stall counter.
- `README.md` — public definition of consecutive stall saturation.
- `docs/superpowers/specs/2026-07-26-progress-stall-adjacency-design.md` — approved design.
- `docs/superpowers/plans/2026-07-26-progress-stall-adjacency.md` — executable implementation plan.
- `docs/superpowers/reports/2026-07-26-progress-stall-adjacency.md` — focused mutation and final verification evidence.

### Task 1: Characterize the adjacency boundary

**Files:**
- Modify: `crates/hoimin-cli/tests/progress.rs`

**Interfaces:**
- Consumes: `compare_reports(&[InputReport], NonZeroUsize) -> ProgressResult`.
- Produces: explicit patience-two contracts for regression- and inconclusive-separated stalls.

- [ ] **Step 1: Replace retained-regression expectations**

Rename `compare_improvement_resets_and_regression_does_not_increment_stalls`
to `compare_improvement_and_regression_break_the_stall_chain`, and change the
regression assertions to:

```rust
let regression = compare_reports(&[killed(), killed(), survived()], nz(3));
assert_eq!(regression.consecutive_stalls, 0);
assert_eq!(regression.latest, ProgressState::Regressing);
```

In the mixed transition test, retain all comparison-count and state assertions
but change the counter assertion to:

```rust
assert_eq!(result.consecutive_stalls, 0);
```

- [ ] **Step 2: Add a regression-separated history**

Add:

```rust
#[test]
fn compare_regression_between_stalls_breaks_adjacency() {
    let result = compare_reports(
        &[killed(), killed(), survived(), survived()],
        nz(2),
    );

    assert_eq!(
        result
            .comparisons
            .iter()
            .map(|comparison| comparison.state)
            .collect::<Vec<_>>(),
        vec![
            ProgressState::Stalled,
            ProgressState::Regressing,
            ProgressState::Stalled,
        ]
    );
    assert_eq!(result.consecutive_stalls, 1);
    assert_eq!(result.latest, ProgressState::Stalled);
}
```

- [ ] **Step 3: Add a status-induced indeterminate history**

Add a same-ID-set history in which only the middle report is inconclusive:

```rust
#[test]
fn compare_inconclusive_status_between_stalls_breaks_adjacency() {
    let result = compare_reports(
        &[
            killed(),
            killed(),
            usable(vec![mutant("common", MutationStatus::Timeout)]),
            killed(),
            killed(),
        ],
        nz(2),
    );

    assert_eq!(
        result
            .comparisons
            .iter()
            .map(|comparison| comparison.state)
            .collect::<Vec<_>>(),
        vec![
            ProgressState::Stalled,
            ProgressState::Indeterminate,
            ProgressState::Indeterminate,
            ProgressState::Stalled,
        ]
    );
    assert_eq!(result.consecutive_stalls, 1);
    assert_eq!(result.latest, ProgressState::Stalled);
}
```

The fourth report returns to a conclusive status but remains indeterminate
against the inconclusive predecessor; the fifth report creates the first new
stall. This proves that neither indeterminate comparison carries old evidence.

- [ ] **Step 4: Align empty-common characterization**

Rename `compare_an_empty_common_set_does_not_change_stalls` to
`compare_an_empty_common_set_breaks_the_stall_chain` and assert:

```rust
assert_eq!(result.consecutive_stalls, 0);
assert_eq!(result.latest, ProgressState::Indeterminate);
```

This does not change empty-common eligibility; it only applies the universal
rule that an `Indeterminate` comparison breaks adjacency.

- [ ] **Step 5: Run the focused test and verify red**

Run:

```bash
cargo test -p hoimin-cli --test progress
```

Expected: the new/reset assertions fail because regression and status-induced
indeterminate states still retain the previous stall count. Existing ordinary
adjacent-stall tests remain green.

- [ ] **Step 6: Commit the failing characterization**

```bash
git add crates/hoimin-cli/tests/progress.rs
git commit -m "test: characterize progress stall adjacency"
```

### Task 2: Enforce an adjacent stalled suffix

**Files:**
- Modify: `crates/hoimin-cli/src/progress/compare.rs`
- Test: `crates/hoimin-cli/tests/progress.rs`

**Interfaces:**
- Consumes: `Comparison.state: ProgressState` from the unchanged classifier.
- Produces: `ProgressResult.consecutive_stalls` as the length of the latest adjacent stalled suffix.

- [ ] **Step 1: Remove eligibility-specific counter maintenance**

Delete:

```rust
if !eligibility.is_matching() {
    consecutive_stalls = 0;
}
```

Counter behavior must follow the resulting comparison state rather than one
particular reason for that state.

- [ ] **Step 2: Reset every non-stalled comparison**

Update the match in `compare_reports` to preserve the latest state while
resetting the suffix counter:

```rust
match comparison.state {
    ProgressState::Improving => {
        consecutive_stalls = 0;
        latest = ProgressState::Improving;
    }
    ProgressState::Regressing => {
        consecutive_stalls = 0;
        latest = ProgressState::Regressing;
    }
    ProgressState::Stalled => {
        consecutive_stalls += 1;
        latest = if consecutive_stalls >= patience.get() {
            ProgressState::Saturated
        } else {
            ProgressState::Stalled
        };
    }
    ProgressState::Indeterminate => {
        consecutive_stalls = 0;
        latest = ProgressState::Indeterminate;
    }
    ProgressState::Saturated => {
        unreachable!("individual comparisons cannot saturate");
    }
}
```

Also reset `consecutive_stalls` in the existing unusable-input branch before
continuing, so a missing adjacent comparison cannot preserve a stalled suffix:

```rust
consecutive_stalls = 0;
latest = ProgressState::Indeterminate;
continue;
```

- [ ] **Step 3: Run the focused test and verify green**

```bash
cargo test -p hoimin-cli --test progress
```

Expected: all progress integration tests pass, including the two patience-two
histories and the existing adjacent-stall/improvement cases.

- [ ] **Step 4: Run strict focused quality checks**

```bash
cargo fmt --all --check
cargo clippy -p hoimin-cli --all-targets --all-features -- -D warnings
```

Expected: both commands exit zero without adding lint exceptions.

- [ ] **Step 5: Commit the state-machine fix**

```bash
git add crates/hoimin-cli/src/progress/compare.rs
git commit -m "fix: require adjacent progress stalls"
```

### Task 3: Align the public explanation

**Files:**
- Modify: `README.md`
- Modify: `docs/superpowers/plans/2026-07-20-mutation-progress.md`

**Interfaces:**
- Consumes: the implemented adjacent-suffix policy.
- Produces: public and historical design text that no longer describes retained stalls.

- [ ] **Step 1: Clarify the README contract**

After the sentence defining `saturated`, add:

```markdown
Only immediately adjacent stalled comparisons contribute to this count.
An improving, regressing, or indeterminate comparison resets the consecutive
stall chain.
```

Do not alter the separate candidate-ID-set mismatch guidance.

- [ ] **Step 2: Correct the original implementation plan**

In `docs/superpowers/plans/2026-07-20-mutation-progress.md`, replace the
retained-regression policy with:

```markdown
Increment stalls only for a nonempty common set with neither improvement nor
regression. Reset to zero on improvement, regression (including a comparison
containing both improvement and regression), an indeterminate comparison, or
a broken adjacency gap. When both transition directions occur, retain both
aggregate counts but publish `Regressing`. Publish `Improving`, `Regressing`,
`Stalled`, `Saturated`, or `Indeterminate`; `Saturated` requires a latest
stalled comparison and `stalls >= patience`.
```

Update its regression test sketch to expect zero stalls.

- [ ] **Step 3: Verify documentation and behavior**

```bash
rg -n "consecutive|regression|indeterminate|stall" \
  README.md \
  docs/superpowers/plans/2026-07-20-mutation-progress.md \
  docs/superpowers/specs/2026-07-26-progress-stall-adjacency-design.md
cargo test -p hoimin-cli --test progress
git diff --check
```

Expected: all descriptions use adjacent suffix semantics, the focused test
passes, and no whitespace errors are reported.

- [ ] **Step 4: Commit the aligned documentation**

```bash
git add README.md docs/superpowers/plans/2026-07-20-mutation-progress.md
git commit -m "docs: clarify adjacent progress stalls"
```

### Task 4: Focused mutation and final verification

**Files:**
- Modify if needed: `crates/hoimin-cli/tests/progress.rs`
- Create: `docs/superpowers/reports/2026-07-26-progress-stall-adjacency.md`

**Interfaces:**
- Consumes: the final `compare_reports` state machine and progress integration tests.
- Produces: bounded mutation evidence, final verification evidence, and a PR-ready branch.

- [ ] **Step 1: Enumerate only the important state-machine mutants**

```bash
cargo mutants \
  --package hoimin-cli \
  --file crates/hoimin-cli/src/progress/compare.rs \
  --re "compare_reports" \
  --list
```

Expected: inventory is limited to mutations inside `compare_reports`. Record
the exact count before execution.

- [ ] **Step 2: Execute a fresh focused mutation profile**

```bash
cargo mutants \
  --package hoimin-cli \
  --jobs 4 \
  --file crates/hoimin-cli/src/progress/compare.rs \
  --re "compare_reports" \
  -- --test progress
```

Expected: every viable state-transition and patience-threshold mutant is
caught, with zero timeout. Inspect `mutants.out/missed.txt`,
`mutants.out/timeout.txt`, and `mutants.out/unviable.txt`; do not count
unviable mutants as caught.

- [ ] **Step 3: Address genuine survivors minimally**

For every missed mutant, inspect its exact diff and add the smallest behavioral
assertion to `crates/hoimin-cli/tests/progress.rs`. Do not add exclusions for
non-equivalent survivors. Re-run the individual survivor during iteration,
then repeat Step 2 fresh without `--iterate`.

- [ ] **Step 4: Record mutation evidence**

Create `docs/superpowers/reports/2026-07-26-progress-stall-adjacency.md` with:

```markdown
# Progress Stall Adjacency Implementation Report

## Scope

- Issue: #29
- Mutation target: `compare_reports`
- cargo-mutants version: `27.1.0`

## Focused Mutation Result

Copy the exact total, caught, missed, unviable, and timeout counts from the
fresh cargo-mutants result. Explain any unviable mutant by its complete name.

## Verification

- `cargo test -p hoimin-cli --test progress`: pass
- strict workspace Clippy: pass
- full workspace tests: pass
- workspace build: pass
```

Replace every bracketed value with observed evidence; no placeholders may
remain.

- [ ] **Step 5: Run fresh final verification**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features --quiet
cargo build --workspace
git diff --check
```

Expected: all commands exit zero. No lint condition or CI condition is
weakened.

- [ ] **Step 6: Commit mutation-driven tests and report**

```bash
git add crates/hoimin-cli/tests/progress.rs \
  docs/superpowers/reports/2026-07-26-progress-stall-adjacency.md
git commit -m "test: verify progress stall transitions"
```

If mutation testing required no test change, commit only the report. Do not
create an empty commit.

- [ ] **Step 7: Push and create the individual PR**

```bash
git push -u origin fix/issue-29-progress-stall-adjacency
gh pr create \
  --base main \
  --head fix/issue-29-progress-stall-adjacency \
  --title "fix: require adjacent progress stalls" \
  --body-file .superpowers/sdd/issue-29-pr-body.md
```

The PR body must include `Closes #29`, the two intervening-transition
histories, focused mutation counts, local verification, and confirmation that
strict lint conditions remain unchanged. Wait for every applicable CI job,
including Windows, before reporting completion.
