# Progress Candidate-Set Eligibility Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prevent `hoimin progress` from comparing or saturating across adjacent reports with different or ambiguous candidate-ID sets.

**Architecture:** Add a private comparison-eligibility classification before the existing semantic-key comparison. Ineligible pairs remain observable as `Indeterminate`, reset only the set-comparability stall chain, and emit a focused stderr warning; eligible pairs preserve all existing semantic transition and stall-policy behavior.

**Tech Stack:** Rust 1.85, Tokio integration tests, Serde JSON report fixtures, existing `hoimin-cli` progress module.

## Global Constraints

- Work only in branch `fix/issue-25-reject-progress-set-mismatch` and its dedicated worktree.
- Preserve the existing five-field semantic transition key after candidate-ID eligibility succeeds.
- Do not change same-set regression or status-induced indeterminate stall retention; Issue #29 owns that policy.
- Keep candidate-set mismatch as exit code zero with `latest.state = indeterminate`.
- Do not change the progress JSON schema or existing stdout fields.
- Use test-driven development: observe each new behavioral test fail before changing production code.
- Mutation-test only the new eligibility/state/stall boundary; do not run the full workspace mutant inventory for this issue.

---

### Task 1: Gate Progress State on Exact Candidate-ID Sets

**Files:**
- Modify: `crates/hoimin-cli/tests/progress.rs`
- Modify: `crates/hoimin-cli/src/progress/compare.rs`

**Interfaces:**
- Consumes: `UsableReport.mutants: Vec<MutantFinished>` and each `MutationCandidate.id: String`.
- Produces: crate-visible `CandidateSetEligibility`, recomputed from adjacent
  usable inputs for renderer diagnostics without changing public result types.
- Preserves: public `compare_reports(&[InputReport], NonZeroUsize) -> ProgressResult`.

- [ ] **Step 1: Write failing mismatch and rotating-history tests**

Extend `compare_counts_added_and_removed_mutants` and add a history test:

```rust
#[test]
fn compare_counts_added_and_removed_mutants() {
    let result = compare_reports(
        &[
            usable(vec![
                mutant("common", MutationStatus::Killed),
                mutant("removed", MutationStatus::Killed),
            ]),
            usable(vec![
                mutant("common", MutationStatus::Killed),
                mutant("added", MutationStatus::Survived),
            ]),
        ],
        nz(3),
    );

    let comparison = &result.comparisons[0];
    assert_eq!(comparison.common, 1);
    assert_eq!(comparison.added, 1);
    assert_eq!(comparison.removed, 1);
    assert_eq!(comparison.ambiguous, 0);
    assert_eq!(comparison.state, ProgressState::Indeterminate);
    assert_eq!(result.consecutive_stalls, 0);
    assert_eq!(result.latest, ProgressState::Indeterminate);
}

#[test]
fn changing_candidate_id_sets_break_the_stall_chain() {
    let result = compare_reports(
        &[
            usable(vec![mutant("common", MutationStatus::Killed)]),
            usable(vec![mutant("common", MutationStatus::Killed)]),
            usable(vec![
                mutant("common", MutationStatus::Killed),
                mutant("rotated-a", MutationStatus::Killed),
            ]),
            usable(vec![
                mutant("common", MutationStatus::Killed),
                mutant("rotated-b", MutationStatus::Killed),
            ]),
            usable(vec![mutant("common", MutationStatus::Killed)]),
        ],
        nz(2),
    );

    assert!(result
        .comparisons
        .iter()
        .skip(1)
        .all(|comparison| comparison.state == ProgressState::Indeterminate));
    assert_eq!(result.consecutive_stalls, 0);
    assert_eq!(result.latest, ProgressState::Indeterminate);
}
```

- [ ] **Step 2: Run the focused tests and verify RED**

Run:

```bash
cargo test -p hoimin-cli --test progress \
  compare_counts_added_and_removed_mutants -- --exact
cargo test -p hoimin-cli --test progress \
  changing_candidate_id_sets_break_the_stall_chain -- --exact
```

Expected: both fail because mismatched sets currently yield a semantic
`Stalled` comparison and can retain or add stall evidence.

- [ ] **Step 3: Add a failing duplicate-ID eligibility test**

Add a helper that changes semantic fields while retaining the same candidate ID,
then assert the duplicate ID makes the pair indeterminate:

