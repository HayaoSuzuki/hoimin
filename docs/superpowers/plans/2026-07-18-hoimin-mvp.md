# Hoimin MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a bounded-memory, cross-platform Python mutation-testing CLI that safely targets small source regions and emits stable machine-readable results for AI agents.

**Architecture:** A Cargo workspace separates a synchronous, I/O-free `hoimin-core` crate from the `hoimin-cli` imperative shell. The core consumes typed `RunEvent` values and returns `RunEffect` requests; narrow shell handlers perform Git, filesystem, LibCST process, worker, OS resource, reporting, and SQLite I/O, then return completion events. An optional `contracts` feature checks preconditions, postconditions, and invariants in CI while release wheels omit contract evaluation.

**Tech Stack:** Rust 2024/MSRV 1.85 Cargo workspace, Tokio and Clap in `hoimin-cli`, Serde and proptest-compatible value types in `hoimin-core`, BLAKE3, ignore, tempfile, rusqlite, Windows Job Objects, Linux cgroup v2/rlimits, Python 3.12–3.14, LibCST 1.8.x, Maturin, pytest.

## Global Constraints

- Treat `docs/superpowers/specs/2026-07-18-python-mutation-tool-design.md` as the source of truth when this plan and an implementation detail appear to conflict.
- Never mutate the user's project tree. Every mutant runs in a bounded disposable copy and the original source hashes are rechecked during execution.
- Do not invoke a shell for the user test command. Preserve argv after `--` and pass it directly to the OS process API.
- Keep candidates and result events on disk or in bounded channels; never accumulate the complete run in memory.
- Enforce one run-wide memory/process/copy budget across the analyzer, workers, and test descendants. `--jobs` must not multiply a configured limit.
- Keep machine output on stdout and diagnostics on stderr.
- Keep `hoimin-core` synchronous and free of filesystem, process, clock, environment, SQLite, Tokio, and OS API dependencies. Enforce this with the Cargo dependency graph.
- Give every shell request a run-unique `EffectId`; an accepted effect reaches exactly one success, failure, timeout, or cancellation event while the process remains alive.
- Pass candidate and output spool references across the core/shell boundary, never their unbounded contents.
- Compile contract checks only with the `contracts` feature. Without it, contract conditions and diagnostic arguments are not evaluated. User-facing validation remains active in every build.
- Develop each task test-first and commit only after its focused and regression tests pass.

Use these exact defaults in `RunLimits` and the CLI help:

```text
jobs=1                    max_mutants=100          max_candidates=10000
analyzer_timeout=30s      baseline_timeout=60s     mutant_timeout=auto
total_timeout=5m          max_memory=1GiB          max_output=1MiB
max_copy_size=1GiB        max_processes=64
```

Compute automatic mutant timeout as `max(5s, 2 * baseline elapsed + 1s)`. Total timeout includes validation preflight, copy, analysis, baseline, mutation runs, and final flushing.

---

## File Structure

```text
Cargo.toml                                      Workspace members and shared profile
Cargo.lock                                      Reproducible Rust dependency graph
pyproject.toml                                  Maturin manifest path and Python metadata
README.md                                       Install, CLI, safety, and output documentation
crates/hoimin-core/Cargo.toml                    I/O-free crate and contracts feature
crates/hoimin-core/src/lib.rs                    Public core API
crates/hoimin-core/src/contracts.rs              CI-only contract macros and invariant trait
crates/hoimin-core/src/model.rs                  Targets, candidates, results, summaries
crates/hoimin-core/src/effect.rs                 EffectId and RunEffect request types
crates/hoimin-core/src/event.rs                  RunEvent completion/input types
crates/hoimin-core/src/config.rs                 Pure cross-flag/default validation
crates/hoimin-core/src/target.rs                 Pure target union/intersection policy
crates/hoimin-core/src/candidate.rs              Stable mutant IDs and candidate policy
crates/hoimin-core/src/machine.rs                Synchronous Event/Effect state machine
crates/hoimin-core/src/report.rs                 Status, score, event, and exit policy
crates/hoimin-core/src/resume.rs                 Canonical fingerprint and reuse policy
crates/hoimin-core/tests/contracts.rs             Enabled/disabled contract behavior
crates/hoimin-core/tests/machine.rs               Table/property state-machine tests
crates/hoimin-cli/Cargo.toml                     Binary and I/O dependencies
crates/hoimin-cli/src/main.rs                    Thin CLI entry and exit handoff
crates/hoimin-cli/src/lib.rs                     Shell module graph
crates/hoimin-cli/src/cli.rs                     Clap grammar and raw argv conversion
crates/hoimin-cli/src/shell.rs                   Effect dispatch and event loop
crates/hoimin-cli/src/target/fs.rs                Filesystem target handler
crates/hoimin-cli/src/target/git.rs               Git changed-line handler
crates/hoimin-cli/src/analyzer/mod.rs             Embedded LibCST handler
crates/hoimin-cli/src/analyzer/protocol.rs        JSONL wire records
crates/hoimin-cli/src/analyzer/store.rs           Disk-backed candidate spool
crates/hoimin-cli/src/workspace/manifest.rs       Project and worker manifest
crates/hoimin-cli/src/workspace/copy.rs           Ignore-aware bounded copy
crates/hoimin-cli/src/workspace/mutation.rs       Validated byte patch
crates/hoimin-cli/src/workspace/reset.rs          Worker restore/delete handler
crates/hoimin-cli/src/process/mod.rs              Process effect handler
crates/hoimin-cli/src/process/output.rs           Bounded output spool/drain
crates/hoimin-cli/src/resource/portable.rs        Process-group/rlimit fallback
crates/hoimin-cli/src/resource/windows.rs         Windows Job Object backend
crates/hoimin-cli/src/resource/linux.rs           Linux cgroup v2 backend
crates/hoimin-cli/src/report/json.rs              Streaming final JSON writer
crates/hoimin-cli/src/report/jsonl.rs             Direct JSONL writer
crates/hoimin-cli/src/session/mod.rs              Single SQLite writer handler
crates/hoimin-cli/src/session/schema.rs           Schema and migrations
crates/hoimin-cli/tests/cli_config.rs              CLI integration tests
crates/hoimin-cli/tests/target_handler.rs          Filesystem/Git handler tests
crates/hoimin-cli/tests/analyzer_handler.rs        Rust↔Python handler tests
crates/hoimin-cli/tests/workspace_handler.rs       Copy/patch/reset tests
crates/hoimin-cli/tests/process_handler.rs         Timeout/output/descendant tests
crates/hoimin-cli/tests/report_handler.rs          JSON/JSONL handler tests
crates/hoimin-cli/tests/session_handler.rs         SQLite handler tests
crates/hoimin-cli/tests/run_e2e.rs                 Framework-independent E2E runs
python/hoimin_analyzer.py                          LibCST parser and operators
python/tests/test_analyzer.py                      Python helper tests
tests/wheel_smoke.py                               Installed-wheel smoke test
tests/fixtures/projects/basic/                     Pytest/unittest fixture project
.github/workflows/ci.yml                           Core/contracts/shell/platform matrix
.github/workflows/release.yml                      Contract-free wheel release
```

## Shared Interfaces

Create these types early and preserve their meaning across tasks:

```rust
pub struct ByteSpan { pub start: u64, pub length: u64 }
pub struct LineRange { pub start: u32, pub end: u32 }
pub struct TargetSlice {
    pub path: camino::Utf8PathBuf,
    pub lines: Vec<LineRange>,
    pub symbols: Vec<String>,
}
pub struct MutationCandidate {
    pub id: String,
    pub sequence: u64,
    pub path: camino::Utf8PathBuf,
    pub span: ByteSpan,
    pub original: String,
    pub replacement: String,
    pub operator: String,
    pub line: u32,
    pub column: u32,
    pub symbol: Option<String>,
    pub file_hash: String,
}
pub enum MutationStatus { Killed, Survived, Timeout, OutOfMemory, Error, NotRun }
pub enum ResourceMode { Hard, BestEffort }
pub enum CommandArg { Unix(Vec<u8>), Windows(Vec<u16>) }

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EffectId(pub u64);

pub struct CandidateSpoolRef { pub token: String, pub records: u64 }
pub struct OutputSpoolRef { pub token: String, pub retained: u64, pub observed: u64 }

pub enum RunEffect {
    ResolveTargets(ResolveTargets), Preflight(Preflight), CreateWorker(CreateWorker),
    RunBaseline(RunProcess), AnalyzeFile(AnalyzeFile), ReadCandidate(ReadCandidate),
    ApplyMutation(ApplyMutation), RunMutant(RunProcess), ResetWorker(ResetWorker),
    LoadSession(LoadSession), BeginSession(BeginSession), PersistResult(PersistResult),
    FinishSession(FinishSession), EmitOutput(EmitOutput), Cleanup(Cleanup),
}

pub enum RunEvent {
    StartRequested(StartRequested), TargetsResolved(TargetsResolved),
    PreflightCompleted(PreflightCompleted), WorkerCreated(WorkerCreated),
    BaselineFinished(ProcessFinished), AnalysisFinished(AnalysisFinished),
    CandidateLoaded(CandidateLoaded), MutationApplied(MutationApplied),
    MutantFinished(ProcessFinished), WorkerReset(WorkerReset),
    SessionLoaded(SessionLoaded), SessionStarted(SessionStarted),
    ResultPersisted(ResultPersisted), SessionFinished(SessionFinished),
    OutputEmitted(OutputEmitted),
    CleanupFinished(CleanupFinished), EffectFailed(EffectFailed),
    DeadlineReached, CancellationRequested,
}

pub fn transition(
    state: RunState,
    event: RunEvent,
) -> Result<(RunState, Vec<RunEffect>), MachineError>;
```

