# Focused Mutation Workflow Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a repository-local Python tool that spends at most 30 minutes ranking and running focused Rust mutations, then preserves evidence and a deterministic next-candidate queue.

**Architecture:** A thin `tools/focused_mutation.py` entry point delegates to focused modules for immutable run models, deadline accounting, Git/cargo-mutants discovery, ranking, process execution, checkpointing, and report rendering. The machine-readable `run.json` is checkpointed atomically and is the only source of truth; Markdown and command logs are derived artifacts. Real mutation execution is isolated under the requested run output directory and never writes the repository's existing `mutants.out*`.

**Tech Stack:** Python 3.14 standard library, `unittest`, Git CLI, cargo-mutants 27.1.0, Cargo.

## Global Constraints

- Keep the public hoimin CLI, JSON schemas, Rust crates, and mutation semantics unchanged.
- Keep the workflow development-only and standard-library-first.
- Default to a single 30-minute monotonic deadline: discovery/baseline ceiling 10 minutes, mutation ceiling ending five minutes before the deadline, and a five-minute reporting reserve.
- Never start a new mutation during the reporting reserve.
- Never stash, reset, checkout, delete, or directly edit tracked or untracked project source files.
- Never execute commands through a shell string; pass native argument arrays.
- Never overwrite a repository `mutants.out` or `mutants.out.*`.
- Treat `survived`, `timeout`, `unviable`, `budget_exhausted`, and unverified candidates as observable results, not proof of a bug.
- Rank deterministically and record every ranking reason plus a ranking-rule version.
- Walk at most twenty first-parent commits and stop history discovery when the queue contains ten unique candidates.
- Perform implementation in `.worktrees/focused-mutation-workflow` on `feat/focused-mutation-workflow`.

---

## File structure

- `tools/focused_mutation.py` — argument parsing, dependency wiring, exit-code mapping.
- `tools/focused_mutation_support/__init__.py` — package marker and public imports only.
- `tools/focused_mutation_support/model.py` — enums and dataclasses serialized into `run.json`.
- `tools/focused_mutation_support/budget.py` — duration parsing and monotonic stage/deadline calculations.
- `tools/focused_mutation_support/store.py` — atomic JSON checkpoints and command artifact paths.
- `tools/focused_mutation_support/discovery.py` — read-only Git and cargo-mutants inventory collection.
- `tools/focused_mutation_support/ranking.py` — versioned, deterministic candidate scoring.
- `tools/focused_mutation_support/runner.py` — native-argv subprocess lifecycle, timeouts, interruption, and command capture.
- `tools/focused_mutation_support/mutation.py` — cargo-mutants 27.1.0 command construction and outcome parsing.
- `tools/focused_mutation_support/reporting.py` — Markdown rendering from the serialized run model.
- `tests/test_focused_mutation_budget.py` — model, duration, stage, and checkpoint tests.
- `tests/test_focused_mutation_discovery.py` — fake-Git discovery and ranking tests.
- `tests/test_focused_mutation_runner.py` — fake executable, timeout, interruption, and artifact tests.
- `tests/test_focused_mutation_reporting.py` — cargo-mutants parsing, JSON/Markdown consistency, and CLI orchestration tests.
- `docs/development.md` — operator instructions, result interpretation, and evidence-run procedure.

### Task 1: Run model, budget, and atomic checkpoints

**Files:**
- Create: `tools/focused_mutation_support/__init__.py`
- Create: `tools/focused_mutation_support/model.py`
- Create: `tools/focused_mutation_support/budget.py`
- Create: `tools/focused_mutation_support/store.py`
- Create: `tests/test_focused_mutation_budget.py`

**Interfaces:**
- Produces: `RunState`, `CandidateState`, `RankingReason`, `CommandRecord`, `Candidate`, `RunRecord`.
- Produces: `parse_duration(text: str) -> float`.
- Produces: `RunBudget.start(total_seconds: float, now: float) -> RunBudget`, `discovery_timeout(now: float) -> float`, `mutation_timeout(now: float) -> float`, and `may_start_mutation(now: float) -> bool`.
- Produces: `RunStore.initialize(record: RunRecord) -> None`, `checkpoint(record: RunRecord) -> None`, and `command_paths(sequence: int, label: str) -> CommandPaths`.

- [ ] **Step 1: Write failing model and duration tests**

Create `tests/test_focused_mutation_budget.py`:

```python
from pathlib import Path
import json
import tempfile
import unittest

from tools.focused_mutation_support.budget import RunBudget, parse_duration
from tools.focused_mutation_support.model import RunRecord, RunState
from tools.focused_mutation_support.store import RunStore


class BudgetTests(unittest.TestCase):
    def test_duration_accepts_positive_seconds_minutes_and_hours(self) -> None:
        self.assertEqual(parse_duration("90s"), 90.0)
        self.assertEqual(parse_duration("30m"), 1_800.0)
        self.assertEqual(parse_duration("1.5h"), 5_400.0)
        for value in ("0m", "-1s", "30", "nanm"):
            with self.subTest(value=value), self.assertRaises(ValueError):
                parse_duration(value)

    def test_default_stage_boundaries_preserve_report_reserve(self) -> None:
        budget = RunBudget.start(total_seconds=1_800.0, now=100.0)
        self.assertEqual(budget.discovery_deadline, 700.0)
        self.assertEqual(budget.mutation_deadline, 1_600.0)
        self.assertEqual(budget.deadline, 1_900.0)
        self.assertEqual(budget.discovery_timeout(650.0), 50.0)
        self.assertEqual(budget.mutation_timeout(1_500.0), 100.0)
        self.assertFalse(budget.may_start_mutation(1_600.0))


class StoreTests(unittest.TestCase):
    def test_checkpoint_atomically_replaces_versioned_run_json(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            store = RunStore(Path(directory))
            record = RunRecord.new(total_budget_seconds=1_800.0)
            store.initialize(record)
            record.state = RunState.COMPLETED
            store.checkpoint(record)
            value = json.loads((Path(directory) / "run.json").read_text())
            self.assertEqual(value["schema_version"], 1)
            self.assertEqual(value["state"], "completed")
            self.assertFalse((Path(directory) / ".run.json.tmp").exists())
```

- [ ] **Step 2: Run the focused test and verify RED**

Run:

```console
uv run --frozen python -m unittest tests.test_focused_mutation_budget -v
```

Expected: FAIL with `ModuleNotFoundError: No module named 'tools.focused_mutation_support'`.

- [ ] **Step 3: Implement typed serializable models**

Create `model.py` with string enums and dataclasses. Use this exact public shape:

```python
SCHEMA_VERSION = 1
RANKING_RULE_VERSION = 1


class RunState(StrEnum):
    RUNNING = "running"
    COMPLETED = "completed"
    BUDGET_EXHAUSTED = "budget_exhausted"
    BASELINE_FAILED = "baseline_failed"
    TOOL_UNAVAILABLE = "tool_unavailable"
    COMMAND_FAILED = "command_failed"
    INTERRUPTED = "interrupted"
    REPORT_FAILED = "report_failed"


class CandidateState(StrEnum):
    PENDING = "pending"
    KILLED = "killed"
    SURVIVED = "survived"
    TIMEOUT = "timeout"
    UNVIABLE = "unviable"
    NOT_RUN = "not_run"
    ERROR = "error"


@dataclass
class RankingReason:
    code: str
    score: int
    detail: str


@dataclass
class Candidate:
    path: str
    symbol: str
    mutant_name: str | None
    score: int = 0
    reasons: list[RankingReason] = field(default_factory=list)
    state: CandidateState = CandidateState.PENDING
    not_run_reason: str | None = None
    command_sequences: list[int] = field(default_factory=list)
    manual_classification: str | None = None


@dataclass
class CommandRecord:
    sequence: int
    label: str
    argv: list[str]
    cwd: str
    started_at: str
    ended_at: str | None = None
    elapsed_seconds: float | None = None
    exit_code: int | None = None
    timed_out: bool = False
    interrupted: bool = False
    stdout_path: str = ""
    stderr_path: str = ""


@dataclass
class RunRecord:
    schema_version: int
    ranking_rule_version: int
    state: RunState
    total_budget_seconds: float
    repository: dict[str, object]
    tools: dict[str, str]
    candidates: list[Candidate]
    commands: list[CommandRecord]
    started_at: str
    ended_at: str | None
    elapsed_seconds: float | None
    comparison: dict[str, object] | None
    error: str | None

    @classmethod
    def new(cls, total_budget_seconds: float) -> "RunRecord":
        return cls(
            schema_version=SCHEMA_VERSION,
            ranking_rule_version=RANKING_RULE_VERSION,
            state=RunState.RUNNING,
            total_budget_seconds=total_budget_seconds,
            repository={},
            tools={},
            candidates=[],
            commands=[],
            started_at=datetime.now(timezone.utc).isoformat(),
            ended_at=None,
            elapsed_seconds=None,
            comparison=None,
            error=None,
        )

    def to_dict(self) -> dict[str, object]:
        def encode(value: object) -> object:
            if isinstance(value, StrEnum):
                return value.value
            if is_dataclass(value):
                return {field.name: encode(getattr(value, field.name)) for field in fields(value)}
            if isinstance(value, list):
                return [encode(item) for item in value]
            if isinstance(value, dict):
                return {str(key): encode(item) for key, item in value.items()}
            return value
        return cast(dict[str, object], encode(self))
```

