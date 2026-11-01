# Diversity-Aware Ranked Selection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an opt-in, deterministic file-round-robin policy for `verify --top N` while preserving strict saved-rank-prefix selection as the default.

**Architecture:** Keep plan ranking and manifests unchanged. Add a typed report policy, parse the user-facing CLI policy only for top-N verification, and resolve ordered candidate IDs through a pure selection module before the existing state machine runs them.

**Tech Stack:** Rust, Clap derive, Serde, Tokio integration tests, Cargo, Python unittest contract suite

## Global Constraints

- `verify PLAN --top N` without a policy must preserve the current strict saved-rank-prefix behavior exactly.
- The user-facing values are `strict` and `diverse`; reports serialize `explicit_candidates`, `strict`, or `file_round_robin_v1`.
- `--selection-policy` is optional and valid only with `--top`; explicit use with `--candidate` is rejected.
- `file_round_robin_v1` may reorder only within one equal-score tier.
- A lower-score candidate must never displace an unselected higher-score candidate.
- File order within a score tier is first occurrence in validated strict order; paths are not lexically re-sorted.
- Candidate order within a file is validated strict order.
- The plan schema, `ranking_rule_version`, candidate rank, score, reasons, and manifest order do not change.
- Selection never changes normalized limits, timeout/cancellation behavior, manifest bytes, completion state, or exit codes.
- Do not expose or reference the private proofreadtools sample.
- Do not modify or commit `.serena/`.

---

### Task 1: Add the typed verification selection policy to report contracts

**Files:**
- Modify: `crates/hoimin-core/src/report.rs`
- Modify: `crates/hoimin-core/src/machine.rs`
- Modify: `crates/hoimin-core/tests/report_policy.rs`
- Modify: `crates/hoimin-core/tests/machine.rs`
- Modify: `crates/hoimin-cli/src/report/human.rs`
- Modify: `crates/hoimin-cli/tests/report_handler.rs`
- Modify: report expectations in `crates/hoimin-cli/tests/plan.rs`

**Interfaces:**
- Consumes: existing `VerificationSelectionMode`, `VerificationSelectionScope`, and `VerificationSelection`
- Produces:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationSelectionPolicy {
    ExplicitCandidates,
    Strict,
    FileRoundRobinV1,
}
```

- Adds `pub policy: VerificationSelectionPolicy` to `VerificationSelection`
- Human output adds `policy={policy}` to the existing verification-selection line

- [ ] **Step 1: Write failing serialization and human-output tests**

In `crates/hoimin-core/tests/report_policy.rs`, extend the run-started event contract to assert:

```rust
assert_eq!(
    serde_json::to_value(VerificationSelectionPolicy::FileRoundRobinV1).unwrap(),
    serde_json::json!("file_round_robin_v1"),
);
```

In `crates/hoimin-cli/tests/report_handler.rs`, construct a run-started event whose selection uses `FileRoundRobinV1` and assert the human output contains:

```text
verification selection: mode=top policy=file_round_robin_v1 requested=3 selected=3 scope=retained_candidates plan_truncated=false
```

Update existing exact JSON selection objects in `crates/hoimin-cli/tests/plan.rs` to require:

```json
{"policy":"explicit_candidates"}
```

for candidate IDs and:

```json
{"policy":"strict"}
```

for current top-N cases.

- [ ] **Step 2: Run tests to verify RED**

Run:

```bash
cargo test -p hoimin-core --test report_policy
cargo test -p hoimin-cli --test report_handler
cargo test -p hoimin-cli --test plan verify_top_ -- --nocapture
```

Expected: compile failures because `VerificationSelectionPolicy` and the `policy` field do not exist.

- [ ] **Step 3: Implement the typed contract**

Add `VerificationSelectionPolicy` beside the other verification enums in
`report.rs`, export it through the existing `hoimin-core` public surface, and
add the required field to `VerificationSelection`.

Update every production and test constructor explicitly:

```rust
VerificationSelection {
    mode: VerificationSelectionMode::CandidateIds,
    policy: VerificationSelectionPolicy::ExplicitCandidates,
    // existing fields
}
```

Current top-N constructors use `VerificationSelectionPolicy::Strict`.

In `human.rs`, use this exact mapping:

```rust
let policy = match selection.policy {
    VerificationSelectionPolicy::ExplicitCandidates => "explicit_candidates",
    VerificationSelectionPolicy::Strict => "strict",
    VerificationSelectionPolicy::FileRoundRobinV1 => "file_round_robin_v1",
};
```

Keep `RunState::is_top_verification` based on `mode == Top`; both strict and
diverse top-N must retain the Issue #41 budget diagnostic.

- [ ] **Step 4: Run focused and core tests**

Run:

```bash
cargo test -p hoimin-core --test report_policy
cargo test -p hoimin-core --test machine
cargo test -p hoimin-cli --test report_handler
cargo test -p hoimin-cli --test plan
cargo fmt --all -- --check
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/hoimin-core/src/report.rs crates/hoimin-core/src/machine.rs crates/hoimin-core/tests/report_policy.rs crates/hoimin-core/tests/machine.rs crates/hoimin-cli/src/report/human.rs crates/hoimin-cli/tests/report_handler.rs crates/hoimin-cli/tests/plan.rs
git commit -m "feat: report verification selection policy"
```

---

### Task 2: Parse an explicit top-N selection policy

**Files:**
- Modify: `crates/hoimin-cli/src/cli.rs`
- Modify: `crates/hoimin-cli/tests/cli_config.rs`

**Interfaces:**
- Consumes: `VerificationSelectionPolicy` from Task 1
- Produces:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, clap::ValueEnum)]
pub enum TopSelectionPolicy {
    Strict,
    Diverse,
}

pub enum VerifySelection {
    CandidateIds(Vec<String>),
    Top {
        count: NonZeroUsize,
        policy: TopSelectionPolicy,
    },
}
```

