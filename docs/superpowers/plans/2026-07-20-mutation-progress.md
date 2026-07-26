# Mutation Progress Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a read-only `hoimin progress` command that compares chronologically ordered JSON run reports and reports improvement, regression, stalls, and saturation.

**Architecture:** Add a focused `progress` module in `hoimin-cli`. Its input layer parses existing version-2 JSON reports, its comparison layer turns valid reports into keyed mutant sets and computes adjacent transitions, and its renderer emits either a stable JSON document or a concise human report. The root CLI dispatches `run` to the existing asynchronous loop and `progress` directly to this read-only module.

**Tech Stack:** Rust 2024 (MSRV 1.85), clap 4 derive, serde/serde_json, existing `hoimin-core` report types, cargo test.

## Global Constraints

- Accept only `hoimin run --format json` report schema version `2`; JSONL is not input.
- Report paths are chronological in their argument order; require at least two.
- Read only report files; do not run tests, create a session, or change the target project.
- Match only `path`, `original`, `replacement`, `operator`, and `symbol`.
- Equivalent-mutant detection and score exclusion are out of scope; survivor means only undetected by that run.
- Compare only adjacent usable reports. An unusable report breaks the chain and never changes saturation.
- `--patience` is nonzero and defaults to `3`.
- `timeout`, `out_of_memory`, `process_limit`, `error`, and `not_run` are inconclusive: report them, but exclude them from score and transition judgments.
- Preserve all existing `hoimin run` contracts.

---

## Planned File Structure

| Path | Responsibility |
| --- | --- |
| `crates/hoimin-cli/src/cli.rs` | Parse `progress` beside `run`. |
| `crates/hoimin-cli/src/lib.rs` | Dispatch parsed commands. |
| `crates/hoimin-cli/src/progress/input.rs` | Deserialize and classify input reports. |
| `crates/hoimin-cli/src/progress/compare.rs` | Key mutants, detect ambiguity, compare, and saturate. |
| `crates/hoimin-cli/src/progress/render.rs` | Write human and JSON output. |
| `crates/hoimin-cli/src/progress/mod.rs` | Coordinate file reads, comparison, and output. |
| `crates/hoimin-cli/tests/progress.rs` | Fixed-report unit and end-to-end tests. |
| `docs/json-schema/progress-result.schema.json` | Versioned JSON contract. |
| `README.md` | Public usage and semantics. |

### Task 1: Parse `progress`

**Files:**
- Modify: `crates/hoimin-cli/src/cli.rs`
- Modify: `crates/hoimin-cli/tests/cli_config.rs`

**Interfaces:**
- Produces `ParsedCommand::{Run(RunArgs), Progress(ProgressArgs)}`.
- Produces `ProgressArgs { reports: Vec<PathBuf>, patience: NonZeroUsize, format: ProgressOutputFormat }`.
- Task 4 consumes `ProgressArgs` through `progress::run`.

- [ ] **Step 1: Write failing CLI tests**

```rust
#[test]
fn progress_defaults_to_human_and_three_stalls() {
    let ParsedCommand::Progress(args) = parse_from([
        "hoimin", "progress", "before.json", "after.json",
    ]).unwrap() else { panic!("expected progress") };
    assert_eq!(args.patience.get(), 3);
    assert_eq!(args.format, ProgressOutputFormat::Human);
}

#[test]
fn progress_requires_two_reports_and_positive_patience() {
    assert!(parse_from(["hoimin", "progress", "one.json"]).is_err());
    assert!(parse_from(["hoimin", "progress", "--patience", "0", "a", "b"]).is_err());
}
```

- [ ] **Step 2: Run the tests and verify failure**

Run: `cargo test -p hoimin-cli --test cli_config progress_ -- --nocapture`

Expected: FAIL because clap does not yet know `progress` and the public parsed types do not exist.

- [ ] **Step 3: Implement the parsed command boundary**

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ProgressOutputFormat { Human, Json }