## Milestone 1: CLI, targets, and candidate discovery

### Task 1: Bootstrap the Rust/Python package and CLI grammar

**Files:**
- Create: `Cargo.toml`, `Cargo.lock`, `pyproject.toml`
- Create: `crates/hoimin-core/Cargo.toml`, `crates/hoimin-core/src/lib.rs`
- Create: `crates/hoimin-core/src/model.rs`, `crates/hoimin-core/src/effect.rs`, `crates/hoimin-core/src/event.rs`
- Create: `crates/hoimin-core/src/contracts.rs`, `crates/hoimin-core/tests/contracts.rs`
- Create: `crates/hoimin-cli/Cargo.toml`, `crates/hoimin-cli/src/main.rs`, `crates/hoimin-cli/src/lib.rs`, `crates/hoimin-cli/src/cli.rs`
- Test: `crates/hoimin-cli/tests/cli_config.rs`

**Interfaces:**
- Consumes: no project code; this task establishes the workspace.
- Produces: `EffectId`, `RunEffect`, `RunEvent`, shared model types, `contract_require!`, `contract_ensure!`, `ContractInvariant`, and `hoimin_cli::cli::parse_from`.

- [ ] **Step 1: Write failing CLI help and argv-boundary tests**

```rust
#[test]
fn run_requires_a_selector_and_test_argv() {
    let err = hoimin_cli::cli::parse_from(["hoimin", "run", "--", "python", "-m", "unittest"])
        .unwrap_err();
    assert!(err.to_string().contains("target selector"));
}

#[test]
fn command_after_separator_is_preserved_without_shell_parsing() {
    let cli = hoimin_cli::cli::parse_from([
        "hoimin", "run", "--file", "src/calc.py", "--", "python", "-m", "pytest", "-q",
    ]).unwrap();
    assert_eq!(cli.test_argv, ["python", "-m", "pytest", "-q"]);
}

#[cfg(not(feature = "contracts"))]
#[test]
fn disabled_contract_does_not_evaluate_condition_or_context() {
    let evaluated = Cell::new(false);
    contract_require!("test.disabled", { evaluated.set(true); false }, { evaluated.set(true); 1 });
    assert!(!evaluated.get());
}

#[cfg(feature = "contracts")]
#[test]
#[should_panic(expected = "contract violation [require:test.enabled]")]
fn enabled_contract_panics_with_stable_id() {
    contract_require!("test.enabled", false, 7_u8);
}
```

- [ ] **Step 2: Run the focused test and confirm the missing-crate/module failure**

Run: `cargo test -p hoimin-cli --test cli_config && cargo test -p hoimin-core --test contracts`

Expected: FAIL because the workspace crates, CLI parser, and contract macros do not exist.

- [ ] **Step 3: Add package metadata, dependencies, shared models, and the Clap parser**

Use Rust edition 2024 and set `rust-version = "1.85"`. Give `hoimin-core` only `serde`, `thiserror`, `camino`, and `blake3`; add `proptest` and `pretty_assertions` as its dev dependencies. Put `clap`, `serde_json`, `tempfile`, `tokio`, `ignore`, `uuid`, bundled `rusqlite`, `tracing`, `humantime`, and target-specific `windows-sys`/`libc` in `hoimin-cli`. Define no default feature in either crate; define `hoimin-cli/contracts = ["hoimin-core/contracts"]`. Configure `pyproject.toml` to build `crates/hoimin-cli/Cargo.toml`.

```rust
pub fn parse_from<I, T>(args: I) -> Result<RunArgs, CliError>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let root = RootCli::try_parse_from(args)?;
    RunArgs::try_from(root.command)
}
```

Implement enabled and disabled macro definitions in `contracts.rs`. The disabled definitions expand to `()` without referring to condition or context expressions. Implement `ContractInvariant` as `fn invariant(&self) -> bool` and call it only from enabled contract macros.

Model the selectors `--root`, `--source`, `--file`, `--line PATH:START-END`, `--symbol MODULE:QUALNAME`, `--changed`, and `--diff-base REV`, plus every safety/output/session flag from the design. Use the exact defaults above and require at least one test argv element.

- [ ] **Step 4: Keep `main` thin and verify help/output behavior**

`main` calls an async `hoimin_cli::run_from`, prints only structured results to stdout, prints typed errors to stderr, and exits with the returned code.

Run: `cargo test --workspace && cargo test -p hoimin-core --features contracts && cargo run -p hoimin-cli -- --help`

Expected: PASS; help lists `run`, selectors, safety flags, formats, session flags, and the `-- <test argv>` boundary.

- [ ] **Step 5: Commit**

```text
git add Cargo.toml Cargo.lock pyproject.toml crates
git commit -m "feat: scaffold sans-io hoimin workspace"
```

### Task 2: Validate configuration and resolve explicit targets

**Files:**
- Create: `crates/hoimin-core/src/config.rs`, `crates/hoimin-core/src/target.rs`
- Create: `crates/hoimin-cli/src/target/mod.rs`, `crates/hoimin-cli/src/target/fs.rs`
- Modify: `crates/hoimin-core/src/lib.rs`, `crates/hoimin-cli/src/cli.rs`
- Test: `crates/hoimin-core/tests/target_policy.rs`, `crates/hoimin-cli/tests/cli_config.rs`, `crates/hoimin-cli/tests/target_handler.rs`

**Interfaces:**
- Consumes: `RawRunConfig`, `TargetSlice`, `LineRange`, and contract macros from Task 1.
- Produces: `RunConfig::try_from(RawRunConfig)`, `auto_mutant_timeout(Duration) -> Duration`, `resolve_explicit(&Selection, &[DiscoveredFile]) -> Result<Vec<TargetSlice>, TargetError>`, and the filesystem handler for `RunEffect::ResolveTargets`.

- [ ] **Step 1: Write failing validation and normalization tests**

```rust
#[test]
fn explicit_selectors_form_a_union_and_are_normalized() {
    let targets = resolve_fixture(&[
        "--source", "pkg", "--file", "pkg/a.py", "--line", "pkg/b.py:4-7",
    ]).unwrap();
    assert_eq!(paths(&targets), ["pkg/a.py", "pkg/b.py", "pkg/c.py"]);
    assert_eq!(targets[1].lines, [LineRange { start: 4, end: 7 }]);
}

#[test]
fn diff_base_without_changed_is_rejected() {
    assert_config_error(&["--file", "a.py", "--diff-base", "main"], "requires --changed");
}

#[test]
fn automatic_mutant_timeout_uses_baseline_duration() {
    assert_eq!(auto_mutant_timeout(Duration::from_secs(1)), Duration::from_secs(5));
    assert_eq!(auto_mutant_timeout(Duration::from_secs(8)), Duration::from_secs(17));
}
```

Create explicit tests named `rejects_missing_selector`, `changed_requires_source`, `rejects_path_outside_root`, `rejects_file_outside_source`, `rejects_invalid_line_range`, `rejects_missing_or_non_python_file`, `resume_requires_session`, `rejects_zero_or_overflow_limit`, and `explicit_exclude_wins_over_include`; each asserts the exact `ConfigError` or `TargetError` variant.

- [ ] **Step 2: Run and confirm the unresolved resolver failure**

Run: `cargo test -p hoimin-core --test target_policy && cargo test -p hoimin-cli --test cli_config --test target_handler`

Expected: FAIL because config validation and explicit target resolution are absent.

- [ ] **Step 3: Implement typed `RunConfig` validation**

```rust
pub struct RunConfig {
    pub root: Utf8PathBuf,
    pub selection: Selection,
    pub limits: RunLimits,
    pub test_argv: Vec<CommandArg>,
    pub output: OutputConfig,
    pub session: Option<SessionConfig>,
}

impl TryFrom<RawRunConfig> for RunConfig {
    type Error = ConfigError;
    fn try_from(raw: RawRunConfig) -> Result<Self, Self::Error> {
        if raw.diff_base.is_some() && !raw.changed {
            return Err(ConfigError::DiffBaseRequiresChanged);
        }
        if raw.changed && raw.sources.is_empty() {
            return Err(ConfigError::ChangedRequiresSource);
        }
        if raw.resume && raw.session.is_none() {
            return Err(ConfigError::ResumeRequiresSession);
        }
        let selection = Selection::try_from(&raw)?;
        let limits = RunLimits::try_from(&raw)?;
        RunConfig::new(raw, selection, limits)
    }
}
```