- Omitted `--selection-policy` resolves to `TopSelectionPolicy::Strict` only when `--top` is selected

- [ ] **Step 1: Write failing parser tests**

Extend `verify_requires_an_explicit_selection_and_accepts_only_format_override`
in `crates/hoimin-cli/tests/cli_config.rs` with these exact cases:

```rust
let ParsedCommand::Verify(args) = parse_from([
    "hoimin", "verify", "plan.json", "--top", "30",
]).unwrap() else { panic!("expected verify") };
assert_eq!(
    args.selection,
    VerifySelection::Top {
        count: NonZeroUsize::new(30).unwrap(),
        policy: TopSelectionPolicy::Strict,
    }
);

let ParsedCommand::Verify(args) = parse_from([
    "hoimin", "verify", "plan.json", "--top", "30",
    "--selection-policy", "diverse",
]).unwrap() else { panic!("expected verify") };
assert_eq!(
    args.selection,
    VerifySelection::Top {
        count: NonZeroUsize::new(30).unwrap(),
        policy: TopSelectionPolicy::Diverse,
    }
);
```

Also assert errors for:

```rust
["hoimin", "verify", "plan.json", "--candidate", "m1", "--selection-policy", "strict"]
["hoimin", "verify", "plan.json", "--top", "3", "--selection-policy", "weighted"]
```

Assert explicit `strict` parses identically to the omitted default.

- [ ] **Step 2: Run the parser test to verify RED**

Run:

```bash
cargo test -p hoimin-cli --test cli_config verify_requires_an_explicit_selection_and_accepts_only_format_override -- --nocapture
```

Expected: compile failure because `TopSelectionPolicy` and the structured `Top` variant do not exist.

- [ ] **Step 3: Implement the CLI contract**

Add to `RawVerifyArgs`:

```rust
/// Select strict saved-rank order or equal-score file diversity for --top.
#[arg(long, value_enum, requires = "top", value_name = "POLICY")]
selection_policy: Option<TopSelectionPolicy>,
```

When `raw.top` is `Some(count)`, construct:

```rust
VerifySelection::Top {
    count,
    policy: raw.selection_policy.unwrap_or(TopSelectionPolicy::Strict),
}
```

When `raw.top` is `None`, preserve `CandidateIds`. Clap's `requires = "top"`
must reject an explicitly supplied policy with candidate IDs while allowing
candidate IDs when the option is omitted.

Update the `--top` help to mention that strict is the default. The selection
policy help must state that diverse round-robins files only within equal-score
tiers.

- [ ] **Step 4: Run focused CLI tests**

Run:

```bash
cargo test -p hoimin-cli --test cli_config
cargo fmt --all -- --check
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/hoimin-cli/src/cli.rs crates/hoimin-cli/tests/cli_config.rs
git commit -m "feat: parse top selection policy"
```

---

### Task 3: Implement deterministic strict and file-round-robin selection

**Files:**
- Create: `crates/hoimin-cli/src/plan/selection.rs`
- Create: `crates/hoimin-cli/src/plan/selection_tests.rs`
- Modify: `crates/hoimin-cli/src/plan.rs`
- Modify: `crates/hoimin-cli/tests/plan.rs`

**Interfaces:**
- Consumes: validated strict-ordered `&[RankedPlanCandidate]`, `NonZeroUsize`, and `TopSelectionPolicy`
- Produces:

```rust
pub(crate) fn select_top_candidate_ids(
    candidates: &[RankedPlanCandidate],
    count: NonZeroUsize,
    policy: TopSelectionPolicy,
) -> Vec<String>;
```

- `Strict` returns the saved rank prefix
- `Diverse` applies `file_round_robin_v1`
- `resolve_verify_selection` maps `TopSelectionPolicy` to the report's `VerificationSelectionPolicy`

- [ ] **Step 1: Add focused pure-policy fixtures**

In `selection_tests.rs`, add a helper that builds `RankedPlanCandidate` values
with explicit `id`, `path`, `rank`, and `score`. All other candidate fields must
use valid concrete values copied from existing ranking test fixtures.

Add these exact behavioral tests:

```rust
// strict: A1, A2, B1, C1, B2
assert_eq!(select_ids(strict, 4), ["A1", "A2", "B1", "C1"]);

// equal score, diverse
assert_eq!(select_ids(diverse, 5), ["A1", "B1", "C1", "A2", "B2"]);

// score 900: A1, A2; score 800: B1, C1
assert_eq!(select_ids(diverse, 3), ["A1", "A2", "B1"]);

// two files and N=5
assert_eq!(select_ids(diverse, 5), ["A1", "B1", "A2", "B2", "A3"]);

// N above retained
assert_eq!(select_ids(diverse, 30).len(), candidates.len());
```

Run the diverse selector twice and assert identical vectors. Assert the result
has the same unique ID set as the selected strict candidates when the entire
input is selected.

- [ ] **Step 2: Run the pure-policy tests to verify RED**

Run:

```bash
cargo test -p hoimin-cli plan::selection_tests -- --nocapture
```

Expected: compile failure because `selection` and `select_top_candidate_ids` do not exist.

- [ ] **Step 3: Implement the pure selector**

In `selection.rs`, implement strict with `iter().take(count.get())`.

For diverse selection, scan one contiguous score tier at a time. Within the
tier, preserve first-seen path order and per-path candidate order. Use a vector
for deterministic group order and a hash map only to find an existing vector
index:

```rust
let mut group_index = HashMap::<&Utf8Path, usize>::new();
let mut groups = Vec::<VecDeque<&RankedPlanCandidate>>::new();
```

Never iterate the hash map to produce output order. Drain one item from each
non-empty vector queue per round. Stop globally at
`min(count.get(), candidates.len())`. Do not use `BTreeMap` ordering to produce
file order. Advancing tier boundaries must compare `candidate.score`.

The selector assumes validated input and must not duplicate, mutate, or
re-rank candidates.

- [ ] **Step 4: Integrate selection into plan verification**

Change the `VerifySelection::Top` match arm in `resolve_verify_selection`:

```rust
VerifySelection::Top { count, policy } => {
    let candidate_ids =
        selection::select_top_candidate_ids(&manifest.candidates, *count, *policy);
    let report_policy = match policy {
        TopSelectionPolicy::Strict => VerificationSelectionPolicy::Strict,
        TopSelectionPolicy::Diverse => VerificationSelectionPolicy::FileRoundRobinV1,
    };
    // preserve max_mutants validation and retained scope
}
```

Explicit candidates use `VerificationSelectionPolicy::ExplicitCandidates`.
Update all `VerifySelection::Top` call sites to the structured variant with
`Strict` unless the test is explicitly about diversity.

- [ ] **Step 5: Add integration-level resolver tests**

In `crates/hoimin-cli/tests/plan.rs`, extend the existing public synthetic
project helper to write multiple production files containing the same
high-value operator shapes. Create the manifest through the real `plan`
pipeline so ranks, scores, reasons, hashes, and source records remain
semantically valid. Arrange for five equal-score candidates to appear in
strict order across paths:

```text
A1, A2, B1, C1, B2
```

Call `prepare_verify_selection` with `Diverse` and assert ordered IDs:

```text
A1, B1, C1, A2, B2
```

Assert `verification_selection.policy ==
VerificationSelectionPolicy::FileRoundRobinV1`. Add a separate unequal-score
case proving `A2` with the higher score remains before `B1` with a lower score.
Compare manifest bytes before and after preparation.

- [ ] **Step 6: Run focused and plan tests**

Run:

```bash
cargo test -p hoimin-cli plan::selection_tests
cargo test -p hoimin-cli --test plan
cargo test -p hoimin-core --test machine
cargo fmt --all -- --check
```

Expected: all pass.

- [ ] **Step 7: Commit**

