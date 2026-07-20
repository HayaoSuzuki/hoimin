# Agent Plan and Candidate Verification Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a read-only JSON `plan` command and a manifest-driven `verify` command that lets an AI agent inspect candidates before testing and later rerun only explicitly chosen, unchanged candidates.

**Architecture:** Build a versioned plan manifest outside the run-report family, using the same target resolver, analyzer candidate construction, profile, selectors, and explicit fingerprint input resolver as `run`. Before `verify` starts a baseline or creates a worker, validate the manifest structure, target/input hashes, and every requested candidate against current source; then pass a candidate-ID filter into the existing state machine so reporting, workspace isolation, resource limits, and exit policies remain unchanged.

**Tech Stack:** Rust 2024 (MSRV 1.85), `clap`, `camino`, `serde_json`, `blake3`, existing hoimin core state machine and workspace/process/report handlers.

## Global Constraints

- Implement this plan after `2026-07-21-fingerprint-include-implementation.md`; it consumes `FingerprintInputFile`, `fingerprint_inputs::resolve`, and fingerprint schema 4.
- `hoimin plan` accepts target/copy/operator/profile/safety/test-argv and `--fingerprint-include`, but rejects `--format`, `--session`, and `--resume` by not declaring them.
- `plan` never starts a test process, creates a workspace worker, creates a SQLite database, or writes to the source tree; it emits exactly one JSON document to stdout.
- A candidate limit emits a partial manifest with `truncated: true`, mandatory `candidate_limit` diagnostic, and exit 4. Structural/analysis/input failures emit no manifest and exit 2.
- `verify PLAN.json --candidate ID...` requires one or more IDs, deduplicates them after parsing, permits only `--format` as an override, and always runs a fresh baseline without session/resume.
- Validate manifest schema/kind/paths, exact target source and explicit-input path/hash sets, and descriptors before baseline; report source/input/descriptor failures as `plan.source.changed`, `plan.fingerprint_input.changed`, and `plan.candidate.invalid` respectively.
- Do not relocate candidates, infer missing candidates, add a GUI/TUI/apply operation, alter run-event/run-result schema version 2, or make an untrusted manifest safe to execute.

---

## File Structure

- Modify: `crates/hoimin-core/src/config.rs` — add a serializable `PlanConfig` that represents normalized run configuration without session/resume and exact conversions to/from `RunConfig`.
- Modify: `crates/hoimin-core/src/machine.rs` — add a `RunState::with_candidate_filter` constructor and skip unselected analyzed candidates without changing worker/report behavior.
- Modify: `crates/hoimin-core/tests/machine.rs` — prove the filter schedules only requested IDs in source sequence order.
- Modify: `crates/hoimin-cli/src/cli.rs` and `crates/hoimin-cli/src/lib.rs` — parse/dispatch `Plan` and `Verify`, share applicable run options, and enforce command-specific option sets.
- Modify: `crates/hoimin-cli/src/analyzer/mod.rs` — expose a shared in-memory discovery function that uses the same descriptor construction, ordering, deduplication, profile, and global candidate limit as the runtime analyzer.
- Create: `crates/hoimin-cli/src/plan.rs` — version-1 manifest types, JSON encode/decode, planning, pre-baseline validation, and verify preparation.
- Modify: `crates/hoimin-cli/src/shell.rs` — run a verified candidate-ID set through the existing loop with no session handler.
- Modify: `crates/hoimin-cli/tests/cli_config.rs`, `crates/hoimin-cli/tests/analyzer_handler.rs`, and `crates/hoimin-cli/tests/run_e2e.rs`; create `crates/hoimin-cli/tests/plan.rs` — parser, shared discovery, manifest, no-side-effect, and end-to-end coverage.
- Modify: `README.md` — document the agent workflow and bounded semantics.

### Task 1: Introduce a plan-safe normalized config contract

**Files:**
- Modify: `crates/hoimin-core/src/config.rs:229-466`
- Create: `crates/hoimin-core/tests/plan_config.rs`

**Interfaces:**
- Produces `PlanConfig`, `RunConfig::into_plan_config(self) -> PlanConfig`, and `PlanConfig::into_run_config(self, output: OutputConfig) -> RunConfig`.
- `PlanConfig` serializes all normalized run semantics except `session` and `resume`, including resolved `fingerprint_inputs`.

- [ ] **Step 1: Write a serialization round-trip test that forbids session/resume leakage**