```rust
fn mutant_with_id(id: &str, semantic_key: &str, status: MutationStatus) -> MutantFinished {
    let mut value = mutant(semantic_key, status);
    value.candidate.id = id.to_owned();
    value
}

#[test]
fn duplicate_candidate_ids_are_ineligible() {
    let result = compare_reports(
        &[
            usable(vec![
                mutant_with_id("duplicate-id", "first", MutationStatus::Killed),
                mutant_with_id("duplicate-id", "second", MutationStatus::Killed),
            ]),
            usable(vec![
                mutant_with_id("duplicate-id", "first", MutationStatus::Killed),
                mutant_with_id("duplicate-id", "second", MutationStatus::Killed),
            ]),
        ],
        nz(1),
    );

    assert_eq!(result.comparisons[0].state, ProgressState::Indeterminate);
    assert_eq!(result.consecutive_stalls, 0);
    assert_eq!(result.latest, ProgressState::Indeterminate);
}
```

- [ ] **Step 4: Run the duplicate-ID test and verify RED**

Run:

```bash
cargo test -p hoimin-cli --test progress \
  duplicate_candidate_ids_are_ineligible -- --exact
```

Expected: FAIL because the existing semantic keys are unique and therefore
produce `Stalled` and `Saturated`.

- [ ] **Step 5: Implement the minimal eligibility gate**

In `compare.rs`, add a private classification and helper:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CandidateSetEligibility {
    Matching,
    Different,
    Duplicate,
}

impl CandidateSetEligibility {
    fn is_matching(self) -> bool {
        self == Self::Matching
    }
}

fn candidate_set_eligibility(
    previous: &UsableReport,
    current: &UsableReport,
) -> CandidateSetEligibility {
    fn ids(report: &UsableReport) -> Option<HashSet<&str>> {
        let ids = report
            .mutants
            .iter()
            .map(|mutant| mutant.candidate.id.as_str())
            .collect::<HashSet<_>>();
        (ids.len() == report.mutants.len()).then_some(ids)
    }

    let (Some(previous), Some(current)) = (ids(previous), ids(current)) else {
        return CandidateSetEligibility::Duplicate;
    };
    if previous == current {
        CandidateSetEligibility::Matching
    } else {
        CandidateSetEligibility::Different
    }
}
```

Keep the public `Comparison` and `ProgressResult` field shapes unchanged.

Add a small state-classification helper so the eligibility decision remains
independently testable and mutation-testable:

```rust
fn comparison_state(
    eligibility: CandidateSetEligibility,
    comparable_common: usize,
    regressions: usize,
    improvements: usize,
) -> ProgressState {
    if !eligibility.is_matching() || comparable_common == 0 {
        return ProgressState::Indeterminate;
    }
    if regressions > 0 {
        ProgressState::Regressing
    } else if improvements > 0 {
        ProgressState::Improving
    } else {
        ProgressState::Stalled
    }
}
```

Compute eligibility in `compare_reports` before indexing semantic keys. Pass it
to `compare_usable_reports`, preserve all existing counts, and call:

```rust
let state = comparison_state(
    candidate_set_eligibility,
    comparable_common,
    regressions,
    improvements,
);
```

This replaces only the existing terminal classification:

```rust
let state = if comparable_common == 0 {
    ProgressState::Indeterminate
} else if regressions > 0 {
    ProgressState::Regressing
} else if improvements > 0 {
    ProgressState::Improving
} else {
    ProgressState::Stalled
};
```

In `compare_reports`, reset the stall chain only for candidate-set
ineligibility before applying the existing state policy:

```rust
let eligibility = candidate_set_eligibility(previous, current);
let comparison = compare_usable_reports(previous, current, eligibility);
if !eligibility.is_matching() {
    consecutive_stalls = 0;
}
match comparison.state {
    // retain the existing arms unchanged
}
```

This deliberately leaves same-set `Indeterminate` behavior unchanged for
Issue #29.

- [ ] **Step 6: Run focused and complete progress tests and verify GREEN**

Run:

```bash
cargo test -p hoimin-cli --test progress \
  compare_counts_added_and_removed_mutants -- --exact
cargo test -p hoimin-cli --test progress \
  changing_candidate_id_sets_break_the_stall_chain -- --exact
cargo test -p hoimin-cli --test progress \
  duplicate_candidate_ids_are_ineligible -- --exact
cargo test -p hoimin-cli --test progress
```

Expected: all pass. Existing identical-ID histories retain their current
semantic comparison behavior.

- [ ] **Step 7: Commit the eligibility behavior**

```bash
git add crates/hoimin-cli/src/progress/compare.rs \
  crates/hoimin-cli/tests/progress.rs