Represent limits as nonzero byte/count/duration values so later modules cannot observe invalid limits. The CLI crate rejects non-UTF-8 selector/config values, but converts every test-command `OsString` losslessly to `CommandArg::Unix(Vec<u8>)` or `CommandArg::Windows(Vec<u16>)`; the process handler reverses that conversion without a shell.

- [ ] **Step 4: Implement ignore-aware explicit target discovery**

The filesystem handler uses `ignore::WalkBuilder` with `.gitignore`, `--include`, and `--exclude`; explicit excludes win. It returns `DiscoveredFile` values rather than selecting targets. Core canonicalizes the supplied logical paths, merges duplicate file/line/symbol entries deterministically, and checks `contract_ensure!("target.resolve.post", targets_are_normalized(&targets), &targets)` under the contracts feature.

- [ ] **Step 5: Run focused tests**

Run: `cargo test -p hoimin-core --test target_policy && cargo test -p hoimin-core --features contracts --test target_policy && cargo test -p hoimin-cli --test cli_config --test target_handler`

Expected: PASS on Windows and Linux, including mixed slash input and case-insensitive containment behavior on Windows.

- [ ] **Step 6: Commit**

```text
git add crates/hoimin-core crates/hoimin-cli
git commit -m "feat: validate config and explicit targets"
```

### Task 3: Add the Git changed-line handler

**Files:**
- Create: `crates/hoimin-cli/src/target/git.rs`
- Modify: `crates/hoimin-cli/src/target/mod.rs`, `crates/hoimin-core/src/target.rs`
- Test: `crates/hoimin-cli/tests/target_handler.rs`, `crates/hoimin-core/tests/target_policy.rs`

**Interfaces:**
- Consumes: `RunEffect::ResolveTargets`, `Selection`, `TargetSlice`, `LineRange`, and `EffectId` from Tasks 1–2.
- Produces: `handle_git(ResolveGitChanges) -> Result<GitChangesResolved, EffectFailed>`, concrete `TargetHandler::handle(ResolveTargets) -> Result<TargetsResolved, EffectFailed>`, and `intersect_changed(&[TargetSlice], &BTreeMap<Utf8PathBuf, Vec<LineRange>>) -> Vec<TargetSlice>`.

- [ ] **Step 1: Write failing temporary-repository tests**

```rust
#[test]
fn changed_collects_staged_unstaged_and_untracked_python_lines() {
    let repo = FixtureRepo::new();
    repo.stage_change("pkg/a.py", 2);
    repo.unstaged_change("pkg/b.py", 5);
    repo.write_untracked("pkg/c.py", "x = 1\n");
    repo.write_ignored("build/ignored.py", "x = 1\n");
    assert_eq!(repo.changed_lines(), BTreeMap::from([
        ("pkg/a.py", vec![LineRange { start: 2, end: 2 }]),
        ("pkg/b.py", vec![LineRange { start: 5, end: 5 }]),
        ("pkg/c.py", vec![LineRange { start: 1, end: 1 }]),
    ]));
}
```

Create `uses_diff_base_against_worktree`, `ignores_deleted_and_binary_paths`, `tracks_renamed_python_destination`, `rejects_non_repository`, and `changed_intersects_file_line_symbol_and_source`; assert exact normalized maps or `TargetError::GitRepositoryRequired`.

- [ ] **Step 2: Run and confirm failure**

Run: `cargo test -p hoimin-cli --test target_handler changed && cargo test -p hoimin-core --test target_policy changed`

Expected: FAIL because the Git resolver is not implemented.

- [ ] **Step 3: Implement Git invocation without a shell**

Without a base, invoke `git diff --unified=0` and `git diff --cached --unified=0`. With a base, invoke `git diff --unified=0 <base>` once so staged and unstaged changes are compared to that revision. In both cases, parse `@@` hunk headers and use `git ls-files --others --exclude-standard` for untracked files. Merge line ranges, intersect with explicit target slices when present, and preserve stable path/line ordering.

```rust
pub async fn handle_git(
    request: ResolveGitChanges,
) -> Result<GitChangesResolved, EffectFailed>;

pub fn intersect_changed(
    explicit: &[TargetSlice],
    changed: &BTreeMap<Utf8PathBuf, Vec<LineRange>>,
) -> Vec<TargetSlice>;
```

The handler only parses Git output and preserves the request `EffectId`. Core performs merging/intersection and checks `target.changed.post` for normalized, non-overlapping line ranges when contracts are enabled.

- [ ] **Step 4: Run target tests and the full Rust suite**

Run: `cargo test -p hoimin-core --features contracts --test target_policy && cargo test -p hoimin-cli --test target_handler && cargo test --workspace`

Expected: PASS; ignored untracked files are absent and changed+explicit selection is an intersection.

- [ ] **Step 5: Commit**

```text
git add crates/hoimin-core/src/target.rs crates/hoimin-core/tests/target_policy.rs crates/hoimin-cli/src/target crates/hoimin-cli/tests/target_handler.rs
git commit -m "feat: resolve changed Python lines"
```

### Task 4: Implement the LibCST candidate analyzer

**Files:**
- Create: `python/hoimin_analyzer.py`, `python/tests/test_analyzer.py`
- Create: `python/tests/fixtures/operators.py`
- Modify: `pyproject.toml`

**Interfaces:**
- Consumes: one JSONL request containing `effect_id`, normalized path, module, line ranges, and syntactic symbols.
- Produces: JSONL `candidate`, `diagnostic`, and `summary` records that echo `effect_id`; candidates contain UTF-8 byte spans and one replacement location.

- [ ] **Step 1: Write failing helper contract tests**

```python
def test_emits_every_mvp_operator_as_one_location_mutants(run_analyzer):
    events = run_analyzer("python/tests/fixtures/operators.py")
    operators = {event["operator"] for event in events if event["kind"] == "candidate"}
    assert operators == {
        "compare_eq_ne", "compare_order", "membership", "identity",
        "boolean_and_or", "binary_add_sub", "augmented_add_sub",
        "binary_mul_div", "binary_floor_mod", "unary_sign", "remove_not",
        "boolean_literal", "break_continue",
    }

def test_unicode_and_crlf_spans_are_utf8_bytes_and_round_trip(run_analyzer):
    candidate = run_analyzer("python/tests/fixtures/unicode_crlf.py")[0]
    raw = Path(candidate["path"]).read_bytes()
    span = candidate["span"]
    assert raw[span["start"]:span["start"] + span["length"]].decode() == candidate["original"]
```

Create named pytest cases for line filtering, `MODULE:QUALNAME`, nested class/function names, decorators, async functions, formatting/comments, LF/CRLF, invalid syntax, and supported Python syntax. Each case compares complete normalized candidate records or the exact diagnostic code.

- [ ] **Step 2: Run and confirm missing-helper failure**

Run: `uv run --python 3.12 pytest python/tests/test_analyzer.py -q`

Expected: FAIL because `python/hoimin_analyzer.py` does not exist.

- [ ] **Step 3: Implement the JSONL helper protocol and metadata visitor**

Read one request object per stdin line and write candidate/diagnostic/summary objects to stdout. Use `MetadataWrapper(module, unsafe_skip_copy=True)`, `PositionProvider`, and `ByteSpanPositionProvider`; visit one file per request and release its CST before reading the next request.

```python
@dataclass(frozen=True)
class AnalyzerRequest:
    path: str
    module: str
    lines: tuple[tuple[int, int], ...]
    symbols: tuple[str, ...]

def emit_candidate(node: cst.CSTNode, replacement: cst.CSTNode, operator: str) -> None:
    position = get_metadata(PositionProvider, node)
    byte_span = get_metadata(ByteSpanPositionProvider, node)
    write_jsonl(candidate_record(position, byte_span, node, replacement, operator))
```

- [ ] **Step 4: Implement each operator with exact syntax-preserving replacement**

Use LibCST node updates instead of text search. Generate exactly one changed location per candidate using these replacements: `==↔!=`, `<↔<=`, `>↔>=`, `in↔not in`, `is↔is not`, `and↔or`, binary and augmented `+↔-`, `*↔/`, `//↔%`, unary `+↔-`, remove unary `not`, `True↔False`, and `break↔continue`. Reject candidates whose original byte slice cannot be reconstructed or whose replacement fails a full-module parse.

- [ ] **Step 5: Run the helper matrix**

Run:

```text
uv run --python 3.12 pytest python/tests/test_analyzer.py -q
uv run --python 3.13 pytest python/tests/test_analyzer.py -q
uv run --python 3.14 pytest python/tests/test_analyzer.py -q
```