```bash
git add crates/hoimin-cli/src/plan/selection.rs crates/hoimin-cli/src/plan/selection_tests.rs crates/hoimin-cli/src/plan.rs crates/hoimin-cli/tests/plan.rs
git commit -m "feat: select diverse ranked candidates"
```

---

### Task 4: Lock the end-to-end execution and documentation contracts

**Files:**
- Modify: `crates/hoimin-cli/tests/plan.rs`
- Modify: `crates/hoimin-cli/tests/run_e2e.rs`
- Modify: `README.md`

**Interfaces:**
- Consumes: `--selection-policy diverse`, `file_round_robin_v1`, ordered selected IDs
- Produces: documented user workflow and real-CLI evidence for JSON, JSONL, human, strict compatibility, and manifest immutability

- [ ] **Step 1: Write the failing README contract test**

Extend `readme_documents_agent_plan_workflow` in
`crates/hoimin-cli/tests/run_e2e.rs` to require:

```text
hoimin verify PLAN.json --top 10 --selection-policy diverse
file_round_robin_v1
equal-score
strict
```

- [ ] **Step 2: Run the documentation test to verify RED**

Run:

```bash
cargo test -p hoimin-cli --test run_e2e readme_documents_agent_plan_workflow -- --nocapture
```

Expected: failure because README does not document the diverse workflow.

- [ ] **Step 3: Add real-CLI diverse execution coverage**

Using the multi-file public synthetic project helper added in Task 3, create a
valid concentrated equal-score manifest with enough candidates in at least
three production files. Invoke:

```text
hoimin verify PLAN.json --top 5 --selection-policy diverse --format jsonl
```

Parse every non-empty stdout line as JSON. Assert:

- run-started `verification_selection.policy` is `file_round_robin_v1`;
- `mode` remains `top`;
- mutant-started or mutant-finished candidate IDs follow the expected diverse
  order;
- no lower-score candidate precedes a remaining higher-score candidate;
- stderr contains no policy warning or parse error;
- manifest bytes after verification equal the bytes before verification.

Add an equivalent default strict invocation and assert its selected IDs equal
the saved rank prefix.

- [ ] **Step 4: Document strict and diverse workflows**

In `README.md`, add:

```text
hoimin verify PLAN.json --top 10 --selection-policy diverse
```

Explain that:

- strict is the default and uses the saved rank prefix;
- diverse round-robins production files only within equal-score tiers;
- high-score tiers are exhausted before lower-score tiers;
- the report records `file_round_robin_v1`;
- verification does not rewrite the plan or change its execution limits.

Extend the README contract test in `run_e2e.rs` to require these statements and
both command examples.

- [ ] **Step 5: Run integration and documentation tests**

Run:

```bash
cargo test -p hoimin-cli --test plan
cargo test -p hoimin-cli --test run_e2e readme_documents_agent_plan_workflow
cargo test -p hoimin-cli --test report_handler
cargo test -p hoimin-core --features contracts
cargo test -p hoimin-cli --features contracts
cargo fmt --all -- --check
git diff --check
```

Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add crates/hoimin-cli/tests/plan.rs crates/hoimin-cli/tests/run_e2e.rs README.md
git commit -m "test: cover diverse top verification"
```

---

### Task 5: Verify the complete feature and request review

**Files:**
- Inspect: every file changed from `main`

**Interfaces:**
- Consumes: completed Tasks 1-4
- Produces: integration-ready Issue #42 branch with stable, Python, shuffled-order, and independent-review evidence

- [ ] **Step 1: Run formatting and static analysis**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check main...HEAD
```

Expected: exit 0.

- [ ] **Step 2: Run stable Rust and Python suites**

```bash
cargo test --workspace
uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v
```

Expected: all pass.

- [ ] **Step 3: Run randomized Rust order**

```bash
cargo +nightly-2026-07-27 test --workspace -- -Z unstable-options --shuffle
```

Expected: every test binary prints a shuffle seed and passes.

- [ ] **Step 4: Audit invariants**

Verify:

- default strict output equals the saved rank prefix;
- diverse ordering changes candidates only within equal-score tiers;
- no production path lexically sorts file groups;
- plan schema and ranking version are unchanged;
- no shell or state-machine scheduling order code changed for diversity;
- report selection policy is present in JSON, JSONL, and human output;
- release workflows are unchanged;
- `.serena/` is not committed and no private sample path appears.

- [ ] **Step 5: Request independent review**

Use `superpowers:requesting-code-review` with base `main` and final `HEAD`.
Resolve every Critical or Important finding, rerun affected tests, and request
a scoped re-review.

- [ ] **Step 6: Finish the branch**

Use `superpowers:finishing-a-development-branch`. Preserve the dedicated
worktree when push-and-PR is selected.