git commit -m "fix: reject mismatched progress candidate sets"
```

---

### Task 2: Explain Ineligible Comparisons Without Changing Stdout

**Files:**
- Modify: `crates/hoimin-cli/src/progress/render.rs`
- Modify: `crates/hoimin-cli/tests/progress.rs`
- Modify: `README.md`

**Interfaces:**
- Consumes: adjacent usable `InputReport` pairs and the crate-visible
  `candidate_set_eligibility` helper.
- Produces: one stderr warning per ineligible adjacent comparison.
- Preserves: exit code zero, progress JSON schema version 1, and all existing stdout fields.

- [ ] **Step 1: Write failing stderr and stdout-compatibility tests**

Add an integration test that writes two valid JSON reports, changes the second
candidate ID without changing its semantic fields, and invokes the real CLI:

```rust
#[tokio::test]
async fn output_warns_when_candidate_id_sets_differ() {
    let fixture = tempfile::tempdir().unwrap();
    let before = valid_report();
    let mut after = valid_report();
    after["mutants"][0]["candidate"]["id"] = json!("different-id");
    let reports = vec![
        write_json(&fixture, "before.json", &before),
        write_json(&fixture, "after.json", &after),
    ];

    let (code, stdout, stderr) = run_progress(&reports, "json").await;
    let value: Value = serde_json::from_slice(&stdout).unwrap();
    let diagnostics = String::from_utf8(stderr).unwrap();

    assert_eq!(code, 0);
    assert_eq!(value["latest"]["state"], "indeterminate");
    assert_eq!(value["latest"]["consecutive_stalls"], 0);
    assert!(diagnostics.contains(
        "comparison 1 has different candidate ID sets; progress is indeterminate"
    ));
    assert!(value["comparisons"][0].get("candidate_set_match").is_none());
}
```

Add a corresponding duplicate-ID CLI fixture and assert its warning contains
`duplicate candidate IDs`.

- [ ] **Step 2: Run both output tests and verify RED**

Run:

```bash
cargo test -p hoimin-cli --test progress \
  output_warns_when_candidate_id_sets_differ -- --exact
cargo test -p hoimin-cli --test progress \
  output_warns_about_duplicate_candidate_ids -- --exact
```

Expected: FAIL because stderr currently warns only about unusable reports and
ambiguous semantic keys.

- [ ] **Step 3: Render one warning for each ineligible comparison**

Import `candidate_set_eligibility` and `CandidateSetEligibility` in `render.rs`.
Walk adjacent input pairs, skip pairs containing an unusable report, and
recompute eligibility for each usable pair. Track a separate one-based usable
comparison index so warnings align with `ProgressResult.comparisons`:

```rust
let mut comparison_index = 0;
for pair in inputs.windows(2) {
    let [InputReport::Usable(previous), InputReport::Usable(current)] = pair else {
        continue;
    };
    comparison_index += 1;
    match candidate_set_eligibility(previous, current) {
    CandidateSetEligibility::Matching => {}
    CandidateSetEligibility::Different => writeln!(
        stderr,
        "warning: comparison {} has different candidate ID sets; progress is indeterminate",
        comparison_index
    )
    .map_err(write_error)?,
    CandidateSetEligibility::Duplicate => writeln!(
        stderr,
        "warning: comparison {} has duplicate candidate IDs; progress is indeterminate",
        comparison_index
    )
    .map_err(write_error)?,
    }
}
```

Do not add an eligibility field to the public `Comparison`,
`ComparisonDocument`, human stdout, or
`docs/json-schema/progress-result.schema.json`. `ProgressResult` also retains
its existing public field shape.

- [ ] **Step 4: Clarify the README enforcement**

Replace the sentence that only instructs callers to supply identical sets with:

```markdown
Pass `hoimin progress` only reports covering the identical candidate-ID set.
The command marks a comparison `indeterminate`, resets its comparable stall
chain, and writes a warning when adjacent candidate-ID sets differ or contain
duplicates. If batch membership changes, start a new history.
```

- [ ] **Step 5: Run output, schema, and documentation tests and verify GREEN**

Run:

```bash
cargo test -p hoimin-cli --test progress \
  output_warns_when_candidate_id_sets_differ -- --exact
cargo test -p hoimin-cli --test progress \
  output_warns_about_duplicate_candidate_ids -- --exact
cargo test -p hoimin-cli --test progress \
  progress_json_document_matches_its_schema -- --exact
cargo test -p hoimin-cli --test run_e2e \
  readme_documents_agent_plan_workflow -- --exact
cargo test -p hoimin-cli --test progress
```

Expected: all pass. JSON output still validates against schema version 1, and
the warnings appear only on stderr.

- [ ] **Step 6: Commit diagnostics and documentation**

```bash
git add crates/hoimin-cli/src/progress/render.rs \
  crates/hoimin-cli/tests/progress.rs README.md