Implement `to_dict` recursively with `dataclasses.asdict`, converting every `StrEnum` to its string value.

- [ ] **Step 4: Implement budget and atomic store**

Implement `RunBudget` as a frozen dataclass. For budgets below 15 minutes, reserve one sixth for reporting and one third for discovery; otherwise cap discovery at 10 minutes and reserve exactly 5 minutes. This preserves the specified 10/15/5 split at 30 minutes while keeping custom short budgets usable:

```python
@dataclass(frozen=True)
class RunBudget:
    started: float
    discovery_deadline: float
    mutation_deadline: float
    deadline: float

    @classmethod
    def start(cls, total_seconds: float, now: float) -> "RunBudget":
        report = 300.0 if total_seconds >= 900.0 else total_seconds / 6.0
        discovery = min(600.0, total_seconds / 3.0)
        return cls(now, now + discovery, now + total_seconds - report, now + total_seconds)
```

`RunStore` must reject a non-directory existing output, reject output paths whose name starts with `mutants.out`, create `commands/`, write UTF-8 JSON with `sort_keys=True, indent=2`, `fsync` the temporary file, and atomically call `Path.replace`.

- [ ] **Step 5: Run focused tests and quality checks**

Run:

```console
uv run --frozen python -m unittest tests.test_focused_mutation_budget -v
uv run --frozen python -m compileall -q tools tests/test_focused_mutation_budget.py
```

Expected: PASS.

- [ ] **Step 6: Commit Task 1**

```bash
git add tools/focused_mutation_support tests/test_focused_mutation_budget.py
git commit -m "feat: add focused mutation run model"
```

### Task 2: Read-only discovery and deterministic ranking

**Files:**
- Create: `tools/focused_mutation_support/discovery.py`
- Create: `tools/focused_mutation_support/ranking.py`
- Create: `tests/test_focused_mutation_discovery.py`

**Interfaces:**
- Consumes: `Candidate`, `RankingReason`.
- Produces: `RepositorySnapshot`, `CommandProbe`, `discover_candidates(snapshot, explicit_files, explicit_symbols, probe) -> list[Candidate]`.
- Produces: `rank_candidates(candidates: Iterable[Candidate], snapshot: RepositorySnapshot, explicit_files: Sequence[str] = (), explicit_symbols: Sequence[str] = ()) -> list[Candidate]`.

- [ ] **Step 1: Write failing discovery and ranking tests**

Use a fake probe that records native argv and returns fixed stdout:

```python
class FakeProbe:
    def __init__(self, replies: dict[tuple[str, ...], str]):
        self.replies = replies
        self.calls: list[tuple[str, ...]] = []

    def text(self, argv: list[str]) -> str:
        key = tuple(argv)
        self.calls.append(key)
        return self.replies[key]


class DiscoveryTests(unittest.TestCase):
    def test_changed_and_explicit_targets_rank_deterministically(self) -> None:
        snapshot = RepositorySnapshot(
            root=Path("/repo"),
            head="abc",
            branch="feature",
            dirty_paths=("crates/hoimin-core/src/machine.rs",),
            base_paths=("crates/hoimin-cli/src/process/mod.rs",),
            recent_paths=(),
        )
        candidates = [
            Candidate("crates/hoimin-cli/src/process/mod.rs", "cancel", None),
            Candidate("crates/hoimin-core/src/machine.rs", "transition", None),
        ]
        ranked = rank_candidates(candidates, snapshot, explicit_symbols=("cancel",))
        self.assertEqual([item.symbol for item in ranked], ["cancel", "transition"])
        self.assertEqual(
            [reason.code for reason in ranked[0].reasons],
            ["explicit_symbol", "changed_since_base", "risk_cancellation"],
        )

    def test_history_is_first_parent_bounded_and_stops_at_ten(self) -> None:
        probe = RecordingGitProbe()
        discover_repository(Path("/repo"), "origin/main", probe)
        self.assertIn(
            ["git", "log", "--first-parent", "-20", "--name-only", "--format="],
            probe.calls,
        )
```

Also test exclusion of `tests/`, `target/`, `.worktrees/`, `.idea/`, non-`.rs` paths, deleted paths, and duplicate path/symbol pairs.

- [ ] **Step 2: Run tests and verify RED**

Run:

```console
uv run --frozen python -m unittest tests.test_focused_mutation_discovery -v
```