Expected: PASS three times with identical normalized candidate records.

- [ ] **Step 6: Commit**

```text
git add pyproject.toml python
git commit -m "feat: enumerate mutations with LibCST"
```

### Task 5: Define the analyzer protocol and bounded candidate spool

**Files:**
- Create: `crates/hoimin-core/src/candidate.rs`
- Create: `crates/hoimin-cli/src/analyzer/mod.rs`, `crates/hoimin-cli/src/analyzer/protocol.rs`, `crates/hoimin-cli/src/analyzer/store.rs`
- Modify: `crates/hoimin-core/src/lib.rs`, `crates/hoimin-cli/src/lib.rs`
- Test: `crates/hoimin-core/tests/candidate_policy.rs`, `crates/hoimin-cli/tests/analyzer_handler.rs`

**Interfaces:**
- Consumes: Task 4 JSONL records, `MutationCandidate`, `CandidateSpoolRef`, `EffectId`, and contract macros.
- Produces: `stable_mutant_id(&CandidateIdentity) -> MutantId`, `validate_candidate(&[u8], &CandidateDescriptor)`, `AnalyzerProtocol::receive_line`, and `CandidateStore::{push, finish, replay_one}`.

- [ ] **Step 1: Write failing protocol, ID, and bounded-store tests**

```rust
#[test]
fn candidate_id_is_stable_across_runs_and_root_locations() {
    let left = fixture_candidate("C:/one/repo", "pkg/calc.py");
    let right = fixture_candidate("D:/other/repo", "pkg/calc.py");
    assert_eq!(stable_mutant_id(&left), stable_mutant_id(&right));
}

#[test]
fn candidate_store_enforces_limit_without_retaining_records() {
    let mut store = CandidateStore::new(2).unwrap();
    store.push(&candidate(1)).unwrap();
    store.push(&candidate(2)).unwrap();
    assert!(matches!(store.push(&candidate(3)), Err(StoreError::LimitExceeded { limit: 2 })));
    assert_eq!(store.count(), 2);
}
```

Create `rejects_malformed_jsonl`, `preserves_helper_diagnostic`, `rejects_effect_id_mismatch`, `rejects_record_after_summary`, `rejects_span_original_mismatch`, `rejects_unparseable_replacement`, and `replays_in_stable_offset_order`; each asserts the exact protocol/store error or record.

- [ ] **Step 2: Run and confirm missing bridge failure**

Run: `cargo test -p hoimin-core --test candidate_policy && cargo test -p hoimin-cli --test analyzer_handler`

Expected: FAIL because the analyzer modules and candidate store are absent.

- [ ] **Step 3: Implement the disk-backed JSONL store and stable IDs**

Hash the exact schema version, BLAKE3 source hash, normalized relative path, byte span, operator, and replacement. Prefix the digest with `m1_`. Store records in a `NamedTempFile`, flush before replay, and deserialize one line at a time.

```rust
pub struct CandidateStore {
    file: tempfile::NamedTempFile,
    count: u64,
    max_candidates: u64,
}

impl CandidateStore {
    pub fn push(&mut self, candidate: &MutationCandidate) -> Result<(), StoreError>;
    pub fn finish(self) -> Result<CandidateSpoolRef, StoreError>;
    pub fn replay_one(reference: &CandidateSpoolRef, offset: u64)
        -> Result<Option<(MutationCandidate, u64)>, StoreError>;
}
```

After every accepted record, check `candidate_store.count.invariant`: logical count equals newline-delimited records written and never exceeds the configured limit. Reaching the limit returns `AnalysisLimitReached` as an expected analyzer completion, not `EffectFailed`.

- [ ] **Step 4: Implement the synchronous JSONL protocol state**

`AnalyzerProtocol` consumes complete stdout lines supplied by a caller and returns typed candidate, diagnostic, or summary records. It performs no process I/O and rejects an effect-ID mismatch, records after summary, malformed JSON, and a missing summary.

```rust
impl AnalyzerProtocol {
    pub fn receive_line(&mut self, line: &[u8])
        -> Result<Option<AnalyzerRecord>, ProtocolError>;
    pub fn finish(self) -> Result<AnalyzerSummary, ProtocolError>;
}
```

Task 12 embeds and launches the helper through the completed process handler, feeds lines into this protocol, and creates one request at a time.

- [ ] **Step 5: Implement candidate descriptor validation**

Core verifies the file hash and exact `original` bytes at the candidate span and computes the stable ID. Mutation writing remains in the Task 6 workspace handler.

Run: `cargo test -p hoimin-core --features contracts --test candidate_policy && cargo test -p hoimin-cli --test analyzer_handler && cargo test --workspace`

Expected: PASS; a 10,001st candidate produces the typed incomplete condition before any mutant command can run.

- [ ] **Step 6: Commit**

```text
git add crates/hoimin-core crates/hoimin-cli
git commit -m "feat: define bounded analyzer protocol"
```

## Milestone 2: Disposable workspaces and process containment

### Task 6: Build the workspace handler with deterministic reset

**Files:**
- Create: `crates/hoimin-core/src/budget.rs`, `crates/hoimin-core/tests/budget.rs`
- Create: `crates/hoimin-cli/src/workspace/mod.rs`, `crates/hoimin-cli/src/workspace/manifest.rs`
- Create: `crates/hoimin-cli/src/workspace/copy.rs`, `crates/hoimin-cli/src/workspace/mutation.rs`, `crates/hoimin-cli/src/workspace/reset.rs`
- Modify: `crates/hoimin-core/src/lib.rs`, `crates/hoimin-cli/src/lib.rs`
- Test: `crates/hoimin-cli/tests/workspace_handler.rs`

**Interfaces:**
- Consumes: workspace-related `RunEffect` variants, `EffectId`, `MutationCandidate`, and core budget reservations.
- Produces: `BudgetLedger::{reserve, release}`, `handle_preflight`, `handle_create_worker`, `handle_apply_mutation`, `handle_reset_worker`, and `handle_cleanup`, each returning its typed completion event or `EffectFailed` with the original ID.

- [ ] **Step 1: Write failing copy/reset tests**

```rust
#[test]
fn reset_restores_changed_and_deleted_files_and_removes_new_files() {
    let project = FixtureProject::new();
    let mut worker = WorkerWorkspace::create(project.root(), limits()).unwrap();
    worker.write("pkg/a.py", b"mutated\n").unwrap();
    worker.remove("pkg/b.py").unwrap();
    worker.write("generated.txt", b"new\n").unwrap();
    worker.reset().unwrap();
    assert_eq!(worker.read("pkg/a.py").unwrap(), b"original\n");
    assert!(worker.exists("pkg/b.py"));
    assert!(!worker.exists("generated.txt"));
}
```

Create `excludes_git_venv_and_caches`, `explicit_exclude_wins`, `skips_symlink_with_diagnostic`, `rejects_aggregate_copy_over_allowance`, `charges_every_worker_copy`, `resets_read_only_file_on_windows`, `detects_original_change`, and `rewrites_original_pythonpath_entries`; assert exact manifests, budget events, diagnostics, and environments.

- [ ] **Step 2: Run and confirm failure**

Run: `cargo test -p hoimin-core --test budget && cargo test -p hoimin-cli --test workspace_handler`

Expected: FAIL because workspace creation and reset do not exist.

- [ ] **Step 3: Implement a manifest and bounded copy**

First walk the tree and build a preflight manifest without materializing a worker; reject a manifest whose per-worker logical size times the requested worker count exceeds the run-wide budget. Record root-relative path, logical size, mtime, and BLAKE3 for every copied regular file. Exclude `.git`, venvs, `__pycache__`, `.pytest_cache`, and common type/lint caches by default. Skip symlinks with a diagnostic. Charge every logical copied byte for every worker against a run-wide atomic budget before writing the file.

```rust
pub struct ManifestEntry {
    pub path: Utf8PathBuf,
    pub size: u64,
    pub modified: Option<std::time::SystemTime>,
    pub blake3: blake3::Hash,
}

pub struct BudgetLedger {
    limits: RunBudgets,
    reservations: BTreeMap<ReservationId, Reservation>,
}

impl BudgetLedger {
    pub fn reserve(&mut self, kind: BudgetKind, amount: u64)
        -> Result<ReservationId, LimitReached>;
    pub fn release(&mut self, id: ReservationId) -> Result<(), BudgetError>;
}
```

The shell preflight computes logical bytes and returns them; core reserves the aggregate amount before returning `CreateWorker`. The handler still stops if observed bytes exceed the granted allowance. With contracts enabled, check `budget.total.invariant` after every reserve/release and reject double release.

- [ ] **Step 4: Implement reset and original-integrity checks**

Before applying a mutation, verify file hash and exact original bytes and replace only the candidate span. Compare worker state to the manifest after every mutant: restore changed/deleted files from the pristine snapshot and delete unmanifested files. If any restore operation fails, return `EffectFailed::WorkspaceRestore`; the core later requests discard/recreate within its existing budget. Rehash original target files before analysis, periodically during the run, and before final reporting.