Create `plan_config.rs` with:

```rust
#[test]
fn plan_config_round_trips_run_semantics_without_session_or_resume() {
    let mut config = fixture_run_config();
    config.session = Some(SessionConfig { path: "state.sqlite".into() });
    config.resume = true;
    let value = serde_json::to_value(config.into_plan_config()).unwrap();
    assert!(value.get("session").is_none());
    assert!(value.get("resume").is_none());

    let restored = serde_json::from_value::<PlanConfig>(value).unwrap()
        .into_run_config(OutputConfig { format: OutputFormat::Json });
    assert_eq!(restored.session, None);
    assert!(!restored.resume);
}
```

Use a fixture that contains a selector, operator set, profile, command argv, limits, `fingerprint_includes`, and one resolved `FingerprintInputFile`.

- [ ] **Step 2: Run the test to verify the type is absent**

Run: `uv run cargo test -p hoimin-core --test plan_config`

Expected: compilation fails because `PlanConfig` and the conversions do not exist.

- [ ] **Step 3: Define the exact plan config and conversion boundary**

In `config.rs`, introduce this field set (all types already exist):

```rust
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanConfig {
    pub root: Utf8PathBuf,
    pub selection: Selection,
    pub limits: RunLimits,
    pub test_argv: Vec<CommandArg>,
    pub output: OutputConfig,
    pub operators: MutationOperatorSelection,
    pub allow_best_effort_memory: bool,
    pub profile: MutationProfile,
    pub fingerprint_includes: Vec<String>,
    pub fingerprint_inputs: Vec<FingerprintInputFile>,
}
```

`into_plan_config` copies every listed field and deliberately drops session/resume. `into_run_config` restores the fields, sets `session: None`, `resume: false`, and overwrites only `output` with its parameter. This makes output format the sole verify-time override.

- [ ] **Step 4: Run core configuration regressions**

Run: `uv run cargo test -p hoimin-core --test plan_config && uv run cargo test -p hoimin-core --test resume_policy`

Expected: PASS.

- [ ] **Step 5: Commit the plan config contract**

```bash
git add crates/hoimin-core/src/config.rs crates/hoimin-core/tests/plan_config.rs
git commit -m "feat: add plan configuration contract"
```

### Task 2: Share candidate discovery between planning and runtime analysis

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/mod.rs:1-167`
- Modify: `crates/hoimin-cli/tests/analyzer_handler.rs:1-520`

**Interfaces:**
- Produces `discover_targets(root, targets, operators, profile, max_candidates) -> Result<Discovery, EffectFailed>`.
- `Discovery { candidates: Vec<MutationCandidate>, diagnostics: Vec<AnalyzerDiagnostic>, truncated: bool }` uses the same descriptor construction and global sequence numbering as `AnalyzerHandler`.

The extracted return type is:

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Discovery {
    pub candidates: Vec<MutationCandidate>,
    pub diagnostics: Vec<AnalyzerDiagnostic>,
    pub truncated: bool,
}
```

- [ ] **Step 1: Write a parity test before extracting discovery**

Build two Python files with candidates and assert the new in-memory result has the exact same IDs, sequences, and descriptors as records replayed from the existing `AnalyzerHandler` spool:

```rust
#[tokio::test]
async fn in_memory_discovery_matches_runtime_candidate_descriptors() {
    let (root, targets, operators) = two_target_fixture();
    let planned = discover_targets(&root, &targets, &operators, MutationProfile::Focused, 100).await.unwrap();
    let runtime = replay_runtime_candidates(root, targets, operators, MutationProfile::Focused).await;
    assert_eq!(planned.candidates, runtime);
    assert!(!planned.truncated);
}
```

- [ ] **Step 2: Run the parity test to verify it fails**

Run: `uv run cargo test -p hoimin-cli --test analyzer_handler in_memory_discovery_matches_runtime_candidate_descriptors -- --exact`

Expected: compilation fails because `discover_targets` does not exist.

- [ ] **Step 3: Extract shared descriptor construction and global limiting**

Refactor `accept_candidate` into a pure builder and use it from both paths:

```rust
fn mutation_candidate(source: &[u8], candidate: AnalyzerCandidate, sequence: u64) -> Result<MutationCandidate, EffectFailed> {
    let descriptor = CandidateDescriptor {
        schema_version: CANDIDATE_SCHEMA_VERSION,
        path: candidate.path,
        span: candidate.span,
        original: candidate.original,
        replacement: candidate.replacement,
        operator: candidate.operator,
        line: candidate.line,
        column: candidate.column,
        symbol: candidate.symbol,
        file_hash: blake3::hash(source).to_hex().to_string(),
    };
    let id = validate_candidate(source, &descriptor)
        .map_err(|error| EffectFailed::other(EffectId(0), "analyzer.candidate", error.to_string()))?;
    Ok(MutationCandidate {
        id: id.to_string(),
        sequence,
        path: descriptor.path,
        span: descriptor.span,
        original: descriptor.original,
        replacement: descriptor.replacement,
        operator: descriptor.operator,
        line: descriptor.line,
        column: descriptor.column,
        symbol: descriptor.symbol,
        file_hash: descriptor.file_hash,
    })
}
```

Implement `discover_targets` by reading each `TargetSlice` from `root`, decoding UTF-8, calling existing `rust::analyze_source` with the **remaining global capacity**, converting candidates with sequence `candidates.len() + 1`, and stopping at the first candidate-limit diagnostic. Preserve target resolver order, candidate sort/dedup/profile behavior in `rust::analyze_source`, and diagnostics in encounter order. `AnalyzerHandler::handle_with_cancellation` must use the same builder and limit calculation, retaining its candidate spool protocol.

When a target reaches the global candidate limit before all targets have been examined, return `Discovery { truncated: true, .. }` rather than an infrastructure error. In the runtime handler, finish and return the accumulated spool whenever that happens, even when `request.final_target == false`; this gives a later filtered verification the exact prefix represented by a truncated plan. The ordinary state machine continues to stop a normal run at `truncated`, so its public behavior remains exit 4.

- [ ] **Step 4: Run analyzer tests**

Run: `uv run cargo test -p hoimin-cli --test analyzer_handler && uv run cargo test -p hoimin-cli analyzer::rust_tests`

Expected: PASS. The focused-profile candidate sequence remains unchanged.

- [ ] **Step 5: Commit shared discovery**

```bash
git add crates/hoimin-cli/src/analyzer/mod.rs crates/hoimin-cli/tests/analyzer_handler.rs
git commit -m "refactor: share candidate discovery with planning"
```

### Task 3: Add `plan` and `verify` command parsing with strictly separate options

**Files:**
- Modify: `crates/hoimin-cli/src/cli.rs:15-470`
- Modify: `crates/hoimin-cli/tests/cli_config.rs:1-370`

**Interfaces:**
- Produces `ParsedCommand::{Plan(PlanArgs), Verify(VerifyArgs)}`.
- `PlanArgs` converts to a `RunConfig` with JSON output, no session, no resume; `VerifyArgs { manifest: PathBuf, candidate_ids: Vec<String>, format: OutputFormat }` does not accept target/test/copy/session options.

- [ ] **Step 1: Add parser tests for accepted and rejected command shapes**

Add these exact cases:

```rust
#[test]
fn plan_accepts_run_selection_but_rejects_report_and_session_options() {
    assert!(matches!(parse_from(["hoimin", "plan", "--file", "src/calc.py", "--fingerprint-include", "pyproject.toml", "--", "python", "-m", "pytest"]), Ok(ParsedCommand::Plan(_))));
    for option in ["--format", "--session", "--resume"] {
        assert!(parse_from(["hoimin", "plan", "--file", "src/calc.py", option, "x", "--", "python"]).is_err(), "{option}");
    }
}

#[test]
fn verify_requires_candidates_and_accepts_only_format_override() {
    assert!(parse_from(["hoimin", "verify", "plan.json", "--format", "jsonl", "--candidate", "m1_a"]).is_ok());
    assert!(parse_from(["hoimin", "verify", "plan.json"]).is_err());
    assert!(parse_from(["hoimin", "verify", "plan.json", "--candidate", "m1_a", "--source", "src"]).is_err());
}
```

- [ ] **Step 2: Run parser tests to verify they fail**

Run: `uv run cargo test -p hoimin-cli --test cli_config plan_accepts_run_selection_but_rejects_report_and_session_options -- --exact`

Expected: FAIL because `plan` is not a recognized command.

- [ ] **Step 3: Refactor shared run fields and add command-specific wrappers**