Expected: FAIL because `discovery` and `ranking` do not exist.

- [ ] **Step 3: Implement repository snapshot discovery**

Define:

```python
@dataclass(frozen=True)
class RepositorySnapshot:
    root: Path
    head: str
    branch: str
    dirty_paths: tuple[str, ...]
    base_paths: tuple[str, ...]
    recent_paths: tuple[str, ...]


class CommandProbe(Protocol):
    def text(self, argv: list[str]) -> str:
        raise NotImplementedError
```

Use only these read-only Git forms:

```python
["git", "rev-parse", "--show-toplevel"]
["git", "rev-parse", "HEAD"]
["git", "branch", "--show-current"]
["git", "status", "--porcelain=v1", "-z", "--untracked-files=all"]
["git", "diff", "--name-only", "-z", f"{base}...HEAD", "--", "*.rs"]
["git", "log", "--first-parent", "-20", "--name-only", "--format="]
```

Parse NUL-delimited status/diff output without a shell. Normalize repository-relative paths with `/`; reject absolute paths and `..`. Add recent paths only when explicit/current/base candidates produce fewer than ten unique candidates, and stop after ten.

- [ ] **Step 4: Implement versioned ranking**

Use named scores, not anonymous arithmetic:

```python
SCORES = {
    "explicit_symbol": 1_000,
    "explicit_file": 900,
    "dirty_worktree": 500,
    "changed_since_base": 400,
    "recent_change": 100,
    "risk_state_transition": 80,
    "risk_cancellation": 80,
    "risk_timeout": 70,
    "risk_resource": 70,
    "risk_filesystem": 60,
    "risk_session_resume": 60,
    "risk_report_completion": 60,
    "conditional_or_error_path": 30,
}
```

Risk signals must be explicit case-insensitive keyword matches against the normalized path, symbol, and cargo-mutants name. Sort reasons by descending score then code, sum them into `Candidate.score`, and sort candidates by `(-score, path, symbol, mutant_name or "")`.

- [ ] **Step 5: Run discovery tests**

Run:

```console
uv run --frozen python -m unittest tests.test_focused_mutation_discovery -v
```

Expected: PASS with no Git writes.

- [ ] **Step 6: Commit Task 2**

```bash
git add tools/focused_mutation_support/discovery.py tools/focused_mutation_support/ranking.py tests/test_focused_mutation_discovery.py
git commit -m "feat: rank focused Rust mutation targets"
```

### Task 3: Bounded native-argv command runner

**Files:**
- Create: `tools/focused_mutation_support/runner.py`
- Create: `tests/test_focused_mutation_runner.py`

**Interfaces:**
- Consumes: `CommandRecord`, `CommandPaths`, `RunStore`.
- Produces: `CommandRunner.run(argv: Sequence[str], cwd: Path, timeout: float, label: str) -> CommandRecord`.
- Produces: `CommandTimedOut(record: CommandRecord)` and `CommandInterrupted(record: CommandRecord)`.

- [ ] **Step 1: Write fake-executable lifecycle tests**

Create a temporary executable Python script in the test rather than invoking a shell:

```python
FAKE = """#!/usr/bin/env python3
import os, sys, time
print("OUT:" + "|".join(sys.argv[1:]), flush=True)
print("ERR:" + os.getcwd(), file=sys.stderr, flush=True)
if "--sleep" in sys.argv:
    time.sleep(30)
raise SystemExit(int(os.environ.get("FAKE_EXIT", "0")))
"""


class RunnerTests(unittest.TestCase):
    def test_records_native_arguments_output_and_nonzero_exit(self) -> None:
        record = self.runner(extra_env={"FAKE_EXIT": "7"}).run(
            [str(self.fake), "a b", "$(never-run)"],
            cwd=self.work,
            timeout=5.0,
            label="baseline",
        )
        self.assertEqual(record.argv[-2:], ["a b", "$(never-run)"])
        self.assertEqual(record.exit_code, 7)
        self.assertIn("OUT:a b|$(never-run)", self.stdout(record))

    def test_timeout_terminates_process_and_keeps_partial_logs(self) -> None:
        with self.assertRaises(CommandTimedOut) as caught:
            self.runner().run(
                [str(self.fake), "--sleep"],
                cwd=self.work,
                timeout=0.1,
                label="mutation",
            )
        self.assertTrue(caught.exception.record.timed_out)
        self.assertIn("OUT:--sleep", self.stdout(caught.exception.record))
```

Add an injected `popen_factory` test that raises `KeyboardInterrupt` from `wait`, verifies termination is attempted, and verifies an interrupted record is returned in `CommandInterrupted`.