After reset, check both the always-on manifest comparison and the CI contract `workspace.reset.post`. The contract supplies a more local panic for development but does not replace the typed release-build error.

- [ ] **Step 5: Implement command environment construction**

Set cwd to worker root. Rewrite inherited `PYTHONPATH` entries that resolve inside the original root to the corresponding worker path, then prepend worker root and selected source roots without duplicates.

Run: `cargo test -p hoimin-core --features contracts --test budget && cargo test -p hoimin-cli --features contracts --test workspace_handler && cargo test --workspace`

Expected: PASS on Windows and Linux; no test modifies the fixture source tree.

- [ ] **Step 6: Commit**

```text
git add crates/hoimin-core crates/hoimin-cli/src/workspace crates/hoimin-cli/tests/workspace_handler.rs
git commit -m "feat: isolate mutants in resettable workspaces"
```

### Task 7: Implement the portable bounded process handler

**Files:**
- Create: `crates/hoimin-cli/src/process/mod.rs`, `crates/hoimin-cli/src/process/output.rs`
- Create: `crates/hoimin-cli/src/resource/mod.rs`, `crates/hoimin-cli/src/resource/portable.rs`
- Modify: `crates/hoimin-core/src/event.rs`, `crates/hoimin-core/src/model.rs`, `crates/hoimin-cli/src/lib.rs`
- Test: `crates/hoimin-cli/tests/process_handler.rs`

**Interfaces:**
- Consumes: `RunEffect::RunBaseline`, `RunEffect::RunMutant`, `CommandArg`, `EffectId`, `ProcessLimits`, and `OutputSpoolRef`.
- Produces: concrete `ProcessHandler::run(ProcessRequest) -> Result<ProcessFinished, EffectFailed>`, `ProcessHandler::handle(RunProcess)` as its core-effect wrapper, `ProcessTermination`, and `ResourceBackend::Portable`.

- [ ] **Step 1: Write failing timeout and output-cap tests**

```rust
#[tokio::test]
async fn output_is_drained_after_retention_cap() {
    let request = run_process(
        python("import sys; sys.stdout.write('x' * 2000000)"),
        ProcessLimits { timeout: Duration::from_secs(5), max_output_bytes: 1024 },
    );
    let event = portable_handler().handle(request).await.unwrap();
    assert_eq!(event.output.retained, 1024);
    assert_eq!(event.output.observed, 2_000_000);
    assert!(event.output.observed > event.output.retained);
}

#[tokio::test]
async fn timeout_terminates_descendants() {
    let event = portable_handler().handle(run_process(spawn_child_forever(), short_limits())).await.unwrap();
    assert_eq!(event.termination, ProcessTermination::Timeout);
    assert!(!process_exists(event.fixture_child_pid));
}
```

Create `drains_stdout_and_stderr_under_combined_cap`, `spools_non_utf8_output`, `returns_zero_and_nonzero_exit`, `returns_cancelled`, `maps_spawn_failure_to_effect_failed`, and `honors_requested_timeout`; assert exact `ProcessFinished`, `EffectFailed`, and `OutputSpoolRef` fields. Timeout calculation remains in Task 2 core policy.

- [ ] **Step 2: Run and confirm failure**

Run: `cargo test -p hoimin-cli --test process_handler portable -- --nocapture`

Expected: FAIL because process supervision is absent.

- [ ] **Step 3: Implement the concrete process handler and bounded pipe drains**

```rust
pub struct ProcessHandler {
    backend: ResourceBackend,
    output_dir: Utf8PathBuf,
}

impl ProcessHandler {
    pub async fn run(&self, request: ProcessRequest)
        -> Result<ProcessFinished, EffectFailed>;
    pub async fn handle(&self, request: RunProcess)
        -> Result<ProcessFinished, EffectFailed> {
        self.run(request.into()).await
    }
}
```

Reconstruct platform-native argv and spawn directly without a shell. Concurrently drain stdout/stderr into a temp spool while retaining at most the combined cap and counting all observed bytes. Return only `OutputSpoolRef` to core. On timeout or cancellation, terminate the process group and keep draining until pipes close.

- [ ] **Step 4: Add portable Unix limits and explicit best-effort policy**

On Unix, create a process group and apply `RLIMIT_AS`/`RLIMIT_CPU` in `pre_exec`. Classify this backend as `BestEffort`. Refuse to start a normal Linux run under it unless `--allow-best-effort-memory` is set; unit tests may construct it explicitly.

- [ ] **Step 5: Add baseline/mutant command classification**

Return raw `ProcessTermination::{Exit(0), Exit(nonzero), Timeout, OutOfMemory, ProcessLimit, Cancelled}`. Do not classify baseline or mutant status in this handler; Task 10 adds the pure core policy.

Run: `cargo test -p hoimin-cli --test process_handler portable && cargo test --workspace`

Expected: PASS with no surviving fixture descendants.

- [ ] **Step 6: Commit**

```text
git add crates/hoimin-core/src crates/hoimin-cli/src/process crates/hoimin-cli/src/resource crates/hoimin-cli/tests/process_handler.rs
git commit -m "feat: handle bounded test processes"
```

### Task 8: Add the Windows Job Object hard-limit backend

**Files:**
- Create: `crates/hoimin-cli/src/resource/windows.rs`
- Modify: `crates/hoimin-cli/src/resource/mod.rs`, `crates/hoimin-cli/Cargo.toml`
- Test: `crates/hoimin-cli/tests/process_handler.rs`

**Interfaces:**
- Consumes: Task 7 `ProcessHandler`, `RunLimits`, `ProcessTermination`, and `ResourceMode`.
- Produces: `WindowsBackend::new(&RunLimits)`, `WindowsBackend::spawn_root`, and `ResourceBackend::Windows` with one run-wide Job Object.

- [ ] **Step 1: Write Windows-only failing tests**

```rust
#[cfg(windows)]
#[tokio::test]
async fn job_object_enforces_memory_process_and_descendant_cleanup() {
    let backend = WindowsBackend::new(&hard_limits(memory_mib(96), 3)).unwrap();
    let run = ProcessHandler::new(ResourceBackend::Windows(backend), output_dir());
    assert_eq!(run.mode(), ResourceMode::Hard);
    assert_eq!(run.handle(run_process(memory_hog(), limits())).await.unwrap().termination,
               ProcessTermination::OutOfMemory);
    assert_eq!(run.handle(run_process(process_fanout(8), limits())).await.unwrap().termination,
               ProcessTermination::ProcessLimit);
    run.close();
    assert_no_fixture_descendants();
}
```

- [ ] **Step 2: Run on Windows and confirm failure**

Run: `cargo test -p hoimin-cli --test process_handler job_object -- --nocapture`

Expected: FAIL because `WindowsSupervisor` is absent.

- [ ] **Step 3: Implement one run-wide Job Object**