Extract all `RawRunArgs` fields except `format`, `session`, and `resume` into `RawMutationArgs: clap::Args`. Embed it with `#[command(flatten)]` in both `RawRunArgs` and `RawPlanArgs`. Keep `RawRunArgs`' current output/session fields unchanged. Define:

```rust
#[derive(Debug, Args)]
struct RawPlanArgs {
    #[command(flatten)] mutation: RawMutationArgs,
    #[arg(last = true, num_args = 1.., value_name = "TEST_ARGV")]
    test_argv: Vec<OsString>,
}

#[derive(Debug, Args)]
struct RawVerifyArgs {
    #[arg(value_name = "PLAN")]
    manifest: PathBuf,
    #[arg(long = "candidate", required = true, value_name = "ID")]
    candidate_ids: Vec<String>,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    format: OutputFormat,
}
```

Use one conversion from `RawMutationArgs + test_argv + output/session/resume` to `RawRunConfig`, so plan and run cannot drift in selector/operator/profile/limit semantics. `PlanArgs` must call it with `OutputFormat::Json`, `None`, and `false`.

- [ ] **Step 4: Run all CLI configuration tests**

Run: `uv run cargo test -p hoimin-cli --test cli_config`

Expected: PASS. Existing `run`, `progress`, `--profile focused`, and non-UTF8 test argv tests remain unchanged.

- [ ] **Step 5: Commit command parsing**

```bash
git add crates/hoimin-cli/src/cli.rs crates/hoimin-cli/tests/cli_config.rs
git commit -m "feat: parse plan and verify commands"
```

### Task 4: Create and serialize version-1 plan manifests without runtime side effects

**Files:**
- Create: `crates/hoimin-cli/src/plan.rs`
- Modify: `crates/hoimin-cli/src/lib.rs:1-68`
- Create: `crates/hoimin-cli/tests/plan.rs`

**Interfaces:**
- Produces `PlanManifest`, `PlanDiagnostic`, and `create(config: RunConfig) -> Result<PlanOutput, PlanError>`.
- `PlanOutput { manifest: PlanManifest, exit_code: i32 }` has exit 4 exactly when `manifest.truncated`.

- [ ] **Step 1: Write plan-only tests before the module exists**

Create tests that invoke `run_with_io` for a small project and assert all of the following:

```rust
assert_eq!(code, 0);
let manifest: serde_json::Value = serde_json::from_slice(&stdout.bytes()).unwrap();
assert_eq!(manifest["schema_version"], 1);
assert_eq!(manifest["kind"], "plan");
assert!(manifest["sources"].as_array().unwrap().iter().any(|s| s["path"] == "src/calc.py"));
assert!(manifest["candidates"].as_array().unwrap().iter().all(|c| c["id"].as_str().unwrap().starts_with("m1_")));
assert!(!workspace_marker.exists());
assert!(!session_path.exists());
```

Use a test argv that would create `workspace_marker` if it ran; assert the marker stays absent. Add focused/operator/line/symbol cases by comparing `manifest.candidates` with `discover_targets` for the same config. Add a candidate-limit case that asserts stdout parses as manifest, `truncated == true`, a `candidate_limit` diagnostic exists, and exit code is 4.

- [ ] **Step 2: Run the plan tests to verify dispatch is missing**

Run: `uv run cargo test -p hoimin-cli --test plan create_plan -- --nocapture`

Expected: FAIL because `ParsedCommand::Plan` is not dispatched in `run_with_io`.

- [ ] **Step 3: Define the manifest data contract and creator**

In `plan.rs`, declare a strict manifest and diagnostics:

```rust
pub const PLAN_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanManifest {
    pub schema_version: u32,
    pub kind: String,
    pub normalized_config: PlanConfig,
    pub sources: Vec<FingerprintInputFile>,
    pub fingerprint_inputs: Vec<FingerprintInputFile>,
    pub candidates: Vec<MutationCandidate>,
    pub truncated: bool,
    pub diagnostics: Vec<PlanDiagnostic>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanDiagnostic {
    pub code: String,
    pub path: Option<Utf8PathBuf>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub message: String,
}
```

`create` must call `shell::prepare_run_config`, `TargetHandler::resolve`, then `discover_targets`; read every resolved target once to construct `sources` as sorted root-relative path/lowercase BLAKE3 records. Use `config.fingerprint_inputs.clone()` for the other input list. Convert the resolved config with `into_plan_config`. Map a truncated discovery to the stable diagnostic code `candidate_limit` and exit 4. Treat `AnalyzerDiagnosticCode::InvalidSyntax`, source read/decode failures, target-resolution failures, and fingerprint-input failures as `PlanError` values returned before any stdout bytes are written; do not encode an invalid-syntax manifest.