#[derive(Debug, Args)]
struct RawProgressArgs {
    #[arg(long, default_value_t = 3, value_parser = clap::value_parser!(NonZeroUsize))]
    patience: NonZeroUsize,
    #[arg(long, value_enum, default_value_t = ProgressOutputFormat::Human)]
    format: ProgressOutputFormat,
    #[arg(required = true, num_args = 2.., value_name = "REPORT")]
    reports: Vec<PathBuf>,
}

#[derive(Debug)]
pub enum ParsedCommand { Run(RunArgs), Progress(ProgressArgs) }
```

Change `RootCli` conversion so the existing run conversion remains unchanged inside the `Run` branch. Do not add `jsonl` to `ProgressOutputFormat`. Task 4 introduces the `run_with_io` dispatch once `progress::run` exists; do not add a temporary runtime stub in this task.

- [ ] **Step 4: Verify parser compatibility**

Run: `cargo test -p hoimin-cli --test cli_config`

Expected: PASS; existing run tests destructure `ParsedCommand::Run` and retain the same assertions.

- [ ] **Step 5: Commit**

```bash
git add crates/hoimin-cli/src/cli.rs crates/hoimin-cli/src/lib.rs crates/hoimin-cli/tests/cli_config.rs
git commit -m "feat: add progress command parsing"
```

### Task 2: Read and classify run reports

**Files:**
- Create: `crates/hoimin-cli/src/progress/mod.rs`
- Create: `crates/hoimin-cli/src/progress/input.rs`
- Create: `crates/hoimin-cli/tests/progress.rs`

**Interfaces:**
- Produces `InputReport::{Usable(UsableReport), Unusable { source, reason }}`.
- `UsableReport` holds the source path and `Vec<MutantFinished>`.
- Produces `read_report(path: &Path) -> Result<InputReport, ProgressError>` for Task 4.

- [ ] **Step 1: Write failing input tests with temporary JSON fixtures**

```rust
#[test]
fn input_marks_incomplete_reports_unusable() {
    assert!(matches!(read_report(&incomplete), Ok(InputReport::Unusable { .. })));
}

#[test]
fn input_rejects_unsupported_report_schema() {
    assert!(read_report(&unsupported).is_err());
}
```

Build fixtures with `serde_json::json!` for a complete baseline-success report, `summary.complete: false`, a nonzero baseline exit, and `REPORT_SCHEMA_VERSION + 1`.

- [ ] **Step 2: Run input tests and verify failure**

Run: `cargo test -p hoimin-cli --test progress input_ -- --nocapture`

Expected: FAIL because the progress module and input interface do not exist.

- [ ] **Step 3: Parse the existing envelope with core types**

```rust
#[derive(Deserialize)]
struct RunReportDocument {
    schema_version: u32,
    run: OutputEvent,
    baseline: Option<OutputEvent>,
    mutants: Vec<OutputEvent>,
    summary: OutputEvent,
}

pub(crate) enum UnusableReason {
    MissingBaseline, BaselineFailed, Incomplete,
}
```

Require `OutputEvent::BaselineFinished` with `ProcessTermination::Exit(0)`, `OutputEvent::RunFinished` with `complete == true`, and only `OutputEvent::MutantFinished` inside `mutants`. Malformed JSON, unreadable paths, invalid document structure, and unsupported schema versions are command errors. Syntactically valid failed-baseline and incomplete reports are normal `UnusableReason` values for output.

- [ ] **Step 4: Verify input classification**

Run: `cargo test -p hoimin-cli --test progress input_`

Expected: PASS; no unusable report is included as a usable report.

- [ ] **Step 5: Commit**

```bash
git add crates/hoimin-cli/src/progress/mod.rs crates/hoimin-cli/src/progress/input.rs crates/hoimin-cli/tests/progress.rs
git commit -m "feat: validate progress report inputs"
```

### Task 3: Compare history and detect saturation

**Files:**
- Create: `crates/hoimin-cli/src/progress/compare.rs`
- Modify: `crates/hoimin-cli/src/progress/mod.rs`
- Modify: `crates/hoimin-cli/tests/progress.rs`

**Interfaces:**
- Consumes ordered `&[InputReport]` and `NonZeroUsize` patience.
- Produces `ProgressResult { comparisons, latest, consecutive_stalls, patience }`.
- Produces `compare_reports(reports, patience) -> ProgressResult` for Task 4.

- [ ] **Step 1: Write failing transition tests**

```rust
#[test]
fn three_adjacent_stalls_are_saturated() {
    let result = compare_reports(&[killed(), killed(), killed(), killed()], nz(3));
    assert_eq!(result.consecutive_stalls, 3);
    assert_eq!(result.latest, ProgressState::Saturated);
}

