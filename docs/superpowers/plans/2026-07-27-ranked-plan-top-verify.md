# Ranked Plans and Top-N Verification Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rank planned mutation candidates with explainable deterministic reasons and let users execute the retained top N with `hoimin verify PLAN.json --top N`.

**Architecture:** A pure CLI-side ranking module wraps analyzer candidates in version-2 plan candidates without changing the analyzer or the shared `MutationCandidate` report contract. Verify parses legacy version-1 and ranked version-2 manifests, validates ranking integrity, and resolves an explicit-ID or top-N selection. Only the top-N path passes manifest order into the core run machine; it defers selected candidate dispatch until discovery completes and then schedules by rank, while ordinary `run` and existing explicit-ID verification preserve their current discovery-order behavior.

**Tech Stack:** Rust 2024, clap, serde/serde_json, Tokio, existing hoimin analyzer and state machine, JSON Schema, Cargo integration tests.

## Global Constraints

- `--top N` requires `N >= 1`, is mutually exclusive with `--candidate`, and neither selection may be omitted.
- Ranking scores are fixed: `explicit_line=300`, `explicit_symbol=250`, `changed_line=200`, `high_value_control=100`, `arithmetic=70`, `type_annotation=50`.
- Ranking ties break by root-relative path, line, column, operator ID, then candidate ID.
- Verify uses the saved ranking and never recomputes ranking from source or Git.
- Version-1 manifests remain valid with `--candidate` and are rejected with `--top`.
- A truncated plan's top N means top N among retained candidates, never global top N.
- Ranking does not filter candidates or claim to estimate defect probability.
- Existing `run`, exact `verify --candidate`, Linux, macOS, and Windows behavior must remain compatible.

---

## File Structure

- Create `crates/hoimin-cli/src/plan/ranking.rs`: pure ranking types, fixed reason scores, operator categories, deterministic ordering, and ranking validation.
- Create `crates/hoimin-cli/src/plan/ranking_tests.rs`: unit tests for scoring, accumulation, tie breaking, and tamper validation.
- Modify `crates/hoimin-cli/src/plan.rs`: version-2 manifest types, version-1 compatibility parser, ranking integration, selection resolution, and manifest validation.
- Modify `crates/hoimin-cli/src/cli.rs`: mutually exclusive `--candidate`/`--top` parsing and public verify selection type.
- Modify `crates/hoimin-cli/src/lib.rs`: pass the resolved ordered selection and provenance into the run loop.
- Modify `crates/hoimin-cli/src/shell.rs`: expose the ordered selected-run entry point to the core machine.
- Modify `crates/hoimin-core/src/machine.rs`: preserve manifest order for verify-only candidate filters after discovery completes.
- Modify `crates/hoimin-core/src/config.rs` and `crates/hoimin-core/src/event.rs`: carry output-only verification selection provenance into run reports.
- Modify `crates/hoimin-cli/tests/cli_config.rs`: CLI selection and help contracts.
- Modify `crates/hoimin-cli/tests/plan.rs`: manifest v2, legacy compatibility, top-N selection, tamper rejection, and end-to-end execution order.
- Modify `crates/hoimin-core/tests/machine.rs`: ordered-filter scheduling without changing ordinary streaming runs.
- Modify `crates/hoimin-cli/tests/report_handler.rs`: structured provenance output.
- Modify `docs/json-schema/run-event.schema.json` and `docs/json-schema/run-result.schema.json`: optional verification selection metadata.
- Modify `README.md`: ranked plan fields, `verify --top`, legacy behavior, and truncated-plan warning.
- Create `tests/test_ranked_plan_docs.py`: README contract for the ranked two-command workflow and its safety qualifications.

---

### Task 1: Pure Candidate Ranking

**Files:**
- Create: `crates/hoimin-cli/src/plan/ranking.rs`
- Create: `crates/hoimin-cli/src/plan/ranking_tests.rs`
- Modify: `crates/hoimin-cli/src/plan.rs`

**Interfaces:**
- Consumes: `hoimin_core::{MutationCandidate, MutationOperator, Selection, TargetSlice}`.
- Produces:

```rust
pub(crate) const RANKING_RULE_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RankingReason {
    pub code: RankingReasonCode,
    pub score: u32,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RankingReasonCode {
    ExplicitLine,
    ExplicitSymbol,
    ChangedLine,
    HighValueControl,
    Arithmetic,
    TypeAnnotation,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RankedPlanCandidate {
    #[serde(flatten)]
    pub candidate: MutationCandidate,
    pub rank: usize,
    pub score: u32,
    pub ranking_reasons: Vec<RankingReason>,
}

pub(crate) fn rank_candidates(
    selection: &Selection,
    targets: &[TargetSlice],
    candidates: Vec<MutationCandidate>,
) -> Vec<RankedPlanCandidate>;

pub(crate) fn validate_ranking(
    candidates: &[RankedPlanCandidate],
) -> Result<(), String>;
```

- [ ] **Step 1: Register the ranking module and write failing score tests**

Add to `plan.rs`:

```rust
mod ranking;
#[cfg(test)]
mod ranking_tests;

pub use ranking::{RankedPlanCandidate, RankingReason, RankingReasonCode};
```

In `plan/ranking_tests.rs`, construct candidates for explicit line, explicit symbol,
changed selection, comparison, arithmetic, and type operators. Assert exact reason
vectors and totals, including the accumulating case:

```rust
assert_eq!(
    ranked[0].ranking_reasons,
    vec![
        reason(ExplicitLine, 300),
        reason(ExplicitSymbol, 250),
        reason(ChangedLine, 200),
        reason(HighValueControl, 100),
    ]
);
assert_eq!(ranked[0].score, 850);
```

- [ ] **Step 2: Run the ranking tests to verify RED**

Run:

```console
cargo test -p hoimin-cli plan::ranking_tests -- --nocapture
```

Expected: compilation fails because `ranking` types and `rank_candidates` do not yet
exist.

- [ ] **Step 3: Implement fixed reason scoring and operator categories**

Implement pure helpers in `ranking.rs`:

```rust
fn fixed_score(code: RankingReasonCode) -> u32 {
    match code {
        RankingReasonCode::ExplicitLine => 300,
        RankingReasonCode::ExplicitSymbol => 250,
        RankingReasonCode::ChangedLine => 200,
        RankingReasonCode::HighValueControl => 100,
        RankingReasonCode::Arithmetic => 70,
        RankingReasonCode::TypeAnnotation => 50,
    }
}
```

Map every `MutationOperator` variant explicitly. Do not use string-prefix fallback.
Match explicit line selections by candidate path and inclusive line range. Match
explicit symbols against resolved `TargetSlice.path` plus `TargetSlice.symbols`, so
identically named symbols in different modules do not receive false credit.

- [ ] **Step 4: Add failing deterministic ordering and validation tests**

Create candidates tied on score and assert ordering by path, line, column, operator,
then ID. Add table tests that tamper with:

- rank zero;
- duplicate and non-contiguous ranks;
- candidate array order;
- duplicated or reordered reasons;
- reason fixed score; and
- total score.

Each case must assert the specific `validate_ranking` error fragment.

- [ ] **Step 5: Implement stable sort, rank assignment, and integrity validation**

Sort with a tuple equivalent to:

```rust
(
    Reverse(score),
    candidate.path.as_str(),
    candidate.line,
    candidate.column,
    candidate.operator.as_str(),
    candidate.id.as_str(),
)
```

Assign `rank = index + 1`. Validate reason order using the canonical reason-code
order, require exactly one operator-category reason, reject duplicate selector
reasons, and recompute the total score.

- [ ] **Step 6: Run tests and commit**

Run:

```console
cargo test -p hoimin-cli plan::ranking_tests
cargo fmt --check
cargo clippy -p hoimin-cli --all-targets -- -D warnings
```

Expected: all pass.

Commit:

```console
git add crates/hoimin-cli/src/plan.rs crates/hoimin-cli/src/plan/ranking.rs crates/hoimin-cli/src/plan/ranking_tests.rs
git commit -m "feat: rank planned mutation candidates"
```

---

### Task 2: Version-2 Plan Manifest and Legacy Compatibility

**Files:**
- Modify: `crates/hoimin-cli/src/plan.rs`
- Modify: `crates/hoimin-cli/tests/plan.rs`

**Interfaces:**
- Consumes: `rank_candidates`, `validate_ranking`, and `RankedPlanCandidate` from Task 1.
- Produces:

```rust
pub const PLAN_SCHEMA_VERSION: u32 = 2;
const LEGACY_PLAN_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanManifest {
    pub schema_version: u32,
    pub kind: String,
    pub ranking_rule_version: u32,
    pub normalized_config: PlanConfig,
    pub sources: Vec<FingerprintInputFile>,
    pub fingerprint_inputs: Vec<FingerprintInputFile>,
    pub candidates: Vec<RankedPlanCandidate>,
    pub truncated: bool,
    pub diagnostics: Vec<PlanDiagnostic>,
}

enum ParsedPlanManifest {
    Ranked(PlanManifest),
    Legacy(LegacyPlanManifest),
}
```

`LegacyPlanManifest` is private, uses `Vec<MutationCandidate>`, and otherwise mirrors
the exact version-1 serialized fields.

- [ ] **Step 1: Write failing version-2 serialization tests**

Update `create_plan_emits_versioned_manifest_without_runtime_side_effects` to assert:

```rust
assert_eq!(manifest["schema_version"], 2);
assert_eq!(manifest["ranking_rule_version"], 1);
assert_eq!(manifest["candidates"][0]["rank"], 1);
assert!(manifest["candidates"][0]["score"].is_u64());
assert!(manifest["candidates"][0]["ranking_reasons"].is_array());
```

Update the shared-discovery comparison to deserialize each candidate through
`RankedPlanCandidate` and compare its flattened `candidate` field with analyzer
discovery.

- [ ] **Step 2: Run the plan creation tests to verify RED**

Run:

```console
cargo test -p hoimin-cli --test plan create_plan_emits_versioned_manifest_without_runtime_side_effects
cargo test -p hoimin-cli --test plan plan_candidates_match_shared_discovery_for_normalized_selectors
```

Expected: assertions fail because plans still emit schema version 1 and unranked
candidates.

- [ ] **Step 3: Emit ranked version-2 plans**

In `create`, preserve `config.selection` and resolved `targets` long enough to call:

```rust
let candidates = rank_candidates(&config.selection, &targets, discovery.candidates);
```

Set `ranking_rule_version` to `RANKING_RULE_VERSION`, serialize flattened ranked
candidates, and keep the analyzer's `truncated` and diagnostics unchanged.

- [ ] **Step 4: Write failing legacy and tamper-validation tests**

Generate a version-2 fixture, convert it to version 1 by removing
`ranking_rule_version`, `rank`, `score`, and `ranking_reasons`, and assert explicit-ID
verification still prepares successfully. Add version-2 cases for every ranking
tamper from Task 1 and assert rejection happens before the marker test command runs.
Add a wrong `ranking_rule_version` case with the exact diagnostic:

```text
plan.manifest.invalid: unsupported ranking rule version 99
```

- [ ] **Step 5: Implement version-discriminated parsing and validation**

Read `schema_version` from a `serde_json::Value` before deserializing:

```rust
match value.get("schema_version").and_then(Value::as_u64) {
    Some(2) => ParsedPlanManifest::Ranked(serde_json::from_value(value)?),
    Some(1) => ParsedPlanManifest::Legacy(serde_json::from_value(value)?),
    Some(version) => return Err(unsupported(version)),
    None => return Err(missing_schema_version()),
}
```

Convert either representation into a private common view for existing source,
fingerprint, candidate ID, descriptor, and rediscovery validation. Run
`validate_ranking` only for version 2. Do not assign ranks to legacy array order.

- [ ] **Step 6: Run tests and commit**

Run:

```console
cargo test -p hoimin-cli --test plan
cargo fmt --check
cargo clippy -p hoimin-cli --all-targets -- -D warnings
```

Expected: all pass.

Commit:

```console
git add crates/hoimin-cli/src/plan.rs crates/hoimin-cli/tests/plan.rs
git commit -m "feat: version ranked plan manifests"
```

---

### Task 3: Mutually Exclusive Verify Selection

**Files:**
- Modify: `crates/hoimin-cli/src/cli.rs`
- Modify: `crates/hoimin-cli/tests/cli_config.rs`

**Interfaces:**
- Produces:

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VerifySelection {
    CandidateIds(Vec<String>),
    Top(NonZeroUsize),
}