In `lib.rs`, serialize only after `create` succeeds:

```rust
let output = plan::create(config).await?;
serde_json::to_writer(&mut *stdout, &output.manifest)?;
writeln!(stdout)?;
Ok(output.exit_code)
```

Do not construct `ShellContext`, `WorkspaceHandler`, `ProcessHandler`, `SessionHandler`, or `RunState` in this path.

- [ ] **Step 4: Run manifest and side-effect tests**

Run: `uv run cargo test -p hoimin-cli --test plan`

Expected: PASS. A plan is one JSON document on stdout, emits no test command output, and has no SQLite or worker-copy side effect.

- [ ] **Step 5: Commit planning support**

```bash
git add crates/hoimin-cli/src/plan.rs crates/hoimin-cli/src/lib.rs crates/hoimin-cli/tests/plan.rs
git commit -m "feat: create agent mutation plans"
```

### Task 5: Validate manifests completely before verify baseline

**Files:**
- Modify: `crates/hoimin-cli/src/plan.rs`
- Modify: `crates/hoimin-cli/tests/plan.rs`

**Interfaces:**
- Produces `prepare_verify(manifest_path, requested_ids, format) -> Result<VerifiedPlan, PlanError>`.
- `VerifiedPlan { config: RunConfig, candidate_ids: BTreeSet<String> }` contains no CLI-controlled mutation/test/session option.

- [ ] **Step 1: Write pre-baseline rejection tests**

For each mutated manifest, invoke `verify` with a test argv that would write a marker and assert marker absence, exit 2, and the named error code:

```rust
assert_verify_rejected_before_test("plan.source.changed", |value| change_target_source(value));
assert_verify_rejected_before_test("plan.fingerprint_input.changed", |value| change_fingerprint_input(value));
assert_verify_rejected_before_test("plan.candidate.invalid", |value| value["candidates"][0]["original"] = serde_json::json!("wrong"));
```

Add malformed JSON, `schema_version: 2`, wrong `kind`, `../outside.py` source path, duplicate manifest candidate IDs, absent requested ID, duplicate requested ID (accepted once), and requested count over `limits.max_mutants`. Test that a truncated manifest can verify one of the candidates it actually contains.

- [ ] **Step 2: Run the focused rejection test to verify validation is absent**

Run: `uv run cargo test -p hoimin-cli --test plan verify_rejects_changed_source_before_baseline -- --exact`

Expected: FAIL because `verify` has no manifest decoder/validator.

- [ ] **Step 3: Implement decode, current-input equality, and descriptor checks in order**

Implement `prepare_verify` in this strict sequence:

```rust
let manifest: PlanManifest = serde_json::from_slice(&std::fs::read(manifest_path)?)?;
validate_header(&manifest)?;                         // schema == 1, kind == "plan", required/normalized paths
let ids = normalize_requested_ids(requested_ids, manifest.normalized_config.limits.max_mutants)?;
let config = manifest.normalized_config.clone().into_run_config(output_from(format));
let targets = TargetHandler::resolve(&config.selection).await.map_err(PlanError::source_changed)?;
let current_sources = source_records(&config.root, &targets)?;
ensure_exact_records("plan.source.changed", &manifest.sources, &current_sources)?;
let current_inputs = fingerprint_inputs::resolve(&config.root, &config.fingerprint_includes)
    .map_err(PlanError::fingerprint_input_changed)?;
ensure_exact_records("plan.fingerprint_input.changed", &manifest.fingerprint_inputs, &current_inputs)?;
config.fingerprint_inputs = current_inputs;
validate_requested_candidates(&manifest, &ids, &config, &targets).await?;
Ok(VerifiedPlan { config, candidate_ids: ids })
```

`validate_requested_candidates` requires each requested ID to occur exactly once, converts it to `CandidateDescriptor`, calls `validate_candidate` against current source bytes, checks the returned stable ID equals `candidate.id`, and checks that `discover_targets` under the manifest config produces exactly the same descriptor for that ID. Map every parse/path/identity/descriptor failure to `plan.candidate.invalid`; map unrecognized requested IDs and count overflow to the same code. Do not rerun target selection with user CLI values or attempt relocation.