Create it before analyzer/worker processes. Set `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, `JOB_OBJECT_LIMIT_JOB_MEMORY`, and `JOB_OBJECT_LIMIT_ACTIVE_PROCESS`; create each root process suspended, assign it to the Job Object, then resume its primary thread so user code cannot fork outside the job. Query job accounting to distinguish memory/process-limit termination from ordinary exit.

- [ ] **Step 4: Verify timeout, Ctrl+C-equivalent close, and aggregate accounting**

Run: `cargo test -p hoimin-cli --test process_handler -- --nocapture`

Expected on Windows: PASS; aggregate memory and active-process caps apply across two concurrent workers and the analyzer.

- [ ] **Step 5: Commit**

```text
git add crates/hoimin-cli/Cargo.toml crates/hoimin-cli/src/resource crates/hoimin-cli/tests/process_handler.rs
git commit -m "feat: enforce Windows Job Object limits"
```

### Task 9: Add Linux cgroup v2 hard limits and fallback detection

**Files:**
- Create: `crates/hoimin-cli/src/resource/linux.rs`
- Modify: `crates/hoimin-cli/src/resource/mod.rs`, `crates/hoimin-cli/Cargo.toml`
- Test: `crates/hoimin-cli/tests/process_handler.rs`

**Interfaces:**
- Consumes: Task 7 `ProcessHandler`, `RunLimits`, `ProcessTermination`, `ResourceMode`, and the best-effort opt-in.
- Produces: `LinuxBackend::probe`, `select_linux_backend(CgroupCapabilities, bool)`, and `ResourceBackend::{LinuxHard, Portable}`.

- [ ] **Step 1: Write failing Linux-only backend-selection tests**

```rust
#[cfg(target_os = "linux")]
#[test]
fn unavailable_cgroup_requires_explicit_best_effort_opt_in() {
    let capabilities = CgroupCapabilities::Unavailable("read-only subtree".into());
    let err = select_linux_backend(capabilities.clone(), false).unwrap_err();
    assert!(err.to_string().contains("--allow-best-effort-memory"));
    assert_eq!(select_linux_backend(capabilities, true).unwrap().mode(), ResourceMode::BestEffort);
}
```

Create host-capability-gated tests `enforces_aggregate_memory_max`, `enforces_pids_max`, `classifies_memory_events`, `kills_descendants`, and `removes_run_cgroup`; skip only when the probe reports no delegated writable cgroup.

- [ ] **Step 2: Run and confirm failure**

Run: `cargo test -p hoimin-cli --test process_handler linux -- --nocapture`

Expected: FAIL because backend probing and cgroup setup are absent.

- [ ] **Step 3: Implement a run-wide cgroup v2 backend**

Probe a writable delegated cgroup v2 subtree, create one uniquely named child, set `memory.max` and `pids.max`, and attach every analyzer/test root PID before it may fork. Start the child stopped, attach its PID, then continue it. Use process groups for timeouts and cancellation. Read `memory.events` and `pids.events` for classification, then kill remaining members and remove the cgroup on drop.

- [ ] **Step 4: Implement deterministic hard/fallback selection**

Return `Hard` only after limits are successfully written and read back. If setup is unavailable, emit a diagnostic describing the failed capability and either refuse the run or select the Task 7 backend when opt-in is present.

Run: `cargo test -p hoimin-cli --test process_handler linux -- --nocapture && cargo test --workspace`

Expected on Linux: PASS; hard-limit tests run when the CI runner exposes delegated cgroups, and fallback-policy tests always run.

- [ ] **Step 5: Commit**

```text
git add crates/hoimin-cli/Cargo.toml crates/hoimin-cli/src/resource crates/hoimin-cli/tests/process_handler.rs
git commit -m "feat: enforce Linux cgroup resource limits"
```

## Milestone 3: Reporting, resume state, and the run engine

### Task 10: Implement streaming reports, scoring, and exit policy

**Files:**
- Create: `crates/hoimin-core/src/report.rs`, `crates/hoimin-core/tests/report_policy.rs`
- Create: `crates/hoimin-cli/src/report/mod.rs`, `crates/hoimin-cli/src/report/json.rs`, `crates/hoimin-cli/src/report/jsonl.rs`
- Modify: `crates/hoimin-core/src/lib.rs`, `crates/hoimin-cli/src/lib.rs`
- Test: `crates/hoimin-cli/tests/report_handler.rs`

**Interfaces:**
- Consumes: `ProcessTermination`, `MutationStatus`, `OutputSpoolRef`, `RunEffect::EmitOutput`, and output format configuration.
- Produces: pure `classify_mutant`, `summarize`, `exit_code`, versioned `OutputEvent`, and concrete `ReportHandler::handle(EmitOutput) -> Result<OutputEmitted, EffectFailed>`.

- [ ] **Step 1: Write failing schema and policy tests**

```rust
#[test]
fn score_uses_only_killed_and_survived() {
    let summary = summarize(&[
        MutationStatus::Killed, MutationStatus::Survived,
        MutationStatus::Timeout, MutationStatus::OutOfMemory,
    ]);
    assert_eq!(summary.score, Some(0.5));
    assert_eq!(summary.inconclusive, 2);
}

#[test]
fn incomplete_takes_precedence_over_survivors() {
    assert_eq!(exit_code(true, true, false), 4);
    assert_eq!(exit_code(false, true, false), 1);
    assert_eq!(exit_code(false, false, false), 0);
}
```

Test the complete exit table: infrastructure `2`, baseline `3`, incomplete `4`, Ctrl+C `130`; no candidates yields code `0` and null score. Validate all event kinds and monotonic `sequence` values.

- [ ] **Step 2: Run and confirm failure**

Run: `cargo test -p hoimin-core --test report_policy && cargo test -p hoimin-cli --test report_handler`

Expected: FAIL because reporters and exit policy are absent.

- [ ] **Step 3: Define versioned event/result records**

```rust
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OutputEvent {
    RunStarted(RunStarted),
    BaselineFinished(BaselineFinished),
    MutantStarted(MutantStarted),
    MutantFinished(MutantFinished),
    Diagnostic(Diagnostic),
    RunFinished(RunSummary),
}

impl ReportHandler {
    pub fn handle(&mut self, request: EmitOutput)
        -> Result<OutputEmitted, EffectFailed>;
}
```

Serialize the variants with the exact `kind` strings `run_started`, `baseline_finished`, `mutant_started`, `mutant_finished`, `diagnostic`, and `run_finished`.

- [ ] **Step 4: Implement bounded JSONL and JSON writers**

Core converts raw process termination to mutation status and constructs the next `OutputEvent`; the handler only serializes it. JSONL serializes and flushes each event directly. JSON writes mutant records into a temp spool and assembles the final object by streaming the spool between the fixed header and summary; do not build a `Vec<MutantResult>`. Both formats write diagnostics to stderr only.

Core tracks output sequence as a value object and checks `report.sequence.invariant` under contracts: it is monotonic, and each mutant finish follows its start. The report handler echoes `EffectId` and does not allocate sequence numbers.

- [ ] **Step 5: Verify byte-for-byte output contracts**

Run: `cargo test -p hoimin-core --features contracts --test report_policy && cargo test -p hoimin-cli --test report_handler && cargo test --workspace`

Expected: PASS; a large synthetic run keeps the test process's measured heap below a fixed allowance independent of result count.

- [ ] **Step 6: Commit**

```text
git add crates/hoimin-core/src/report.rs crates/hoimin-core/tests/report_policy.rs crates/hoimin-cli/src/report crates/hoimin-cli/tests/report_handler.rs
git commit -m "feat: stream machine-readable mutation reports"
```

### Task 11: Add SQLite sessions and compatible resume

**Files:**
- Create: `crates/hoimin-core/src/resume.rs`, `crates/hoimin-core/tests/resume_policy.rs`
- Create: `crates/hoimin-cli/src/session/mod.rs`, `crates/hoimin-cli/src/session/schema.rs`
- Modify: `crates/hoimin-core/src/lib.rs`, `crates/hoimin-cli/src/lib.rs`
- Test: `crates/hoimin-cli/tests/session_handler.rs`

**Interfaces:**
- Consumes: `CommandArg`, source/target hashes, limits, versions, resource mode, `LoadSession`, `BeginSession`, `PersistResult`, `FinishSession`, and `MutantResult`.
- Produces: pure `fingerprint(&FingerprintInput) -> RunFingerprint`, `resume_policy(&[StoredResult]) -> ResumeDecision`, and concrete `SessionHandler::{load, begin, persist, finish}` methods returning the matching typed completion event or `EffectFailed`.

- [ ] **Step 1: Write failing persistence/resume tests**

```rust
#[test]
fn resume_reuses_only_determinate_completed_results() {
    let db = fixture_session(&[
        ("m1", MutationStatus::Killed),
        ("m2", MutationStatus::Survived),
        ("m3", MutationStatus::Timeout),
        ("m4", MutationStatus::Error),
    ]);
    let resume = db.resume(compatible_fingerprint()).unwrap();
    assert_eq!(resume.reusable_ids, BTreeSet::from(["m1", "m2"]));
    assert_eq!(resume.rerun_ids, BTreeSet::from(["m3", "m4"]));
}
```

Create `selects_newest_compatible_incomplete_run`, one fingerprint-mismatch test per source/target/operator/argv/limit/Python/LibCST field, `rolls_back_failed_transaction`, `maps_commit_failure`, `ignores_complete_run`, and `reruns_not_run`; assert exact `ResumeDecision` and completion events.

- [ ] **Step 2: Run and confirm failure**

Run: `cargo test -p hoimin-core --test resume_policy && cargo test -p hoimin-cli --test session_handler`

Expected: FAIL because session storage is absent.

- [ ] **Step 3: Create and migrate the schema**

Use `PRAGMA user_version`. Store runs, normalized fingerprints, candidates, results, and diagnostics. Enable WAL and a busy timeout. Use one writer connection owned by one task/thread; commit one transaction per mutant so interruption loses at most the active result.

- [ ] **Step 4: Implement canonical fingerprinting and resume selection**

Encode a sorted canonical structure with an explicit schema byte and length-prefixed fields containing source BLAKE3 hashes, normalized target slices, enabled operators, exact `CommandArg` units, all safety limits, Python version, LibCST version, and resource mode. Hash those canonical bytes with BLAKE3 without adding a JSON dependency to core. Resume the newest matching incomplete run and return only killed/survived records as reusable.

```rust
impl SessionHandler {
    pub fn load(&mut self, request: LoadSession) -> Result<SessionLoaded, EffectFailed>;
    pub fn begin(&mut self, request: BeginSession) -> Result<SessionStarted, EffectFailed>;
    pub fn persist(&mut self, request: PersistResult) -> Result<ResultPersisted, EffectFailed>;
    pub fn finish(&mut self, request: FinishSession) -> Result<SessionFinished, EffectFailed>;
}
```

The handler performs no reuse or retry policy. It returns stored rows to core and commits requested records one transaction at a time. With contracts enabled, `session.commit.post` reads back the `(run_id, mutant_id)` record after commit and checks equality.

**Implementation deviation after review:** The batch-shaped example in Step 1 is not the runtime API. To preserve the design requirement that functional-core memory is independent of mutant count (design lines 109–112), `LoadSession` carries the fingerprint and returns only the newest compatible incomplete run reference. A separate `LookupStoredResult` effect reads at most one status for the current candidate, and core classifies one `Option<StoredResult>` as reuse or rerun. No runtime `Vec<StoredRun>`, `BTreeSet` of mutant IDs, or run-wide diagnostics load is used. An inconclusive stored result may be transactionally replaced after resume; `finish(false)` may later transition to `finish(true)`, while completed runs reject further persistence and finish operations.

- [ ] **Step 5: Run focused and regression tests**

Run: `cargo test -p hoimin-core --features contracts --test resume_policy && cargo test -p hoimin-cli --features contracts --test session_handler && cargo test --workspace`

Expected: PASS; a deliberately failed commit stops scheduling subsequent mutants and returns infrastructure code `2`.

- [ ] **Step 6: Commit**

```text
git add crates/hoimin-core/src/resume.rs crates/hoimin-core/tests/resume_policy.rs crates/hoimin-cli/src/session crates/hoimin-cli/tests/session_handler.rs
git commit -m "feat: persist and resume mutation sessions"
```

### Task 12: Implement the core run machine and sequential shell loop

**Files:**
- Create: `crates/hoimin-core/src/machine.rs`, `crates/hoimin-core/tests/machine.rs`
- Create: `crates/hoimin-cli/src/shell.rs`
- Modify: `crates/hoimin-core/src/lib.rs`, `crates/hoimin-cli/src/lib.rs`, `crates/hoimin-cli/src/main.rs`, `crates/hoimin-cli/src/analyzer/mod.rs`
- Create: `crates/hoimin-cli/tests/run_e2e.rs`
- Create: `tests/fixtures/projects/basic/pyproject.toml`
- Create: `tests/fixtures/projects/basic/src/calc.py`
- Create: `tests/fixtures/projects/basic/tests/test_calc_pytest.py`
- Create: `tests/fixtures/projects/basic/tests/test_calc_unittest.py`

**Interfaces:**
- Consumes: every `RunEffect`/`RunEvent`, core policy, spool reference, and concrete shell handler produced by Tasks 1–11.
- Produces: pure `transition(RunState, RunEvent)`, `MachineError`, `ShellContext`, `execute_effect(&mut ShellContext, RunEffect) -> RunEvent`, and sequential `run_loop`.

- [ ] **Step 1: Write failing framework-independent end-to-end tests**

```rust
#[test]
fn pytest_and_unittest_commands_produce_the_same_mutant_statuses() {
    let pytest = run_fixture(["python", "-m", "pytest", "-q"]);
    let unittest = run_fixture(["python", "-m", "unittest", "discover", "-s", "tests"]);
    assert_eq!(pytest.statuses(), unittest.statuses());
    assert_eq!(pytest.exit_code(), unittest.exit_code());
}