- [ ] **Step 2: Run tests and verify RED**

Run:

```console
uv run --frozen python -m unittest tests.test_focused_mutation_runner -v
```

Expected: FAIL because `CommandRunner` does not exist.

- [ ] **Step 3: Implement process capture and bounded cleanup**

Open stdout/stderr artifact files before `subprocess.Popen`. Invoke:

```python
subprocess.Popen(
    list(argv),
    cwd=cwd,
    stdin=subprocess.DEVNULL,
    stdout=stdout_file,
    stderr=stderr_file,
    env=environment,
    shell=False,
    start_new_session=(os.name != "nt"),
)
```

On timeout or interruption:

- Unix: send `SIGTERM` to the child process group, wait up to two seconds, then `SIGKILL`.
- Windows: call `terminate()`, wait up to two seconds, then `kill()`.
- Always close log files and complete the command record.
- Re-raise typed exceptions carrying the record so orchestration can checkpoint it.

Use `time.monotonic` for elapsed duration and an injected UTC timestamp function for report timestamps.

- [ ] **Step 4: Run lifecycle tests**

Run:

```console
uv run --frozen python -m unittest tests.test_focused_mutation_runner -v
```

Expected: PASS and no sleeping fake process remains.

- [ ] **Step 5: Commit Task 3**

```bash
git add tools/focused_mutation_support/runner.py tests/test_focused_mutation_runner.py
git commit -m "feat: add bounded focused command runner"
```

### Task 4: cargo-mutants inventory and focused outcome parsing

**Files:**
- Create: `tools/focused_mutation_support/mutation.py`
- Modify: `tests/test_focused_mutation_reporting.py`

**Interfaces:**
- Consumes: `Candidate`, `CandidateState`, `CommandRecord`.
- Produces: `build_list_command(repository: Path, files: Sequence[str]) -> list[str]`.
- Produces: `parse_list_json(text: str) -> list[Candidate]`.
- Produces: `build_baseline_command(candidate: Candidate) -> list[str]`.
- Produces: `build_mutation_command(repository: Path, candidate: Candidate, iterate: bool) -> list[str]`.
- Produces: `classify_mutation_output(run_directory: Path, record: CommandRecord, candidate: Candidate) -> CandidateState`.

- [ ] **Step 1: Write failing cargo-mutants contract tests**

Create `tests/test_focused_mutation_reporting.py` with fixtures based on cargo-mutants 27.1.0 `--list --json` output captured from a tiny temporary crate. Assert:

```python
def test_list_command_is_workspace_json_and_narrow_files(self) -> None:
    argv = build_list_command(
        Path("/repo"),
        ["crates/hoimin-core/src/machine.rs"],
    )
    self.assertEqual(
        argv,
        [
            "cargo", "mutants", "--workspace", "--list", "--json",
            "--manifest-path", "/repo/Cargo.toml",
            "--file", "crates/hoimin-core/src/machine.rs",
        ],
    )

def test_focused_command_uses_exact_anchored_mutant_name(self) -> None:
    candidate = Candidate(
        path="crates/hoimin-core/src/machine.rs",
        symbol="RunState::accept_completion",
        mutant_name="crates/hoimin-core/src/machine.rs:324: replace guard",
    )
    argv = build_mutation_command(Path("/repo"), candidate, iterate=False)
    self.assertIn("^crates/hoimin\\-core/src/machine\\.rs:324:\\ replace\\ guard$", argv)
    self.assertNotIn("--iterate", argv)

def test_baseline_is_scoped_to_the_candidate_crate(self) -> None:
    core = Candidate("crates/hoimin-core/src/machine.rs", "transition", None)
    cli = Candidate("crates/hoimin-cli/src/process/mod.rs", "cancel", None)
    self.assertEqual(build_baseline_command(core), ["cargo", "test", "-p", "hoimin-core"])
    self.assertEqual(build_baseline_command(cli), ["cargo", "test", "-p", "hoimin-cli"])
```

Add fixture-directory tests for `caught.txt`, `missed.txt`, `timeout.txt`, and `unviable.txt`, plus missing/incomplete artifacts. Exact-name matching must prevent a result for another mutant from being accepted.

- [ ] **Step 2: Run tests and verify RED**

Run:

```console
uv run --frozen python -m unittest tests.test_focused_mutation_reporting -v
```

Expected: FAIL because `mutation` does not exist.

- [ ] **Step 3: Capture and pin the real list JSON shape**

Create a temporary minimal Rust crate under `/tmp`, then run:

```console
cargo mutants --list --json --manifest-path /tmp/<crate>/Cargo.toml
```