- [ ] **Step 4: Run all plan validation tests**

Run: `uv run cargo test -p hoimin-cli --test plan verify_rejects -- --nocapture && uv run cargo test -p hoimin-cli --test plan truncated_plan`

Expected: PASS. No rejected case starts baseline, worker copy, or the test process.

- [ ] **Step 5: Commit manifest validation**

```bash
git add crates/hoimin-cli/src/plan.rs crates/hoimin-cli/tests/plan.rs
git commit -m "feat: validate agent plan manifests"
```

### Task 6: Execute only verified candidate IDs through the existing run state machine

**Files:**
- Modify: `crates/hoimin-core/src/machine.rs:142-1160`
- Modify: `crates/hoimin-core/tests/machine.rs:1-1920`
- Modify: `crates/hoimin-cli/src/shell.rs:398-710`
- Modify: `crates/hoimin-cli/src/lib.rs:28-68`
- Modify: `crates/hoimin-cli/tests/plan.rs`

**Interfaces:**
- Produces `RunState::with_candidate_filter(run_id, config, BTreeSet<String>)` and `shell::run_selected_loop(config, candidate_ids, stdout, stderr)`.
- `verify` dispatches only after Task 5's `VerifiedPlan`; normal `run_loop` remains behaviorally unchanged.

- [ ] **Step 1: Write state-machine tests for filter semantics**

In `machine.rs` tests, construct a state with IDs `{second.id}` and feed it a spool containing `first` then `second`. Assert the first `CandidateLoaded` produces another `ReadCandidate`, the second schedules mutation, and summary/output contains only `second`. Add an empty-filter constructor test only if the API is public; `verify` itself must never pass an empty set.

```rust
let state = RunState::with_candidate_filter("run-1", fixture_config(), BTreeSet::from([second.id.clone()]));
// drive to Mutants, complete CandidateLoaded(first), then assert the next effect is ReadCandidate.
```

- [ ] **Step 2: Run the focused state-machine test to verify it fails**

Run: `uv run cargo test -p hoimin-core --test machine candidate_filter_skips_unrequested_candidates -- --exact`

Expected: compilation fails because the filtered constructor does not exist.

- [ ] **Step 3: Add a filter owned by `RunState`, not `RunConfig`**

Add `candidate_filter: Option<BTreeSet<String>>` to `RunState`, initialize it to `None` in `new`, and add:

```rust
pub fn with_candidate_filter(
    run_id: impl Into<String>,
    config: RunConfig,
    candidate_ids: BTreeSet<String>,
) -> Self {
    let mut state = Self::new(run_id, config);
    state.candidate_filter = Some(candidate_ids);
    state
}
```

In the `RunEvent::CandidateLoaded` transition, before setting `worker_state.candidate`, branch on `candidate_filter`: if the candidate exists but its ID is absent, leave that worker idle with no candidate and return `state.schedule_read_or_finalize()?`. Do not increment `scheduled_mutants`, emit mutant output, apply a mutation, or mark the run incomplete for skipped entries. Preserve source sequence order for retained candidates and retain the ordinary `max_mutants` guard.

In the `RunEvent::AnalysisFinished` transition, keep the existing `truncated` exit-4 path when `candidate_filter` is `None`. When it is `Some(_)`, require a returned spool, do **not** set the incomplete flag merely because discovery was truncated, enter `Mutants`, and schedule reads from that spool. This is what permits a candidate that is present in a truncated manifest to be verified: pre-baseline validation has already established that the requested ID belongs to the retained prefix, and verify does not claim that it enumerated all candidates.

Split the existing runner into a private `run_loop_prepared` that accepts a config whose input records were resolved exactly once. Let normal `run_loop`/`run_loop_with_control` call `prepare_run_config` then that private function. Add `run_selected_loop` beside them; it accepts the already validated `VerifiedPlan.config`, calls `run_loop_prepared` directly without resolving globs a second time, and constructs `RunState::with_candidate_filter`. It must reject `config.session.is_some()` or `config.resume` with a plain shell error, though `PlanConfig` conversion already sets both off.

In `lib.rs`, dispatch `ParsedCommand::Verify` by calling `plan::prepare_verify`, then `shell::run_selected_loop`. Do not emit a plan-specific event; existing JSON/JSONL/human report types remain the output contract.