git commit -m "docs: explain progress candidate-set mismatches"
```

---

### Task 3: Mutation-Test the Critical Eligibility Boundary

**Files:**
- Test: `crates/hoimin-cli/tests/progress.rs`
- Exercise: `crates/hoimin-cli/src/progress/compare.rs`
- Local only: `mutants.out/`

**Interfaces:**
- Consumes: the passing implementation and integration tests from Tasks 1–2.
- Produces: focused evidence that tests kill mutations in ID-set eligibility, state classification, and stall-chain handling.

- [ ] **Step 1: List only the intended mutants**

Run:

```bash
cargo mutants --workspace \
  --file crates/hoimin-cli/src/progress/compare.rs \
  --re '(candidate_set_eligibility|comparison_state|compare_reports)' \
  --list
```

Expected: the list contains mutants only from the three named functions in
`compare.rs`. If the regex selects another function, narrow it before running
mutants.

- [ ] **Step 2: Remove stale local mutation output**

Run:

```bash
rm -rf mutants.out mutants.out.old
```

Expected: only ignored cargo-mutants output directories are removed. No tracked
file changes.

- [ ] **Step 3: Run the focused mutation test**

Run:

```bash
cargo mutants --workspace --jobs 4 \
  --file crates/hoimin-cli/src/progress/compare.rs \
  --re '(candidate_set_eligibility|comparison_state|compare_reports)'
```

Expected: a successful unmutated baseline and outcomes only for the listed
critical functions. Do not use `--iterate` for this acceptance run.

- [ ] **Step 4: Inspect every non-caught outcome**

Run:

```bash
test -f mutants.out/missed.txt && sed -n '1,240p' mutants.out/missed.txt
test -f mutants.out/timeout.txt && sed -n '1,240p' mutants.out/timeout.txt
test -f mutants.out/unviable.txt && sed -n '1,240p' mutants.out/unviable.txt
```

Expected: `missed.txt` and `timeout.txt` are empty. `unviable.txt` is
inconclusive and must be read, but does not represent a test-quality failure.

If a mutant survives, inspect its exact diff under `mutants.out/diff/`, add the
smallest behavior test in `progress.rs`, observe that test fail against the
mutant, restore the implementation, and rerun the focused command. Do not
broaden production scope merely to kill an equivalent mutant. Any genuinely
equivalent exception requires an exact anchored `exclude_re` plus rationale in
`.cargo/mutants.toml`; do not add a broad function-level exclusion.

- [ ] **Step 5: Assert the focused acceptance result**

Run:

```bash
test ! -s mutants.out/missed.txt
test ! -s mutants.out/timeout.txt
git status --short
```

Expected: no surviving or timed-out mutant in the important boundary, and
`mutants.out*` remains ignored. If new tests were required, only
`crates/hoimin-cli/tests/progress.rs` is modified.

- [ ] **Step 6: Commit any mutation-driven test improvement**

If Step 4 required a new test:

```bash
git add crates/hoimin-cli/tests/progress.rs
git commit -m "test: strengthen progress eligibility coverage"
```

If no mutant survived, do not create an empty commit.

---

### Task 4: Verify the Issue Boundary and Prepare the Pull Request

**Files:**
- Verify only: all files changed by Tasks 1–2

**Interfaces:**
- Consumes: committed candidate-set eligibility and diagnostics.
- Produces: a reviewed, pushable Issue #25 branch with no Issue #29 behavior changes.

- [ ] **Step 1: Run formatting and static analysis**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Expected: both exit zero with no warnings.

- [ ] **Step 2: Run the full workspace suite**

Run:

```bash
cargo test --workspace
```

Expected: all unit, integration, and documentation tests pass.

- [ ] **Step 3: Verify scope and repository state**

Run:

```bash
git diff main...HEAD --check
git diff main...HEAD --stat
git status --short
```

Expected: only the Issue #25 design, plan, progress implementation/tests,
mutation-test configuration if an exact reviewed exception was necessary, and
README are changed; the worktree is clean. Generated `mutants.out*` artifacts
remain ignored and uncommitted.

- [ ] **Step 4: Request independent review**

Review `main...HEAD` against:

- GitHub Issue #25;
- `docs/superpowers/specs/2026-07-24-progress-candidate-set-eligibility-design.md`;
- this implementation plan.

Critical and Important findings must be fixed and reverified before push.

- [ ] **Step 5: Push and create the dedicated pull request**

```bash
git push -u origin fix/issue-25-reject-progress-set-mismatch
gh pr create --repo tokyogas-tech/hoimin --base main \
  --head fix/issue-25-reject-progress-set-mismatch \
  --title "fix: reject mismatched progress candidate sets" \
  --body-file .superpowers/issue-25-pr-body.md
```

The PR body must link `Fixes #25`, summarize the exact ID-set gate and
diagnostics, and list the fresh verification commands. Keep this worktree after
opening the PR for review follow-up.