#[test]
fn improvement_and_regression_break_the_stall_chain() {
    assert_eq!(compare_reports(&[survived(), killed()], nz(3)).consecutive_stalls, 0);
    let regression = compare_reports(&[killed(), killed(), survived()], nz(3));
    assert_eq!(regression.consecutive_stalls, 0);
    assert_eq!(regression.latest, ProgressState::Regressing);
}
```

Also cover added/removed mutants, duplicate keys, each inconclusive status, empty common set, `--patience 1`, and a usable/unusable/usable history that creates no cross-gap comparison.

- [ ] **Step 2: Run comparison tests and verify failure**

Run: `cargo test -p hoimin-cli --test progress compare_ -- --nocapture`

Expected: FAIL because comparison types and rules are absent.

- [ ] **Step 3: Implement exact mutant keys and aggregation**

```rust
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct MutantKey {
    path: Utf8PathBuf,
    original: String,
    replacement: String,
    operator: String,
    symbol: Option<String>,
}

fn key(candidate: &MutationCandidate) -> MutantKey { /* copy these five fields only */ }
```

Create a per-report map of unique keys and a duplicate-key set. For each immediately adjacent pair of `Usable` reports, exclude a key duplicated in either side, then count common, added, removed, and inconclusive mutants. `survived -> killed` is improvement; `killed -> survived` is regression. Common-set score is `killed / (killed + survived)` and is null if its denominator is zero.

Increment stalls only for a nonempty common set with neither improvement nor regression. Reset to zero on improvement, regression (including a comparison containing both improvement and regression), an indeterminate comparison, or a broken adjacency gap. When both transition directions occur, retain both aggregate counts but publish `Regressing`. Publish `Improving`, `Regressing`, `Stalled`, `Saturated`, or `Indeterminate`; `Saturated` requires a latest stalled comparison and `stalls >= patience`.

- [ ] **Step 4: Verify comparison behavior**

Run: `cargo test -p hoimin-cli --test progress compare_`

Expected: PASS for all transition, ambiguity, and patience cases.

- [ ] **Step 5: Commit**

```bash
git add crates/hoimin-cli/src/progress/mod.rs crates/hoimin-cli/src/progress/compare.rs crates/hoimin-cli/tests/progress.rs
git commit -m "feat: compare mutation progress history"
```

### Task 4: Render, schema, and runtime integration

**Files:**
- Create: `crates/hoimin-cli/src/progress/render.rs`
- Modify: `crates/hoimin-cli/src/progress/mod.rs`
- Modify: `crates/hoimin-cli/src/lib.rs`
- Modify: `crates/hoimin-cli/tests/progress.rs`
- Create: `docs/json-schema/progress-result.schema.json`

**Interfaces:**
- Produces `progress::run(args, stdout, stderr) -> Result<i32, ProgressError>`.
- Produces JSON schema version `1` with `latest`, `inputs`, and `comparisons`.

- [ ] **Step 1: Write failing end-to-end output tests**

```rust
#[tokio::test]
async fn progress_json_exposes_agent_decision_fields() {
    let code = hoimin_cli::run_with_io(argv, &mut stdout, &mut stderr).await;
    let value: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(code, 0);
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["latest"]["state"], "saturated");
    assert_eq!(value["latest"]["consecutive_stalls"], 3);
}
```

Add tests for readable human fields and for malformed JSON, unreadable paths, or unsupported schema returning code `2`; valid unusable reports with no usable adjacent pair must return `0` and `indeterminate`.

- [ ] **Step 2: Run output tests and verify failure**

Run: `cargo test -p hoimin-cli --test progress output_ -- --nocapture`

Expected: FAIL because no renderer writes progress output.

- [ ] **Step 3: Implement stable JSON and human renderers**

```rust
#[derive(Serialize)]
struct ProgressDocument<'a> {
    schema_version: u32,
    patience: usize,
    consecutive_stalls: usize,
    latest: LatestDecision,
    inputs: &'a [InputDisposition],
    comparisons: &'a [Comparison],
}