pub struct VerifyArgs {
    pub manifest: PathBuf,
    pub selection: VerifySelection,
    pub format: OutputFormat,
}
```

- [ ] **Step 1: Write failing parser tests**

Replace the old “candidate required” test with cases that assert:

```rust
assert_eq!(
    parse_verify(["plan.json", "--top", "10"]).selection,
    VerifySelection::Top(NonZeroUsize::new(10).unwrap())
);
```

Also assert:

- repeated candidate IDs are deduplicated in first-occurrence order;
- `--top 0` fails;
- `--top 1 --candidate m1_a` fails; and
- omitting both fails.

Update help assertions to require `--top <N>`, mutual exclusion wording, retained
candidate wording, and inherited execution settings.

- [ ] **Step 2: Run parser tests to verify RED**

Run:

```console
cargo test -p hoimin-cli --test cli_config verify_
```

Expected: `--top` is unknown and `VerifyArgs` lacks `selection`.

- [ ] **Step 3: Implement clap argument group and normalized selection**

Use a required, non-multiple clap group:

```rust
#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("selection")
        .required(true)
        .multiple(false)
        .args(["candidate_ids", "top"])
))]
struct RawVerifyArgs {
    manifest: PathBuf,
    #[arg(long = "candidate", value_name = "ID")]
    candidate_ids: Vec<String>,
    #[arg(long, value_parser = clap::value_parser!(NonZeroUsize))]
    top: Option<NonZeroUsize>,
    // format unchanged
}
```

Convert the group into `VerifySelection`, retaining the current stable candidate-ID
deduplication.

- [ ] **Step 4: Run tests and commit**

Run:

```console
cargo test -p hoimin-cli --test cli_config
cargo fmt --check
cargo clippy -p hoimin-cli --all-targets -- -D warnings
```

Expected: all pass.

Commit:

```console
git add crates/hoimin-cli/src/cli.rs crates/hoimin-cli/tests/cli_config.rs
git commit -m "feat: add exclusive top-n verify selection"
```

---

### Task 4: Resolve Top-N and Preserve Manifest Execution Order

**Files:**
- Modify: `crates/hoimin-cli/src/plan.rs`
- Modify: `crates/hoimin-cli/src/lib.rs`
- Modify: `crates/hoimin-cli/src/shell.rs`
- Modify: `crates/hoimin-core/src/machine.rs`
- Modify: `crates/hoimin-cli/tests/plan.rs`
- Modify: `crates/hoimin-core/tests/machine.rs`

**Interfaces:**
- Consumes: `VerifySelection` from Task 3 and ranked/legacy parsed manifests from Task 2.
- Produces:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerifySelectionScope {
    ExplicitCandidates,
    RetainedCandidates,
}

pub struct VerifiedPlan {
    pub config: RunConfig,
    pub selection: ResolvedVerifySelection,
    pub selection_scope: VerifySelectionScope,
    pub plan_truncated: bool,
}

pub enum ResolvedVerifySelection {
    ExplicitCandidates(BTreeSet<String>),
    RankedCandidates(Vec<String>),
}

impl RunState {
    pub fn with_ordered_candidate_filter(
        run_id: String,
        config: RunConfig,
        ordered_candidate_ids: Vec<String>,
    ) -> Self;
}
```

- [ ] **Step 1: Write failing plan-selection tests**

Create a plan with at least three ranked candidates and assert:

- `--top 2` resolves exactly ranks 1 and 2 in rank order;
- `--top` equal to candidate count selects all;
- `--top` above candidate count selects all;
- explicit IDs retain the existing set-based discovery-order execution behavior;
- legacy plus explicit IDs succeeds;
- legacy plus top N fails before baseline with guidance to regenerate the plan; and
- ranked truncated plus top N succeeds with `RetainedCandidates` and
  `plan_truncated=true`.

- [ ] **Step 2: Run selection tests to verify RED**

Run:

```console
cargo test -p hoimin-cli --test plan verify_top
cargo test -p hoimin-cli --test plan legacy
```

Expected: compilation fails because `prepare_verify` does not accept
`VerifySelection` or return ordered IDs and scope.

- [ ] **Step 3: Resolve selection before project work**

Change `prepare_verify` to accept `&VerifySelection`. After header and ranking
validation but before target resolution:

```rust
let selection = match selection {
    VerifySelection::CandidateIds(ids) => ResolvedVerifySelection::ExplicitCandidates(
        validate_explicit_ids(ids, max_mutants)?,
    ),
    VerifySelection::Top(count) => ResolvedVerifySelection::RankedCandidates(
        ranked_manifest
            .ok_or_else(legacy_top_error)?
            .candidates
            .iter()
            .take(count.get())
            .map(|entry| entry.candidate.id.clone())
            .collect(),
    ),
};
```