#[test]
fn failing_baseline_runs_no_mutants_and_returns_three() {
    let run = run_fixture_with_failing_baseline();
    assert_eq!(run.exit_code(), 3);
    assert_eq!(run.mutant_started_count(), 0);
}

#[test]
fn baseline_success_requests_analysis_without_performing_io() {
    let state = fixture_state_waiting_for_baseline(effect_id(4));
    let event = RunEvent::BaselineFinished(process_finished(effect_id(4), 0));
    let (next, effects) = transition(state, event).unwrap();
    assert_eq!(next.phase(), RunPhase::Analyze);
    assert!(matches!(effects.as_slice(), [RunEffect::AnalyzeFile(_)]));
}
```

Create named tests for no candidates, every status, `--max-mutants` stable truncation/not_run results, analyzer/copy/reset failure, original modification, clean stdout, and session save errors. In `machine.rs`, add table rows for every valid phase transition plus unknown ID, duplicate completion, wrong-phase event, fatal error, and cancellation.

- [ ] **Step 2: Run and confirm failure**

Run: `cargo test -p hoimin-core --test machine && cargo test -p hoimin-cli --test run_e2e -- --nocapture`

Expected: FAIL because no complete run engine exists.

- [ ] **Step 3: Implement the synchronous core transition function**

```rust
pub enum RunPhase { Validate, Preflight, Copy, Baseline, Analyze, Mutants, Finalize, Cleaning, Finished }

pub fn transition(
    mut state: RunState,
    event: RunEvent,
) -> Result<(RunState, Vec<RunEffect>), MachineError> {
    state.accept(event)?;
    let effects = state.next_effects()?;
    state.register(&effects)?;
    Ok((state, effects))
}
```

`RunState` owns the phase, next effect ID, pending-effect map, workers, budget ledger, candidate offset, completed IDs, summary, and output sequence. It contains no path contents, process handles, clocks, database handles, or async primitives. Validate CLI constraints and request target/Python/LibCST/copy preflight before workspace creation. Then request copy, baseline, one-file-at-a-time analysis, candidate replay, patch, test, persist, report, and reset.

With contracts enabled, check `machine.pending.invariant`, `machine.transition.post`, `machine.worker.invariant`, and `machine.fatal.post` after every transition. Unknown, duplicate, or wrong-phase completion returns `MachineError`; expected shell failures remain `RunEvent::EffectFailed` and follow the normal state table.

- [ ] **Step 4: Implement the concrete sequential shell loop**

`execute_effect` matches every `RunEffect` and calls exactly one target, workspace, analyzer, process, report, or session handler. It embeds/materializes the Python helper, verifies Python/LibCST, drains helper output through Task 5 `AnalyzerProtocol`, and stores candidates through `CandidateStore`. Map every handler error to one `RunEvent::EffectFailed` with the original ID.

`run_loop` starts with `StartRequested`, calls pure `transition`, executes returned effects with `--jobs 1`, and feeds each completion event back. The shell does not inspect phase/status to decide what to run next.

- [ ] **Step 5: Enforce run limits in core sequential policy**

Start the total-timeout clock before copy. Stop before mutant execution on candidate overflow. On mutant overflow, run the first stable `--max-mutants`, emit `not_run` for the rest, and mark incomplete. Abort following mutants on span mismatch, reset failure, original change, or persistence failure.

- [ ] **Step 6: Run core, contract, end-to-end, and workspace tests**

Run: `cargo test -p hoimin-core --features contracts --test machine && cargo test -p hoimin-cli --features contracts --test run_e2e -- --nocapture && cargo test --workspace`

Expected: PASS for both pytest and unittest commands, proving there is no pytest integration dependency.

- [ ] **Step 7: Commit**

```text
git add crates/hoimin-core/src/machine.rs crates/hoimin-core/tests/machine.rs crates/hoimin-cli/src crates/hoimin-cli/tests/run_e2e.rs tests/fixtures/projects/basic
git commit -m "feat: execute runs through sans-io machine"
```

### Task 13: Add bounded parallel scheduling, total timeout, and cancellation

**Files:**
- Modify: `crates/hoimin-core/src/machine.rs`, `crates/hoimin-core/tests/machine.rs`
- Modify: `crates/hoimin-cli/src/shell.rs`, `crates/hoimin-cli/src/workspace/mod.rs`, `crates/hoimin-cli/src/resource/mod.rs`
- Modify: `crates/hoimin-cli/src/report/mod.rs`, `crates/hoimin-cli/src/session/mod.rs`
- Test: `crates/hoimin-cli/tests/run_e2e.rs`, `crates/hoimin-cli/tests/process_handler.rs`

**Interfaces:**
- Consumes: Task 12 `transition`, `ShellContext`, `execute_effect`, worker state, budget ledger, and platform resource backend.
- Produces: bounded concurrent `run_loop`, external `DeadlineReached`/`CancellationRequested` injection, and core scheduling of up to `jobs` in-flight mutant effects.

- [ ] **Step 1: Write failing concurrency and Ctrl+C tests**

```rust
#[tokio::test]
async fn jobs_do_not_multiply_run_wide_limits() {
    let run = run_fixture_async(["--jobs", "4", "--max-memory", "128MiB", "--max-processes", "6"]).await;
    assert!(run.observed_process_peak <= 6);
    assert!(run.observed_memory_peak <= mebibytes(128));
}