- [ ] **Step 4: Run focused state and verify E2E tests**

Run: `uv run cargo test -p hoimin-core --test machine candidate_filter_skips_unrequested_candidates -- --exact && uv run cargo test -p hoimin-cli --test plan verify_runs_only_requested_candidates -- --exact`

Expected: PASS. The E2E report contains one fresh baseline and exactly the requested mutant IDs, never creates a SQLite session, and uses the requested `--format` only for rendering. Include the truncated-manifest candidate case from Task 5 in this command; it must execute its selected retained candidate rather than exit before mutation.

- [ ] **Step 5: Commit selected execution**

```bash
git add crates/hoimin-core/src/machine.rs crates/hoimin-core/tests/machine.rs crates/hoimin-cli/src/shell.rs crates/hoimin-cli/src/lib.rs crates/hoimin-cli/tests/plan.rs
git commit -m "feat: verify selected mutation candidates"
```

### Task 7: Document the agent workflow and protect regressions

**Files:**
- Modify: `README.md:45-165`
- Modify: `crates/hoimin-cli/tests/run_e2e.rs:330-410`
- Modify: `crates/hoimin-cli/tests/plan.rs`

**Interfaces:**
- Documents `plan > PLAN.json`, test improvement, and repeated `verify PLAN.json --candidate ...` commands.
- Preserves `run`/`progress` behavior and report schema version 2.

- [ ] **Step 1: Add documentation regression assertions**

Add a README test that asserts each literal is present:

```rust
for expected in [
    "hoimin plan",
    "hoimin verify PLAN.json --candidate",
    "`--fingerprint-include GLOB`",
    "`truncated`",
] {
    assert!(documented_readme.contains(expected), "missing {expected}");
}
```

- [ ] **Step 2: Run it to verify the documentation is not yet present**

Run: `uv run cargo test -p hoimin-cli --test run_e2e readme_documents_agent_plan_workflow -- --exact`

Expected: FAIL because the new commands are undocumented.

- [ ] **Step 3: Add concise, executable README guidance**

Add a section directly after the normal run example:

```console
hoimin plan --root . --source src --profile focused \
  --fingerprint-include pyproject.toml -- python -m pytest -q > PLAN.json
# improve tests, then choose IDs from PLAN.json
hoimin verify PLAN.json --candidate m1_example --format json
```

State that `plan` performs no baseline/test/copy/session work; a truncated manifest is a partial candidate set and exits 4; `verify` rejects changed target or fingerprint input before baseline; each verify reruns baseline and does not reuse a session; manifests are trusted local invocations, not a security boundary. Do not claim candidate relocation or full-plan coverage for a truncated manifest.

- [ ] **Step 4: Run documentation and regression tests**

Run: `uv run cargo test -p hoimin-cli --test run_e2e readme_documents_agent_plan_workflow -- --exact && uv run cargo test -p hoimin-cli --test progress && uv run cargo test -p hoimin-cli --test report_handler`

Expected: PASS. No JSON schema file or public schema version changes are necessary.

- [ ] **Step 5: Commit documentation**

```bash
git add README.md crates/hoimin-cli/tests/run_e2e.rs crates/hoimin-cli/tests/plan.rs
git commit -m "docs: describe agent plan verification workflow"
```

### Task 8: Run the complete quality gate

**Files:**
- Verify: all files touched by Tasks 1-7

- [ ] **Step 1: Format and lint the workspace**

Run: `uv run cargo fmt --check && uv run cargo clippy --workspace --all-targets -- -D warnings`

Expected: PASS with no newly allowed lints.

- [ ] **Step 2: Run all automated tests**

Run: `uv run cargo test --workspace`

Expected: PASS, including core state-machine contracts, plan/verify E2E coverage, session/resume coverage, report schema validation, and progress tests.

- [ ] **Step 3: Perform end-to-end command smoke checks in a disposable project**

Run: `uv run cargo test -p hoimin-cli --test plan && uv run cargo test -p hoimin-cli --test run_e2e focused_profile_is_reported_and_omits_arid_candidates -- --exact`

Expected: PASS. This jointly confirms plan parity with focused selection and preserves the existing focused `run` report contract.

- [ ] **Step 4: Inspect the final diff before integration**

Run: `git diff --check HEAD~7..HEAD && git status --short`

Expected: no whitespace errors and no staged unrelated local files such as `.idea/`.