const PROGRESS_SCHEMA_VERSION: u32 = 1;
```

Human output must include state, score and delta when present, improvement, regression, carried survivors, added, removed, ambiguous, inconclusive, stalls, patience, and saturated. Send unusable-input reasons and ambiguity warnings to stderr. JSON contains the same fields structurally. Add `progress-result.schema.json` with closed top-level/comparison objects, five-state enum, and nonnegative integer fields. Reuse the schema-validation pattern from `crates/hoimin-cli/tests/report_handler.rs`.

In `run_with_io`, retain the existing `RunConfig` conversion and `shell::run_loop` for `ParsedCommand::Run`. Dispatch `ParsedCommand::Progress` to `progress::run`, printing any `ProgressError` to stderr and returning exit code `2`.

- [ ] **Step 4: Verify integration and schema**

Run: `cargo test -p hoimin-cli --test progress --test cli_config`

Expected: PASS; a successful progress state never changes its exit code from `0`.

- [ ] **Step 5: Commit**

```bash
git add crates/hoimin-cli/src/progress crates/hoimin-cli/src/lib.rs crates/hoimin-cli/tests/progress.rs docs/json-schema/progress-result.schema.json
git commit -m "feat: report mutation progress saturation"
```

### Task 5: Document and verify

**Files:**
- Modify: `README.md`
- Modify: `crates/hoimin-cli/tests/progress.rs`

**Interfaces:**
- Documents the final CLI and directs agents to `latest.state`, not an exit code, for progress decisions.

- [ ] **Step 1: Add an executable README-example test**

```rust
#[tokio::test]
async fn documented_progress_invocation_accepts_ordered_reports() {
    let code = hoimin_cli::run_with_io(
        ["hoimin", "progress", "--patience", "3", first, second],
        &mut stdout, &mut stderr,
    ).await;
    assert_eq!(code, 0);
}
```

- [ ] **Step 2: Run the example test**

Run: `cargo test -p hoimin-cli --test progress documented_progress_`

Expected: PASS before publishing the corresponding README command.

- [ ] **Step 3: Document the command after “Results” in `README.md`**

Add these commands and state that inputs are oldest-to-newest, patience defaults to three, comparisons require adjacent complete baseline-success reports, `saturated` means consecutive comparable stalls, JSON exposes `latest.state`, and a survivor is not proof of equivalence.

```console
hoimin progress --patience 3 reports/before.json reports/after.json reports/latest.json
hoimin progress --format json reports/*.json
```

- [ ] **Step 4: Run complete verification**

Run: `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`

Expected: all commands exit `0` without format changes, clippy diagnostics, or test failures.

- [ ] **Step 5: Commit**

```bash
git add README.md crates/hoimin-cli/tests/progress.rs
git commit -m "docs: explain mutation progress reports"
```

## Plan Self-Review

- Spec coverage: Tasks 1–4 cover the command, chronological report handling, patience, eligibility, matching key, ambiguity, statuses, transition rules, saturation, all output states, errors, and read-only behavior. Task 5 covers agent-facing documentation and complete verification.
- Placeholder scan: no TBD/TODO items or unspecified tests remain; every code task names files, interfaces, commands, and expected output.
- Type consistency: Task 1 creates `ProgressArgs`; Task 2 creates `InputReport`; Task 3 consumes it to create `ProgressResult`; Task 4 consumes all of these to render and dispatch the command.