#[tokio::test]
async fn cancellation_stops_descendants_flushes_results_and_returns_130() {
    let run = start_long_fixture().await;
    run.cancel().await;
    let finished = run.wait().await;
    assert_eq!(finished.exit_code(), 130);
    assert!(finished.output_is_parseable());
    assert_no_fixture_descendants();
}
```

Add core event-trace tests for completions arriving in every worker order, monotonic output sequences, worker isolation, total deadline during every phase, duplicate EffectId rejection, and no mutation effect after fatal/cancellation events.

- [ ] **Step 2: Run and confirm failure**

Run: `cargo test -p hoimin-core --test machine parallel && cargo test -p hoimin-cli --test run_e2e parallel -- --nocapture`

Expected: FAIL because execution is sequential and lacks cancellation coordination.

- [ ] **Step 3: Implement bounded worker scheduling**

Core returns at most `--jobs` independent `ReadCandidate`/worker effect chains and reserves all run-wide budgets before issuing them. The shell executes returned effects in a `JoinSet` and sends only completion `RunEvent` values through a bounded channel to the single transition loop. Each worker keeps a disposable workspace; analyzer, report, and session handlers remain single-owner where ordering requires it. Output remains in spools referenced by events.

Do not reorder completion events in the shell. The transition loop assigns output sequence in arrival order and selects the next effects deterministically from current state plus the received event.

- [ ] **Step 4: Implement total deadline and cancellation**

Use one cancellation token only in the shell. Convert deadline expiry and Ctrl+C to core events, then execute only the stop/reset/flush/cleanup effects returned by core. Process handlers also observe the token to terminate descendants and return `Cancelled`. Core marks undispatched candidates `not_run` and selects exit `4` or `130`.

Check contracts `machine.worker.invariant`, `machine.budget.invariant`, `machine.effect.once`, and `machine.cancel.post` for every randomized completion order.

- [ ] **Step 5: Run stress and regression tests**

Run: `cargo test -p hoimin-core --features contracts --test machine && cargo test -p hoimin-cli --features contracts --test run_e2e --test process_handler -- --nocapture && cargo test --workspace`

Expected: PASS under repeated runs with `--jobs 1` and `--jobs 4`; status sets are identical and no resource cap scales with job count.

- [ ] **Step 6: Commit**

```text
git add crates/hoimin-core/src/machine.rs crates/hoimin-core/tests/machine.rs crates/hoimin-cli/src crates/hoimin-cli/tests
git commit -m "feat: schedule bounded parallel mutation runs"
```

## Milestone 4: Distribution and platform acceptance

### Task 14: Document and lock the public CLI contract

**Files:**
- Modify: `README.md`
- Create: `docs/json-schema/run-result.schema.json`
- Create: `docs/json-schema/run-event.schema.json`
- Modify: `crates/hoimin-cli/tests/report_handler.rs`, `crates/hoimin-cli/tests/run_e2e.rs`

**Interfaces:**
- Consumes: emitted JSON/JSONL records, CLI help, exit policy, and safety semantics from previous tasks.
- Produces: versioned public JSON Schemas and executable README examples; internal Event/Effect and contract identifiers remain implementation details.

- [ ] **Step 1: Write failing schema-validation and README command tests**

Extract every fenced `hoimin run` command from README into a temporary fixture invocation and validate sample JSON/JSONL output against the checked-in schemas.

Run: `cargo test -p hoimin-cli --test report_handler documentation_contract`

Expected: FAIL because schemas and complete usage examples are absent.

- [ ] **Step 2: Write concise install/usage/safety documentation**

Document `uvx hoimin` and `pipx run hoimin`, every selector and combination rule, defaults, direct argv semantics, operator set, statuses, score, exit codes, sessions/resume, hard/best-effort resource modes, symlink behavior, and the explicit non-security-boundary warning. Include pytest and unittest examples but no framework-specific configuration.

- [ ] **Step 3: Add versioned JSON Schemas and validate emitted data**

Set `additionalProperties` deliberately for each object and make schema version, resource mode, truncation counts, null score, and all event/status variants explicit.

Run: `cargo test -p hoimin-cli --test report_handler documentation_contract && cargo test -p hoimin-cli --test run_e2e`

Expected: PASS; README commands parse and generated output validates.

- [ ] **Step 4: Commit**

```text
git add README.md docs/json-schema crates/hoimin-cli/tests/report_handler.rs crates/hoimin-cli/tests/run_e2e.rs
git commit -m "docs: define hoimin CLI and output contracts"
```

### Task 15: Package native wheels and run the release matrix

**Files:**
- Modify: `Cargo.toml`, `pyproject.toml`, `crates/hoimin-core/Cargo.toml`, `crates/hoimin-cli/Cargo.toml`
- Create: `.github/workflows/ci.yml`, `.github/workflows/release.yml`
- Create: `tests/wheel_smoke.py`
- Modify: `README.md`
- Test: `crates/hoimin-cli/tests/run_e2e.rs`, `tests/wheel_smoke.py`

**Interfaces:**
- Consumes: the `hoimin-cli` binary, embedded helper, `contracts` feature, and all workspace test suites.
- Produces: contract-free Windows/Linux x86-64 wheels and CI jobs that separately verify default behavior, contract violations, core dependency purity, and platform I/O handlers.

- [ ] **Step 1: Add a failing installed-wheel smoke test**

Build a wheel, install it into an empty Python environment, run `hoimin --version`, then run the basic fixture using the wheel's embedded analyzer while the repository checkout is not on `PYTHONPATH`.

Run: `uv run maturin build --release && uv run pytest -q tests/wheel_smoke.py`

Expected: FAIL until wheel metadata and embedded-resource lookup are complete.

- [ ] **Step 2: Configure Maturin binary wheels**

Declare the Rust binary binding with manifest path `crates/hoimin-cli/Cargo.toml`, package name `hoimin`, `requires-python = ">=3.12,<3.15"`, and runtime dependency `libcst>=1.8.6,<2`. Keep `contracts` out of default features and build release wheels without it. Include license/readme metadata; do not expose a Python extension module. Verify the wheel contains the platform executable and can materialize the embedded helper.

- [ ] **Step 3: Add the CI matrix**

On `windows-latest` and `ubuntu-latest`, run default `cargo test --workspace`, then contract-enabled tests for both crates. Run `cargo tree -p hoimin-core --edges normal` and fail if it contains Tokio, rusqlite, tempfile, windows-sys, libc, or `hoimin-cli`. Run Python helper tests for 3.12/3.13/3.14, release wheel build, installed-wheel pytest/unittest smoke tests, timeout/output/reset/Ctrl+C/resume tests, and platform resource-backend tests. Run Linux hard-cgroup tests in a delegated cgroup v2 job and best-effort tests separately.

- [ ] **Step 4: Add release workflow and local acceptance commands**

Release workflow builds x86-64 Windows and Linux wheels with Maturin, verifies them in clean environments, uploads artifacts, and publishes to PyPI only from an approved tagged environment.

Run:

```text
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace
cargo test -p hoimin-core --features contracts
cargo test -p hoimin-cli --features contracts
cargo tree -p hoimin-core --edges normal
uv run --python 3.12 pytest python/tests -q
uv run --python 3.13 pytest python/tests -q
uv run --python 3.14 pytest python/tests -q
uv run maturin build --release
uv run pytest tests/wheel_smoke.py -q
```

Expected: every command PASS on its supported host; wheel smoke tests do not read analyzer files from the checkout.

- [ ] **Step 5: Commit**

```text
git add Cargo.toml pyproject.toml crates/hoimin-core/Cargo.toml crates/hoimin-cli/Cargo.toml README.md .github tests/wheel_smoke.py
git commit -m "build: publish verified native wheels"
```

## Final Acceptance Checklist

- [ ] Run the complete Windows matrix with Python 3.12, 3.13, and 3.14; confirm Job Object mode is `hard` and descendants are gone after timeout/Ctrl+C.
- [ ] Run the complete Linux matrix in both delegated-cgroup and explicit best-effort environments; confirm the reported mode matches enforcement.
- [ ] Run a 10,001-candidate fixture and confirm analysis stops before every mutant with exit `4` and bounded memory.
- [ ] Run core table/property tests with `contracts` enabled and confirm invalid transitions, duplicate EffectIds, reset mismatches, budget violations, and sequence violations panic with stable identifiers.
- [ ] Build the release wheel without `contracts` and confirm contract condition/context expressions are not evaluated.
- [ ] Confirm `cargo tree -p hoimin-core --edges normal` contains no I/O, async runtime, SQLite, or OS resource dependency.
- [ ] Run more than 100 candidates with default limits and confirm exactly 100 execute in stable order while the remainder are `not_run` and exit is `4`.
- [ ] Run parallel memory/process/copy stress tests and confirm `--jobs 4` does not multiply any run-wide cap.
- [ ] Interrupt a session, resume it, and confirm only killed/survived mutants are reused.
- [ ] Confirm pytest and unittest require no plugin and receive identical mutation semantics.
- [ ] Confirm JSON and JSONL stdout parse cleanly while all diagnostics stay on stderr.
- [ ] Confirm the original project hashes are unchanged after success, timeout, OOM, infrastructure failure, and Ctrl+C.