Copy only a minimal anonymized JSON object into the test fixture string. Assert `cargo mutants --version` equals `cargo-mutants 27.1.0` in the evidence metadata; reject other major/minor versions with `tool_unavailable` and a message explaining the supported version.

- [ ] **Step 4: Implement command construction and parsing**

Use `re.escape(candidate.mutant_name)` surrounded by `^` and `$`. Execute cargo-mutants with:

```python
[
    "cargo", "mutants", "--workspace",
    "--manifest-path", str(repository / "Cargo.toml"),
    "--file", candidate.path,
    "--re", exact_pattern,
]
```

Append `--iterate` only when the caller explicitly requests reuse. Run the command with `cwd` set to a per-candidate directory below `<output>/cargo-mutants/`; this causes cargo-mutants' `mutants.out*` artifacts to remain outside the repository. Before relying on this behavior, add a focused integration test using `--list` against the real workspace and assert no repository-root `mutants.out*` mtime or contents change.

Map `crates/<directory>/...` to `["cargo", "test", "-p", <directory>]`.
Reject candidates outside a workspace member rather than guessing a baseline.
The orchestrator runs each unique package baseline at most once per evidence run;
a failed package baseline marks that package's remaining candidates
`not_run_reason="baseline_failed"` and sets the overall state to
`baseline_failed`.

Classify by exact full mutant name in cargo-mutants result files. Precedence is `timeout`, `unviable`, `missed` as survived, `caught` as killed. Missing files, duplicates across categories, or a command failure without a recognized exact result are `ERROR`.

- [ ] **Step 5: Run parser and real list-safety tests**

Run:

```console
uv run --frozen python -m unittest tests.test_focused_mutation_reporting -v
cargo mutants --workspace --list --json --file crates/hoimin-core/src/machine.rs > /tmp/hoimin-focused-list.json
git status --short
```

Expected: tests PASS; list output is valid JSON; no new repository `mutants.out*` appears.

- [ ] **Step 6: Commit Task 4**

```bash
git add tools/focused_mutation_support/mutation.py tests/test_focused_mutation_reporting.py
git commit -m "feat: integrate focused cargo mutants runs"
```

### Task 5: End-to-end orchestration, checkpointing, and reports

**Files:**
- Create: `tools/focused_mutation_support/reporting.py`
- Create: `tools/focused_mutation.py`
- Modify: `tools/focused_mutation_support/__init__.py`
- Modify: `tests/test_focused_mutation_reporting.py`

**Interfaces:**
- Consumes: every interface produced by Tasks 1–4.
- Produces: `render_markdown(record: RunRecord) -> str`.
- Produces: `run_workflow(options: Options, dependencies: Dependencies) -> RunRecord`.
- Produces: `main(argv: Sequence[str] | None = None) -> int`.

- [ ] **Step 1: Write failing Markdown and orchestration tests**

Add:

```python
def fixture_candidate(
    symbol: str,
    state: CandidateState,
    *,
    not_run_reason: str | None = None,
) -> Candidate:
    return Candidate(
        path="crates/hoimin-core/src/machine.rs",
        symbol=symbol,
        mutant_name=f"machine.rs:1: replace {symbol}",
        score=100,
        reasons=[RankingReason("fixture", 100, "test fixture")],
        state=state,
        not_run_reason=not_run_reason,
    )


def fixture_record(
    *,
    candidates: list[Candidate],
    state: RunState,
) -> RunRecord:
    record = RunRecord.new(total_budget_seconds=1_800.0)
    record.candidates = candidates
    record.state = state
    record.repository = {"head": "abc", "dirty": False}
    record.elapsed_seconds = 12.5
    return record


def test_report_preserves_verified_unverified_and_next_order(self) -> None:
    record = fixture_record(
        candidates=[
            fixture_candidate("a", CandidateState.SURVIVED),
            fixture_candidate("b", CandidateState.NOT_RUN, not_run_reason="reporting_reserve"),
        ],
        state=RunState.BUDGET_EXHAUSTED,
    )
    markdown = render_markdown(record)
    self.assertIn("State: `budget_exhausted`", markdown)
    self.assertIn("`a` — survived", markdown)
    self.assertIn("`b` — reporting_reserve", markdown)
    self.assertLess(markdown.index("`a`"), markdown.index("`b`"))

def test_baseline_failure_checkpoints_and_skips_mutation(self) -> None:
    dependencies = FakeDependencies(baseline_exit=1)
    record = run_workflow(fixture_options(), dependencies)
    self.assertEqual(record.state, RunState.BASELINE_FAILED)
    self.assertEqual(dependencies.mutation_calls, [])
    self.assertTrue((dependencies.output / "run.json").is_file())

def test_reporting_reserve_marks_remaining_candidates(self) -> None:
    clock = FakeClock([0.0, 1_500.0, 1_600.0])
    record = run_workflow(fixture_options(), FakeDependencies(clock=clock))
    self.assertEqual(record.state, RunState.BUDGET_EXHAUSTED)
    self.assertTrue(all(
        candidate.not_run_reason == "reporting_reserve"
        for candidate in record.candidates
        if candidate.state is CandidateState.NOT_RUN
    ))
```

