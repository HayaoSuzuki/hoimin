# Surface Analyzer Diagnostics in Run Implementation Plan

> **For Codex:** Use Superpowers subagent-driven development and test-driven development to execute this plan task by task.

**Goal:** Make `hoimin run` report analyzer diagnostics and finish incomplete instead of silently succeeding when selected source files cannot be analyzed.

**Architecture:** Carry transport-neutral warning records on `AnalysisFinished`. The core state machine serializes each warning through its normal `EmitOutput` effect chain before continuing analysis, preserving output ordering and report failures. Any analyzer diagnostic marks the run incomplete. The CLI analyzer maps its structured diagnostic enum and location into stable warning codes/messages; plan discovery keeps its existing structured diagnostics.

**Tech Stack:** Rust, core state machine, Ruff-backed analyzer, CLI report formats, Cargo integration tests.

---

## Task 1: Add ordered diagnostic transport to the core machine

**Files:**

- Modify: `crates/hoimin-core/src/event.rs`
- Modify: `crates/hoimin-core/src/machine.rs`
- Modify: `crates/hoimin-core/tests/machine.rs`
- Modify mechanically as needed: existing `AnalysisFinished` struct literals

### Step 1: Add a failing state-machine contract

Define the desired transport shape in the test with an analyzer completion
containing one warning:

```rust
AnalysisDiagnostic {
    code: "analyzer.invalid_syntax".to_owned(),
    message: "pkg/broken.py: source could not be parsed".to_owned(),
}
```

Drive a normal run to the analysis phase and complete analysis with an empty
final spool plus the warning. Assert:

- the next effect is `EmitOutput(OutputEvent::Diagnostic)`;
- level is `warning` and code/message are preserved;
- no final report effect is emitted before the warning acknowledgement;
- after `OutputEmitted`, finalization resumes;
- the run is incomplete and exits 4.

Run:

```console
cargo test -p hoimin-core --test machine \
  analyzer_diagnostic_is_emitted_before_incomplete_finalization -- --exact
```

Expected RED: `AnalysisFinished` cannot carry diagnostics and the machine
silently finalizes complete.

### Step 2: Add the core transport type

In `event.rs`, add:

```rust
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AnalysisDiagnostic {
    pub code: String,
    pub message: String,
}
```

Add `diagnostics: Vec<AnalysisDiagnostic>` to `AnalysisFinished`, with
`#[serde(default)]` for compatibility when deserializing older event fixtures.
Update existing struct literals to use `diagnostics: Vec::new()`.

### Step 3: Serialize warnings through the machine

Add state for:

- queued analysis diagnostics;
- the pending `AnalysisFinished` payload with diagnostics removed;
- an output action that continues the diagnostic chain.

Refactor the existing `AnalysisFinished` branch into a helper that applies the
spool/truncation result after warnings have been acknowledged.

When an analysis completion contains diagnostics:

1. mark `outcome.incomplete = true`;
2. save its spool/truncation continuation;
3. emit exactly one warning using `Diagnostic::new`;
4. on `OutputEmitted`, emit the next queued warning or resume the saved
   analysis completion.

Do not schedule the next target, candidate replay, or final report concurrently
with warning output. Report-output failure must continue to use the normal
effect failure path.

### Step 4: Verify the core task

Run:

```console
cargo fmt --all -- --check
cargo test -p hoimin-core --test machine
cargo test -p hoimin-core --features contracts
git diff --check
```

Expected: all pass.

### Step 5: Commit

```console
git add crates/hoimin-core/src/event.rs crates/hoimin-core/src/machine.rs crates/hoimin-core/tests/machine.rs
git add -u
git commit -m "core: carry analyzer diagnostics through runs"
```

## Task 2: Map CLI diagnostics and prove the real run behavior

**Files:**

- Modify: `crates/hoimin-cli/src/analyzer/mod.rs`
- Modify: `crates/hoimin-cli/tests/analyzer_handler.rs`
- Modify: `crates/hoimin-cli/tests/run_e2e.rs`
- Modify if needed: report-format contract tests

### Step 1: Add a failing analyzer-handler test

Analyze a selected source with invalid syntax and assert that
`AnalysisFinished.diagnostics` contains:

- code `analyzer.invalid_syntax`;
- a message containing the normalized source path;
- a useful default explanation when Ruff provides no message.

Run the focused test. Expected RED: diagnostics are currently discarded.

### Step 2: Map every analyzer diagnostic

Convert `AnalyzerDiagnosticCode` to stable run codes:

- `analyzer.invalid_syntax`
- `analyzer.unreconstructable_span`
- `analyzer.unparseable_replacement`
- `analyzer.candidate_limit`
- `analyzer.invalid_request`

Format the optional path, line, and column into the message, followed by the
analyzer message or a stable default explanation. Preserve all diagnostics
returned for each target.

Candidate-limit diagnostics remain warnings and continue to pair with the
existing `truncated` incomplete outcome.

### Step 3: Add a real run regression

Create an end-to-end project with a selected syntactically invalid Python file
and a baseline command that succeeds without importing it. Run in JSON or
JSONL mode and assert:

- stderr contains a diagnostic with level `warning`;
- code is `analyzer.invalid_syntax`;
- the message identifies the source path;
- the final report has `summary.complete == false`;
- process exit code is 4;
- stdout/report does not claim a complete zero-candidate success.

### Step 4: Full verification

Run:

```console
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo test -p hoimin-core --features contracts
cargo test -p hoimin-cli --features contracts
git diff --check
```

Expected: all pass.

### Step 5: Commit

```console
git add crates/hoimin-cli/src/analyzer/mod.rs crates/hoimin-cli/tests/analyzer_handler.rs crates/hoimin-cli/tests/run_e2e.rs
git commit -m "fix: report analyzer diagnostics during runs"
```

## Acceptance Checklist

- Invalid syntax in a selected run target emits a visible warning.
- Any analyzer diagnostic makes the run incomplete with exit code 4.
- Warnings are emitted before later analysis/replay/final report output.
- Multiple diagnostics preserve order and use the ordinary report failure path.
- `plan` retains its existing structured diagnostic behavior.
- Candidate-limit truncation behavior is unchanged except that its warning is visible.
- Full workspace and contracts suites pass.