Reject a selected count above `max_mutants` only when the actual selected count
exceeds the planned immutable limit. An oversized `--top` that selects fewer retained
candidates remains valid.

- [ ] **Step 4: Write failing core scheduling tests**

In `machine.rs` tests, feed analyzer candidates in discovery order `A, B, C`, request
ranked IDs `[C, A]`, use two workers, and assert `MutantStarted` sequence is `C, A`.
Also assert:

- no selected mutant starts before analysis finishes in ordered-filter mode;
- missing requested ID produces the existing selected-candidate error;
- ordinary unfiltered `RunState::new` still starts candidates while discovery streams;
- the existing unordered filter constructor still executes explicit-ID verification
  in analyzer discovery order.

- [ ] **Step 5: Implement ordered verify scheduling**

Store the requested vector plus an ID-to-priority map in run state. In ordered mode,
spool matching analyzer candidates and record their spool references without assigning
workers. When analysis completes:

1. verify every requested ID was discovered;
2. enqueue selected candidates in requested priority order; and
3. dispatch them through the existing worker/mutation effects.

Do not change ordinary run streaming, cancellation, resource reservation, or
not-run synthesis. Preserve each candidate's original descriptor and stable ID; only
dispatch order changes.

- [ ] **Step 6: Wire ordered IDs through CLI and shell**

Update `lib.rs` and `shell.rs` to dispatch by `ResolvedVerifySelection`: explicit IDs
call the existing set-filter path, while ranked IDs call the new ordered-filter
constructor. Ordinary run and session/resume paths retain their existing constructors.
Do not convert ranked IDs to `BTreeSet` before the core boundary.

- [ ] **Step 7: Run focused and full tests, then commit**

Run:

```console
cargo test -p hoimin-core machine
cargo test -p hoimin-cli --test plan
cargo test --workspace
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Expected: all pass, including pre-existing cancellation and resource tests.

Commit:

```console
git add crates/hoimin-cli/src/plan.rs crates/hoimin-cli/src/lib.rs crates/hoimin-cli/src/shell.rs crates/hoimin-cli/tests/plan.rs crates/hoimin-core/src/machine.rs crates/hoimin-core/tests/machine.rs
git commit -m "feat: execute planned top candidates in rank order"
```

---

### Task 5: Report Selection Provenance and Truncated Scope

**Files:**
- Modify: `crates/hoimin-core/src/config.rs`
- Modify: `crates/hoimin-core/src/event.rs`
- Modify: `crates/hoimin-cli/src/plan.rs`
- Modify: `crates/hoimin-cli/src/shell.rs`
- Modify: `crates/hoimin-cli/tests/report_handler.rs`
- Modify: `docs/json-schema/run-event.schema.json`
- Modify: `docs/json-schema/run-result.schema.json`

**Interfaces:**
- Consumes: `VerifySelectionScope` and `plan_truncated` from Task 4.
- Produces an optional output-only value:

```rust
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct VerificationSelection {
    pub mode: VerificationSelectionMode,
    pub requested: usize,
    pub selected: usize,
    pub scope: VerificationSelectionScope,
    pub plan_truncated: bool,
}
```

Modes are `candidate_ids` and `top`; scopes are `explicit_candidates` and
`retained_candidates`. Ordinary `run` reports omit this field.

- [ ] **Step 1: Write failing structured-report tests**

For `verify --top 10` on a truncated three-candidate plan, assert the run-started
event and final result contain:

```json
{
  "verification_selection": {
    "mode": "top",
    "requested": 10,
    "selected": 3,
    "scope": "retained_candidates",
    "plan_truncated": true
  }
}
```

For explicit IDs, assert `mode=candidate_ids`, requested and selected equal the stable
deduplicated count, and `scope=explicit_candidates`. Assert ordinary run output omits
the field.

- [ ] **Step 2: Run report tests to verify RED**

Run:

```console
cargo test -p hoimin-cli --test report_handler verification_selection
```

Expected: JSON assertions fail because the field is absent.

- [ ] **Step 3: Add output-only provenance without changing compatibility fingerprints**

Carry `Option<VerificationSelection>` separately from normalized plan/run
compatibility inputs. Add it to the run-started/report representation, not to
`PlanConfig`, session keys, or resume compatibility. Set it only in the verify shell
entry point from `VerifiedPlan`.

- [ ] **Step 4: Update both JSON schemas and schema tests**

Add an optional `verification_selection` definition with:

- enum-constrained mode and scope;
- integer `requested >= 1`;
- integer `selected >= 0`;
- boolean `plan_truncated`; and
- `additionalProperties: false`.

Run:

```console
cargo test -p hoimin-cli --test report_handler
```

Expected: producer and schema tests pass, and deliberate invalid fixtures fail schema
validation.

- [ ] **Step 5: Commit**

```console
git add crates/hoimin-core/src/config.rs crates/hoimin-core/src/event.rs crates/hoimin-cli/src/plan.rs crates/hoimin-cli/src/shell.rs crates/hoimin-cli/tests/report_handler.rs docs/json-schema/run-event.schema.json docs/json-schema/run-result.schema.json
git commit -m "feat: report ranked verification scope"
```

---

### Task 6: Documentation, End-to-End Contract, and Final Verification

**Files:**
- Modify: `README.md`
- Modify: `crates/hoimin-cli/tests/plan.rs`
- Modify: `crates/hoimin-cli/tests/cli_config.rs`
- Create: `tests/test_ranked_plan_docs.py`

**Interfaces:**
- Consumes: all user-facing contracts from Tasks 1–5.
- Produces: documented two-command workflow and a regression-tested public CLI.

- [ ] **Step 1: Write failing documentation contract assertions**

Create `tests/test_ranked_plan_docs.py` that reads the repository `README.md` and
asserts these exact public-contract concepts:

- `hoimin verify PLAN.json --top 10`;
- rank, score, and ranking reason explanation;
- `--candidate`/`--top` exclusivity;
- version-1 explicit-ID compatibility and top-N rejection;
- oversized top behavior; and
- the retained-subset warning for truncated plans.

Use a concrete contract test:

```python
from pathlib import Path
import unittest


class RankedPlanDocumentationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.readme = (Path(__file__).resolve().parents[1] / "README.md").read_text()

    def test_documents_ranked_two_command_workflow(self) -> None:
        for text in [
            "hoimin verify PLAN.json --top 10",
            "ranking_reasons",
            "ordering heuristic",
            "mutually exclusive",
            "version-1",
            "retained candidates",
        ]:
            self.assertIn(text, self.readme)
```

- [ ] **Step 2: Run documentation tests to verify RED**

Run:

```console
uv run --frozen python -m unittest tests.test_ranked_plan_docs -v
cargo test -p hoimin-cli --test cli_config verify_help
```

Expected: at least one required phrase or CLI help assertion fails.

- [ ] **Step 3: Update README and CLI help**

Document:

```console
hoimin plan --root . --source src --changed \
  -- python -m pytest -q > PLAN.json
hoimin verify PLAN.json --top 10 --format json
```

State explicitly that scores are transparent ordering heuristics, lower-ranked
candidates remain valid, verify never re-ranks, and truncated top N covers only
retained candidates.

- [ ] **Step 4: Run the complete local verification matrix**

Run:

```console
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
uv run --frozen python -m unittest discover -s tests -p 'test_*.py'
uv run --frozen maturin build --release
uv run --frozen python tests/wheel_smoke.py
git diff --check
```

Expected: every command exits 0; Rust tests have zero failures, Python tests report
`OK`, wheel smoke installs and runs the built wheel, and `git diff --check` is silent.

- [ ] **Step 5: Perform a real two-command smoke run**

Create a temporary Python project outside the repository with one comparison and one
arithmetic candidate. Run the documented plan command, inspect ranks/reasons, then run
`verify --top 1`. Assert only rank 1 appears in mutant lifecycle events and
`verification_selection.selected` equals 1.

Keep the temporary project and reports outside the repository and remove nothing from
the user's workspace.

- [ ] **Step 6: Commit**

```console
git add README.md crates/hoimin-cli/tests/plan.rs crates/hoimin-cli/tests/cli_config.rs tests/test_ranked_plan_docs.py
git commit -m "docs: explain ranked top-n verification"
```

- [ ] **Step 7: Push, create the feature PR, and verify CI**

Push `feat/ranked-plan-top-verify`, create a PR whose body lists the version-2
manifest compatibility rules and real smoke evidence, then monitor every required
Linux, macOS, and Windows check to completion. A skipped platform-specific hard-limit
job is acceptable only when it is the workflow's existing conditional skip.