Also cover tool absence, malformed list JSON, timeout, `KeyboardInterrupt`, dirty repository metadata, command checkpoint ordering, `--file`/`--symbol` repetition, output-path rejection, and optional prior-inventory comparison.

- [ ] **Step 2: Run reporting tests and verify RED**

Run:

```console
uv run --frozen python -m unittest tests.test_focused_mutation_reporting -v
```

Expected: FAIL because reporting and entry-point interfaces are absent.

- [ ] **Step 3: Implement Markdown rendering**

Render fixed sections:

```markdown
# Focused mutation report

- State: `<state>`
- Elapsed: `<seconds>s` of `<budget>s`
- Commit: `<head>`
- Dirty worktree: `<yes|no>`

## Verified candidates
- `<symbol>` — `<state>` — `<path>`
## Investigation results
- `<symbol>` — `<state>` — inspect the recorded command artifacts
## Unverified candidates
- `<symbol>` — `<not_run_reason>`
## Next recommended order
1. `<symbol>` — score `<score>` — `<ranking reason codes>`
## Manual classification
- `<symbol>` — `<manual classification or unclassified>`
## Full-inventory comparison
`reduction ratio not measured`
```

Escape Markdown backticks in untrusted tool strings or render those strings in fenced blocks. Report `reduction ratio not measured` when no compatible prior inventory is supplied.

- [ ] **Step 4: Implement workflow orchestration**

Define immutable option/dependency containers:

```python
@dataclass(frozen=True)
class Options:
    repository: Path
    output: Path
    budget_seconds: float
    base: str
    files: tuple[str, ...]
    symbols: tuple[str, ...]
    iterate: bool
    prior_inventory: Path | None


@dataclass(frozen=True)
class Dependencies:
    monotonic: Callable[[], float]
    utc_now: Callable[[], datetime]
    probe: CommandProbe
    runner: CommandRunner
```

Checkpoint after metadata, ranked discovery, and every caught command result. Baseline failure stops only the affected candidate; if the failure proves the shared workspace baseline is broken, set `BASELINE_FAILED` and stop the run. Convert typed timeout/interruption exceptions to candidate/run states, mark untouched candidates with exact reasons, checkpoint, render Markdown, and set `REPORT_FAILED` only when final rendering/persistence fails.

- [ ] **Step 5: Implement CLI and exit mapping**

Use `argparse` with:

```python
parser.add_argument("--budget", default="30m")
parser.add_argument("--base", default="origin/main")
parser.add_argument("--output", type=Path, required=True)
parser.add_argument("--file", action="append", default=[])
parser.add_argument("--symbol", action="append", default=[])
parser.add_argument("--iterate", action="store_true")
parser.add_argument("--prior-inventory", type=Path)
```

Resolve the repository from `git rev-parse --show-toplevel`. Exit `0` for `completed` and `budget_exhausted`, `130` for `interrupted`, and `2` for every infrastructure/configuration failure. Survivors do not change the exit code.

- [ ] **Step 6: Run all Python workflow tests**

Run:

```console
uv run --frozen python -m unittest \
  tests.test_focused_mutation_budget \
  tests.test_focused_mutation_discovery \
  tests.test_focused_mutation_runner \
  tests.test_focused_mutation_reporting -v
uv run --frozen python -m compileall -q tools tests
```

Expected: PASS.

- [ ] **Step 7: Commit Task 5**

```bash
git add tools/focused_mutation.py tools/focused_mutation_support tests/test_focused_mutation_reporting.py
git commit -m "feat: orchestrate focused mutation evidence runs"
```

### Task 6: Developer documentation and contract protection

**Files:**
- Modify: `docs/development.md`
- Create: `tests/test_focused_mutation_docs.py`

**Interfaces:**
- Consumes: the final CLI and output contract.
- Produces: a checked operator workflow that cannot silently drift from the implementation.

- [ ] **Step 1: Write failing documentation contract test**

Create:

```python
class FocusedMutationDocumentationTests(unittest.TestCase):
    def test_development_guide_documents_bounded_workflow(self) -> None:
        text = (ROOT / "docs" / "development.md").read_text(encoding="utf-8")
        for required in (
            "tools/focused_mutation.py",
            "--budget 30m",
            "--output /tmp/hoimin-focused-run",
            "run.json",
            "report.md",
            "budget_exhausted",
            "survivor is not proof of a bug",
            "cargo mutants --workspace",
            "do not use `--iterate` for the required final inventory",
        ):
            with self.subTest(required=required):
                self.assertIn(required, text)
```

- [ ] **Step 2: Run docs test and verify RED**

Run:

```console
uv run --frozen python -m unittest tests.test_focused_mutation_docs -v
```

Expected: FAIL because the bounded workflow is not documented.

- [ ] **Step 3: Document operation and interpretation**

Add a “Focused 30-minute Rust mutation workflow” section to `docs/development.md`. Include:

```console
output_dir="$(mktemp -d /tmp/hoimin-focused-run.XXXXXX)"
uv run --frozen python tools/focused_mutation.py \
  --budget 30m \
  --base origin/main \
  --output "$output_dir"
```

Explain explicit `--file`/`--symbol`, `--iterate`, the five-minute reserve, exit codes, partial reports, manual survivor classification, prior inventory comparison, and the distinction between focused evidence and the required non-iterated final full inventory.

- [ ] **Step 4: Run documentation and full normal tests**

Run:

```console
uv run --frozen python -m unittest tests.test_focused_mutation_docs -v
uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v
cargo test --workspace
```

Expected: PASS.

- [ ] **Step 5: Commit Task 6**

```bash
git add docs/development.md tests/test_focused_mutation_docs.py
git commit -m "docs: explain focused mutation evidence runs"
```

### Task 7: One bounded evidence run and final verification

**Files:**
- Modify only if evidence exposes a workflow defect: files created in Tasks 1–6.
- Do not add the generated run directory to Git.

**Interfaces:**
- Consumes: completed development tool and documentation.
- Produces: external evidence under `/tmp`, a final verification record in the implementation handoff, and no repository mutation artifacts.

- [ ] **Step 1: Record pre-run repository state**

Run:

```console
git status --short
find . -maxdepth 1 -name 'mutants.out*' -print
cargo mutants --version
```

Expected: only the ignored/untracked worktree `.venv` link may appear; no new mutation output belongs to this task.

- [ ] **Step 2: Run the 30-minute evidence workflow**

Run:

```console
output_dir="$(mktemp -d /tmp/hoimin-focused-run.XXXXXX)"
/usr/bin/time -p uv run --frozen python tools/focused_mutation.py \
  --budget 30m \
  --base origin/main \
  --output "$output_dir"
```

Expected: exit `0` for `completed` or `budget_exhausted`; `$output_dir/run.json`, `$output_dir/report.md`, and command artifacts exist. If it exits `2` or `130`, diagnose the workflow defect before proceeding.

- [ ] **Step 3: Validate evidence invariants**

Run:

```console
uv run --frozen python -c '
import json, pathlib, sys
p = pathlib.Path(sys.argv[1])
r = json.loads((p / "run.json").read_text())
assert r["schema_version"] == 1
assert r["state"] in {"completed", "budget_exhausted"}
assert r["candidates"]
assert all(c["reasons"] for c in r["candidates"])
assert all(c["state"] != "pending" for c in r["candidates"])
assert (p / "report.md").is_file()
' "$output_dir"
git status --short
find . -maxdepth 1 -name 'mutants.out*' -print
```

Expected: assertions pass; source status is unchanged from Step 1; no repository-root mutation output is created.

- [ ] **Step 4: Repair only demonstrated workflow defects test-first**

If Step 2 or 3 fails, add a focused regression to the matching `tests/test_focused_mutation_*.py`, run it to observe RED, make the smallest workflow fix, rerun the focused test, and repeat Steps 2–3 with a fresh output directory. Do not change hoimin production Rust behavior in this task.

- [ ] **Step 5: Run final quality gates**

Run:

```console
git diff --check
uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: all commands PASS.

- [ ] **Step 6: Commit any evidence-driven workflow fixes**

If Step 4 changed tracked files:

```bash
git add tools tests docs/development.md
git commit -m "fix: harden focused mutation evidence run"
```

If no tracked files changed, do not create an empty commit.

- [ ] **Step 7: Prepare the implementation handoff**

Report:

- evidence run directory;
- total elapsed time and terminal state;
- verified candidate count and outcomes;
- unverified candidate count and reasons;
- first five next recommended candidates;
- whether a prior full inventory allowed a reduction comparison;
- final test commands and results;
- confirmation that repository source and `mutants.out*` were untouched.
