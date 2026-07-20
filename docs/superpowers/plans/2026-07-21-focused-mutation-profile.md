# Focused Mutation Profile Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an opt-in `--profile focused` mode that suppresses four low-value Python mutation contexts while preserving the default full mutation behavior.

**Architecture:** A core `MutationProfile` travels from Clap through the normalized run configuration to the analyzer and resume fingerprint. The Rust analyzer records AST-derived byte ranges for the four focused rules, then filters only legacy candidates before candidate-limit truncation. The state machine emits the normalized configuration so JSON, JSONL, and human output reveal the selected profile without a public schema-version change.

**Tech Stack:** Rust 2024, Clap, Serde, Ruff Python AST/parser 0.6.2, SQLite/rusqlite, Tokio, Cargo tests.

## Global Constraints

- `--profile` accepts only `full` and `focused`; the default is `full`.
- `full` must retain the existing candidate set, candidate IDs, order, mutant statuses, and exit-code behavior.
- `focused` filters only operators whose name does not begin with `type_`; annotation mutations remain eligible.
- Focused rules are exactly: `if __name__ == "__main__"` condition/body, bare `print(...)`, `assert`, and function default expressions.
- Apply focused filtering after existing line/symbol/operator selection and before deduplication, sorting, and `--max-candidates` truncation.
- Include the profile in the resume fingerprint; raise the fingerprint encoding schema to 3 and persist `3` in new session fingerprint rows.
- Keep run-result and run-event schema version 2. `normalized_config.profile` uses the existing extensible config object.
- Do not add coverage ingestion, test selection, probabilistic sampling, CI review annotations, user feedback collection, or SQLite table migrations.

---

## File Structure

| File | Responsibility |
| --- | --- |
| `crates/hoimin-core/src/config.rs` | Defines `MutationProfile` and carries it through raw and normalized run configuration. |
| `crates/hoimin-core/src/resume.rs` | Encodes profile in the schema-3 resume fingerprint. |
| `crates/hoimin-core/src/machine.rs` | Publishes the normalized configuration in each real `run_started` event. |
| `crates/hoimin-core/tests/target_policy.rs` | Verifies the normalized config default and focused value. |
| `crates/hoimin-core/tests/resume_policy.rs` | Verifies profile-specific fingerprint compatibility. |
| `crates/hoimin-cli/src/cli.rs` | Parses `--profile` and builds `RawRunConfig`. |
| `crates/hoimin-cli/src/analyzer/mod.rs` | Passes the selected profile into the in-process Rust analyzer. |
| `crates/hoimin-cli/src/analyzer/rust.rs` | Collects arid AST ranges and filters focused legacy candidates. |
| `crates/hoimin-cli/src/analyzer/rust_tests.rs` | Covers every focused suppression rule, exceptions, type candidates, and limit ordering. |
| `crates/hoimin-cli/src/shell.rs` | Supplies profile to analysis and fingerprint preparation; attaches runtime versions to the emitted run config. |
| `crates/hoimin-cli/src/session/mod.rs` | Stores the shared fingerprint encoding version for newly created session rows. |
| `crates/hoimin-cli/src/report/human.rs` | Shows the profile in human `run started` output when normalized config is present. |
| `crates/hoimin-cli/tests/cli_config.rs` | Covers CLI parsing/default/rejection behavior. |
| `crates/hoimin-cli/tests/run_e2e.rs` | Exercises profile-aware reports, candidate selection, and resume separation end to end. |
| `crates/hoimin-cli/tests/report_handler.rs` | Verifies human output with a real normalized profile. |
| `README.md` | Documents the opt-in profile and its deliberate false-negative trade-off. |

## Interfaces

```rust
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationProfile {
    #[default]
    Full,
    Focused,
}

impl MutationProfile {
    pub const fn as_str(self) -> &'static str;
}

pub const FINGERPRINT_SCHEMA_VERSION: u8 = 3;

pub struct FingerprintInput {
    pub sources: Vec<SourceHash>,
    pub targets: Vec<TargetSlice>,
    pub operators: Vec<String>,
    pub profile: MutationProfile,
    pub test_argv: Vec<CommandArg>,
    pub limits: RunLimits,
    pub resource_mode: ResourceMode,
}

pub(crate) struct AnalyzeRequest<'a> {
    pub path: &'a Utf8Path,
    pub lines: &'a [LineRange],
    pub symbols: &'a [String],
    pub operators: &'a MutationOperatorSelection,
    pub profile: MutationProfile,
    pub max_candidates: usize,
}
```

- `RawRunConfig::profile: MutationProfile` defaults through the enum's derived `Default`.
- `RunConfig::profile: MutationProfile` is copied from `RawRunConfig` during normalization.

### Task 1: Add the core profile model, compatibility fingerprint, and run metadata

**Files:**

- Modify: `crates/hoimin-core/src/config.rs:14-31,209-306,391-457`
- Modify: `crates/hoimin-core/src/resume.rs:1-60`
- Modify: `crates/hoimin-core/src/machine.rs:453-461`
- Modify: `crates/hoimin-core/tests/target_policy.rs:1-18`
- Modify: `crates/hoimin-core/tests/resume_policy.rs:1-20,84-120,158-186`
- Modify: `crates/hoimin-core/tests/machine.rs:1627-1691`

**Consumes:** Existing `RawRunConfig -> RunConfig` normalization, `FingerprintInput`, and `RunStarted::minimal`.

**Produces:** `MutationProfile`, normalized `RunConfig.profile`, profile-sensitive fingerprints, and a real `run_started.normalized_config` event.

- [ ] **Step 1: Write the failing core configuration and fingerprint tests**

  In `target_policy.rs`, add a test that creates `raw_config()` and asserts both raw and normalized configuration use `MutationProfile::Full`; then set `raw.profile = MutationProfile::Focused` and assert the normalized field is `Focused`.

  In `resume_policy.rs`, import `MutationProfile`, give `fixture_input()` `profile: MutationProfile::Full`, and add this test:

  ```rust
  #[test]
  fn fingerprint_changes_when_mutation_profile_changes() {
      let focused = FingerprintInput {
          profile: MutationProfile::Focused,
          ..fixture_input()
      };

      assert_ne!(fingerprint(&fixture_input()), fingerprint(&focused));
  }
  ```

  In `machine.rs`, add a focused configuration fixture and assert the first emitted `RunEffect::EmitOutput` contains `OutputEvent::RunStarted` whose `normalized_config.as_ref().unwrap().profile` is `MutationProfile::Focused`.

- [ ] **Step 2: Run the new tests and verify they fail to compile**

  Run:

  ```bash
  cargo test -p hoimin-core --test target_policy --test resume_policy --test machine
  ```

  Expected: FAIL because `MutationProfile` and the `profile` fields do not exist.

- [ ] **Step 3: Define and normalize `MutationProfile`**

  In `config.rs`, place this enum before `RawRunConfig` so both config structs can own it:

  ```rust
  #[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
  #[serde(rename_all = "snake_case")]
  pub enum MutationProfile {
      #[default]
      Full,
      Focused,
  }

  impl MutationProfile {
      #[must_use]
      pub const fn as_str(self) -> &'static str {
          match self {
              Self::Full => "full",
              Self::Focused => "focused",
          }
      }
  }
  ```

  Add `pub profile: MutationProfile` to both `RawRunConfig` and `RunConfig`. In `TryFrom<RawRunConfig> for RunConfig`, copy `raw.profile` into the returned `RunConfig`. Do not add validation: the CLI enum will already constrain command-line values, and the core enum cannot be invalid.

- [ ] **Step 4: Version and encode the resume fingerprint**

  In `resume.rs`, replace the private schema constant and extend the input:

  ```rust
  pub const FINGERPRINT_SCHEMA_VERSION: u8 = 3;

  pub struct FingerprintInput {
      pub sources: Vec<SourceHash>,
      pub targets: Vec<TargetSlice>,
      pub operators: Vec<String>,
      pub profile: MutationProfile,
      pub test_argv: Vec<CommandArg>,
      pub limits: RunLimits,
      pub resource_mode: ResourceMode,
  }
  ```

  Begin `fingerprint` with `FINGERPRINT_SCHEMA_VERSION`, and add a seventh framed field after resource mode:

  ```rust
  encoder.field(
      7,
      &[match input.profile {
          MutationProfile::Full => 0,
          MutationProfile::Focused => 1,
      }],
  );
  ```

  Import `MutationProfile` from `crate`. Keep the field tags 1 through 6 unchanged so the encoding is easy to audit; the schema byte prevents old schema-2 fingerprints from colliding with the new encoding.

- [ ] **Step 5: Emit normalized configuration from a real run**

  In `RunState::start_run_effects` in `machine.rs`, preserve the `RunStarted::minimal` helper for report-only fixtures, but attach the state’s config before emitting:

  ```rust
  let mut run_started = RunStarted::minimal(self.run_id.clone(), sequence);
  run_started.normalized_config = Some(self.config.clone());
  Ok(vec![RunEffect::EmitOutput(EmitOutput {
      id,
      event: OutputEvent::RunStarted(run_started),
  })])
  ```

  This makes `profile` visible in existing JSON/JSONL `normalized_config` without changing `REPORT_SCHEMA_VERSION`.

- [ ] **Step 6: Run the core tests and verify they pass**

  Run:

  ```bash
  cargo test -p hoimin-core --test target_policy --test resume_policy --test machine
  ```

  Expected: PASS. The profile default is `full`, a changed profile changes the fingerprint, and real `run_started` output contains the normalized profile.

- [ ] **Step 7: Commit the core model**

  ```bash
  git add crates/hoimin-core/src/config.rs crates/hoimin-core/src/resume.rs crates/hoimin-core/src/machine.rs crates/hoimin-core/tests/target_policy.rs crates/hoimin-core/tests/resume_policy.rs crates/hoimin-core/tests/machine.rs
  git commit -m "feat: add mutation profile configuration"
  ```

### Task 2: Parse and propagate `--profile`, expose it in reports, and version session rows

**Files:**

- Modify: `crates/hoimin-cli/src/cli.rs:1-112,142-180,309-379`
- Modify: `crates/hoimin-cli/src/analyzer/mod.rs:1-127,181-216`
- Modify: `crates/hoimin-cli/src/analyzer/rust.rs:1-27`
- Modify: `crates/hoimin-cli/src/shell.rs:184-214,295-299,365-369`
- Modify: `crates/hoimin-cli/src/session/mod.rs:6-18,139-155`
- Modify: `crates/hoimin-cli/src/report/human.rs:1-30`
- Modify: `crates/hoimin-cli/tests/cli_config.rs:1-35,119-170`
- Modify: `crates/hoimin-cli/tests/report_handler.rs:847-870`
- Modify: `crates/hoimin-cli/tests/session_handler.rs:11-91,445-453`

**Consumes:** `MutationProfile`, `FINGERPRINT_SCHEMA_VERSION`, and `RunConfig.profile` from Task 1.

**Produces:** A typed `--profile` CLI option, profile-bearing analyzer requests/fingerprints, session rows marked schema 3, and human output that displays the profile for live runs.

- [ ] **Step 1: Write failing CLI and human-report tests**

  In `cli_config.rs`, import `MutationProfile` and add tests for the explicit and default cases:

  ```rust
  #[test]
  fn mutation_profile_defaults_to_full_and_accepts_focused() {
      let full = hoimin_cli::cli::parse_config_from([
          "hoimin", "run", "--file", "x.py", "--", "check",
      ]).unwrap();
      assert_eq!(full.profile, MutationProfile::Full);

      let focused = hoimin_cli::cli::parse_config_from([
          "hoimin", "run", "--file", "x.py", "--profile", "focused", "--", "check",
      ]).unwrap();
      assert_eq!(focused.profile, MutationProfile::Focused);
  }

  #[test]
  fn mutation_profile_rejects_unknown_value() {
      let error = hoimin_cli::cli::parse_from([
          "hoimin", "run", "--file", "x.py", "--profile", "sampled", "--", "check",
      ]).unwrap_err();
      assert!(error.to_string().contains("sampled"));
  }
  ```

  In `report_handler.rs`, build a `RunStarted::minimal`, set its `normalized_config` to a focused fixture config, send it through `OutputFormat::Human`, and assert the exact line contains `run started: run-1 (profile: focused)`.

  In `session_handler.rs`, after `handler.begin(begin_request(1, "run-1"))`, open a read-only `rusqlite::Connection` to the same temporary database and assert:

  ```rust
  let stored_schema: i64 = connection
      .query_row("SELECT schema_version FROM fingerprints", [], |row| row.get(0))
      .unwrap();
  assert_eq!(stored_schema, i64::from(hoimin_core::FINGERPRINT_SCHEMA_VERSION));
  ```

- [ ] **Step 2: Run the CLI/report tests and verify they fail**

  Run:

  ```bash
  cargo test -p hoimin-cli --test cli_config --test report_handler --test session_handler
  ```

  Expected: FAIL because `--profile` is unknown and no human profile line is emitted.

- [ ] **Step 3: Add the Clap adapter and raw-config conversion**

  In `cli.rs`, import `MutationProfile` and add a local Clap-only enum:

  ```rust
  #[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
  enum ProfileArg {
      #[default]
      Full,
      Focused,
  }

  impl From<ProfileArg> for MutationProfile {
      fn from(value: ProfileArg) -> Self {
          match value {
              ProfileArg::Full => Self::Full,
              ProfileArg::Focused => Self::Focused,
          }
      }
  }
  ```

  Add the following option beside `--operators` in `RawRunArgs` and a `profile: ProfileArg` field in `RunArgs`:

  ```rust
  /// Candidate-selection profile.
  #[arg(long, value_enum, default_value_t = ProfileArg::Full)]
  profile: ProfileArg,
  ```

  Carry it through `TryFrom<Command> for RunArgs`, then assign `profile: args.profile.into()` in `raw_config`. Add `--profile` to the `after_help` option inventory.

- [ ] **Step 4: Thread profile through analysis and fingerprint preparation**

  Change analyzer handler signatures so every entry point has an explicit profile:

  ```rust
  pub async fn handle(
      &mut self,
      request: AnalyzeFile,
      operators: &MutationOperatorSelection,
      profile: MutationProfile,
  ) -> Result<AnalysisFinished, EffectFailed>

  pub(crate) async fn handle_with_cancellation(
      &mut self,
      request: AnalyzeFile,
      operators: &MutationOperatorSelection,
      profile: MutationProfile,
      cancellation: ProcessCancellation,
  ) -> Result<AnalysisFinished, EffectFailed>
  ```

  Add `profile` to `rust::AnalyzeRequest`, pass it into `analyze_source`, update the cancellation unit test to use `MutationProfile::Full`, and change the shell call to:

  ```rust
  .handle_with_cancellation(
      request,
      &context.config.operators,
      context.config.profile,
      cancellation.clone(),
  )
  ```

  In `prepare_fingerprint`, add `profile: context.config.profile` to `FingerprintInput`.

- [ ] **Step 5: Persist fingerprint schema 3 and display the profile for live runs**

  In `session/mod.rs`, import `FINGERPRINT_SCHEMA_VERSION` and replace the literal in `begin`:

  ```rust
  params![
      request.fingerprint.as_bytes().as_slice(),
      i64::from(FINGERPRINT_SCHEMA_VERSION),
  ]
  ```

  Do not alter `SCHEMA_VERSION` or add a database migration; only newly inserted fingerprint rows receive the new encoding version.

  In `report/human.rs`, retain the old line for fixture events with no normalized config and add the profile suffix for real runs:

  ```rust
  OutputEvent::RunStarted(value) => match value.normalized_config.as_ref() {
      Some(config) => writeln!(
          writer,
          "run started: {} (profile: {})",
          value.run_id,
          config.profile.as_str(),
      )?,
      None => writeln!(writer, "run started: {}", value.run_id)?,
  },
  ```

- [ ] **Step 6: Run the focused CLI/report test set**

  Run:

  ```bash
  cargo test -p hoimin-cli --test cli_config --test report_handler --test session_handler
  cargo test -p hoimin-cli --lib analyzer::tests
  ```

  Expected: PASS. The option parses to `Focused`, unknown values are Clap errors, report fixtures without config retain their old line, and focused config emits the profile suffix.

- [ ] **Step 7: Commit the CLI and propagation boundary**

  ```bash
  git add crates/hoimin-cli/src/cli.rs crates/hoimin-cli/src/analyzer/mod.rs crates/hoimin-cli/src/analyzer/rust.rs crates/hoimin-cli/src/shell.rs crates/hoimin-cli/src/session/mod.rs crates/hoimin-cli/src/report/human.rs crates/hoimin-cli/tests/cli_config.rs crates/hoimin-cli/tests/report_handler.rs crates/hoimin-cli/tests/session_handler.rs
  git commit -m "feat: expose focused mutation profile"
  ```

### Task 3: Implement AST arid-range collection and focused candidate filtering

**Files:**

- Modify: `crates/hoimin-cli/src/analyzer/rust.rs:1-98,219-342`
- Modify: `crates/hoimin-cli/src/analyzer/rust_tests.rs:1-57,600-640`

**Consumes:** `AnalyzeRequest.profile` from Task 2 and existing Ruff `Stmt`, `Expr`, `CmpOp`, `Ranged`, candidate, selector, sort, and truncation behavior.

**Produces:** Stable focused filtering of legacy candidates before `--max-candidates`, while full and `type_` candidates retain current behavior.

- [ ] **Step 1: Add failing focused analyzer tests and helpers**

  Extend `rust_tests.rs` with `analyze_with_profile` so tests can request either profile:

  ```rust
  fn analyze_with_profile(
      profile: MutationProfile,
      max_candidates: usize,
      source: &str,
  ) -> super::AnalyzerOutput {
      let operators = MutationOperatorSelection::default();
      analyze_source(
          &AnalyzeRequest {
              path: Utf8Path::new("pkg/sample.py"),
              lines: &[],
              symbols: &[],
              operators: &operators,
              profile,
              max_candidates,
          },
          source,
      )
  }

  fn analyze_types_with_profile(
      profile: MutationProfile,
      source: &str,
  ) -> super::AnalyzerOutput {
      let mut operators = MutationOperatorSelection::default();
      for operator in [
          MutationOperator::TypeNullableRemove,
          MutationOperator::TypeNullableAdd,
          MutationOperator::TypeListSequence,
          MutationOperator::TypeSetAbstractSet,
          MutationOperator::TypeMapping,
          MutationOperator::TypeIterableIterator,
          MutationOperator::TypeSequenceIterable,
      ] {
          operators.include(operator);
      }
      analyze_source(
          &AnalyzeRequest {
              path: Utf8Path::new("pkg/sample.py"),
              lines: &[],
              symbols: &[],
              operators: &operators,
              profile,
              max_candidates: 10_000,
          },
          source,
      )
  }
  ```

  Add these six tests with concrete assertions:

  ```rust
  #[test]
  fn focused_profile_suppresses_main_print_assert_and_defaults() {
      let source = "if __name__ == \"__main__\":\n    print(1 + 2)\n    assert 3 == 3\nelse:\n    fallback = 4 + 5\n\ndef f(flag=True, *, enabled=False):\n    return flag + enabled\n";
      let focused = analyze_with_profile(MutationProfile::Focused, 10_000, source);
      let descriptors: Vec<_> = focused.candidates.iter()
          .map(|candidate| (candidate.line, candidate.operator.as_str()))
          .collect();
      assert_eq!(descriptors, vec![(5, "binary_add_sub"), (8, "binary_add_sub")]);
  }

  #[test]
  fn focused_profile_accepts_only_exact_main_guard_shapes() {
      let source = "if \"__main__\" == __name__:\n    reversed = 1 + 2\nif __name__ != \"__main__\":\n    inequality = 3 + 4\nif __name__ == \"__main__\" == \"__main__\":\n    chained = 5 + 6\nif __name__ == \"entry\":\n    entry = 7 + 8\n";
      let focused = analyze_with_profile(MutationProfile::Focused, 10_000, source);
      assert!(focused.candidates.iter().all(|candidate| candidate.line != 2));
      for line in [3, 4, 5, 6, 7, 8] {
          assert!(
              focused.candidates.iter().any(|candidate| candidate.line == line),
              "expected an eligible candidate on line {line}",
          );
      }
  }

  #[test]
  fn focused_profile_suppresses_only_bare_print_and_assert() {
      let source = "print(1 + 2)\nlogger.print(3 + 4)\nassert 5 + 6\nregular = 7 + 8\n";
      let focused = analyze_with_profile(MutationProfile::Focused, 10_000, source);
      let descriptors: Vec<_> = focused.candidates.iter()
          .map(|candidate| (candidate.line, candidate.operator.as_str()))
          .collect();
      assert_eq!(descriptors, vec![(2, "binary_add_sub"), (4, "binary_add_sub")]);
  }

  #[test]
  fn focused_profile_retains_type_annotation_candidates() {
      let source = "from typing import Optional\n\ndef choose(value: Optional[int], enabled=True) -> Optional[int]:\n    return value\n";
      let focused = analyze_types_with_profile(MutationProfile::Focused, source);
      assert!(focused.candidates.iter().any(|candidate| candidate.operator.starts_with("type_")));
      assert!(focused.candidates.iter().all(|candidate| candidate.operator != "boolean_literal"));
  }

  #[test]
  fn focused_filter_runs_before_candidate_limit() {
      let focused = analyze_with_profile(
          MutationProfile::Focused,
          1,
          "def choose(enabled=True):\n    return 1 + 2\n",
      );
      assert_eq!(focused.candidates.len(), 1);
      assert_eq!(focused.candidates[0].line, 2);
      assert_eq!(focused.candidates[0].operator, "binary_add_sub");
  }

  #[test]
  fn focused_profile_applies_after_line_and_symbol_selection() {
      let source = "def selected(enabled=True):\n    return 1 + 2\n\ndef ignored(enabled=True):\n    return 3 + 4\n";
      let lines = vec![LineRange { start: 2, end: 2 }];
      let symbols = vec!["pkg.sample:selected".to_owned()];
      let operators = MutationOperatorSelection::default();
      let focused = analyze_source(
          &AnalyzeRequest {
              path: Utf8Path::new("pkg/sample.py"),
              lines: &lines,
              symbols: &symbols,
              operators: &operators,
              profile: MutationProfile::Focused,
              max_candidates: 10_000,
          },
          source,
      );
      let descriptors: Vec<_> = focused.candidates.iter()
          .map(|candidate| (candidate.line, candidate.operator.as_str()))
          .collect();
      assert_eq!(descriptors, vec![(2, "binary_add_sub")]);
  }
  ```

  Add `full_profile_matches_default_candidate_output` with this equality assertion:

  ```rust
  assert_eq!(
      analyze("result = first + second\n").candidates,
      analyze_with_profile(MutationProfile::Full, 10_000, "result = first + second\n").candidates,
  );
  ```

- [ ] **Step 2: Run the analyzer tests and verify they fail**

  Run:

  ```bash
  cargo test -p hoimin-cli focused_profile
  ```

  Expected: FAIL because all profiles currently retain the same candidate list.

- [ ] **Step 3: Record and normalize arid byte ranges in `AstFacts`**

  Import `CmpOp` and `MutationProfile`. Add `arid_ranges: Vec<(usize, usize)>` to `AstFacts`, plus these helpers:

  ```rust
  fn record_arid_range(&mut self, range: ruff_text_size::TextRange) {
      self.arid_ranges
          .push((usize::from(range.start()), usize::from(range.end())));
  }

  fn normalize_arid_ranges(&mut self) {
      self.arid_ranges.sort_unstable_by_key(|range| range.0);
      let mut merged = Vec::with_capacity(self.arid_ranges.len());
      for (start, end) in self.arid_ranges.drain(..) {
          if let Some((_, previous_end)) = merged.last_mut()
              && start <= *previous_end
          {
              *previous_end = (*previous_end).max(end);
          } else {
              merged.push((start, end));
          }
      }
      self.arid_ranges = merged;
  }

  fn contains_arid_span(&self, start: usize, end: usize) -> bool {
      self.arid_ranges
          .iter()
          .any(|(range_start, range_end)| *range_start <= start && end <= *range_end)
  }
  ```

  Call `normalize_arid_ranges()` at the end of `AstFacts::from_module`, after visiting every module statement.

- [ ] **Step 4: Collect precisely the four focused contexts**

  Extend `Visitor for AstFacts` without changing its existing scope and unary-operator logic:

  ```rust
  fn visit_stmt(&mut self, statement: &'ast Stmt) {
      match statement {
          Stmt::FunctionDef(definition) => {
              for parameter in definition.parameters.iter_non_variadic_params() {
                  if let Some(default) = parameter.default() {
                      self.record_arid_range(default.range());
                  }
              }
              self.visit_definition(
                  definition.name.as_str(),
                  definition.range(),
                  &definition.decorator_list,
                  statement,
              );
          }
          Stmt::If(statement_if) if is_main_guard(statement_if.test.as_ref()) => {
              self.record_arid_range(statement_if.test.range());
              for child in &statement_if.body {
                  self.record_arid_range(child.range());
              }
              visitor::walk_stmt(self, statement);
          }
          Stmt::Assert(_) => {
              self.record_arid_range(statement.range());
              visitor::walk_stmt(self, statement);
          }
          Stmt::ClassDef(definition) => self.visit_definition(
              definition.name.as_str(),
              definition.range(),
              &definition.decorator_list,
              statement,
          ),
          _ => visitor::walk_stmt(self, statement),
      }
  }
  ```

  In `visit_expr`, before the existing unary handling, record a call only when its function is `Expr::Name` with `id.as_str() == "print"`:

  ```rust
  if let Expr::Call(call) = expression
      && matches!(call.func.as_ref(), Expr::Name(name) if name.id.as_str() == "print")
  {
      self.record_arid_range(call.range());
  }
  ```

  Add `is_main_guard` and its two helpers. Require exactly one `CmpOp::Eq`, one comparator, a `Name` whose `id` is `__name__`, and an `Expr::StringLiteral` whose `value.to_str()` is `__main__`; accept either operand order:

  ```rust
  fn is_main_guard(expression: &Expr) -> bool {
      let Expr::Compare(compare) = expression else {
          return false;
      };
      if compare.ops.len() != 1
          || compare.ops[0] != CmpOp::Eq
          || compare.comparators.len() != 1
      {
          return false;
      }
      let right = &compare.comparators[0];
      (is_dunder_name(compare.left.as_ref()) && is_main_literal(right))
          || (is_main_literal(compare.left.as_ref()) && is_dunder_name(right))
  }

  fn is_dunder_name(expression: &Expr) -> bool {
      matches!(expression, Expr::Name(name) if name.id.as_str() == "__name__")
  }

  fn is_main_literal(expression: &Expr) -> bool {
      matches!(expression, Expr::StringLiteral(value) if value.value.to_str() == "__main__")
  }
  ```

  Do not add ranges for `elif_else_clauses`, attribute calls, alias imports, decorators, annotations, or function bodies.

- [ ] **Step 5: Filter only focused legacy candidates before sorting and limits**

  In `analyze_source`, after appending `type_annotation_candidates` and before the existing `seen` deduplication, add:

  ```rust
  if request.profile == MutationProfile::Focused {
      candidates.retain(|candidate| {
          if candidate.operator.starts_with("type_") {
              return true;
          }
          let Some(end) = candidate.span.start.checked_add(candidate.span.length) else {
              return true;
          };
          let (Ok(start), Ok(end)) = (
              usize::try_from(candidate.span.start),
              usize::try_from(end),
          ) else {
              return true;
          };
          !facts.contains_arid_span(start, end)
      });
  }
  ```

  Keep the existing deduplication, `(span.start, operator)` sort, and truncation code in their current order after this filter. An overflowing or non-convertible span remains eligible rather than causing an analysis failure or an accidental suppression.

- [ ] **Step 6: Run focused and full analyzer regression tests**

  Run:

  ```bash
  cargo test -p hoimin-cli focused_profile
  cargo test -p hoimin-cli --lib analyzer::rust::rust_tests
  ```

  Expected: PASS. Focused output contains only non-arid legacy candidates, type candidates survive focused filtering, and the full profile matches the old descriptor order.

- [ ] **Step 7: Commit focused AST filtering**

  ```bash
  git add crates/hoimin-cli/src/analyzer/rust.rs crates/hoimin-cli/src/analyzer/rust_tests.rs
  git commit -m "feat: suppress arid focused-profile candidates"
  ```

### Task 4: Prove live-run behavior, resume isolation, and document the feature

**Files:**

- Modify: `crates/hoimin-cli/tests/run_e2e.rs:1-30,680-760,850-960`
- Modify: `README.md:8-39,65-90,122-125`

**Consumes:** The completed CLI, analyzer, run metadata, and schema-3 fingerprint pipeline from Tasks 1–3.

**Produces:** End-to-end proof that focused reports and sessions use a distinct candidate set, plus user-facing guidance for choosing the profile.

- [ ] **Step 1: Write a failing focused-run fixture and report assertions**

  In `run_e2e.rs`, add a helper that creates `src/focused.py` with this exact source:

  ```python
  def decide(value=True):
      print(1 + 2)
      assert value is True
      return 3 + 4

  if __name__ == "__main__":
      launch = 5 + 6
  else:
      fallback = 7 + 8
  ```

  Add `focused_profile_is_reported_and_omits_arid_candidates`. Run it with `--profile focused --format json` and the direct argv test command `from src.focused import decide; assert decide() == 7`. Assert:

  ```rust
  assert_eq!(run.document["run"]["normalized_config"]["profile"], "focused");
  let lines: Vec<_> = run.document["mutants"].as_array().unwrap().iter()
      .map(|mutant| mutant["candidate"]["line"].as_u64().unwrap())
      .collect();
  assert_eq!(lines, vec![4, 9]);
  ```

  Add a second test that invokes `--format jsonl` and asserts its `run_started` record has `normalized_config.profile == "focused"`; invoke `--format human` and assert its first progress line contains `(profile: focused)`.

- [ ] **Step 2: Write a failing resume-separation test**

  Add `focused_profile_does_not_resume_full_profile_session` using the same fixture and a temporary SQLite path:

  1. Run `--profile full --session PATH --max-mutants 1` so the run remains incomplete.
  2. Run `--profile focused --session PATH --resume --max-mutants 1`.
  3. Open the database with `rusqlite::Connection` and assert `SELECT COUNT(*) FROM runs` is `2`.

  The assertion proves a changed profile starts a new incomplete run rather than reusing full-profile results.

- [ ] **Step 3: Run the new E2E tests and verify they fail before the completed feature is present**

  Run:

  ```bash
  cargo test -p hoimin-cli focused_profile_is_reported_and_omits_arid_candidates
  cargo test -p hoimin-cli focused_profile_does_not_resume_full_profile_session
  ```

  Expected: FAIL until Tasks 1–3 are integrated; before the final analyzer change, arid candidate lines appear in the focused run.

- [ ] **Step 4: Implement fixture helpers and make the E2E tests pass**

  Reuse `python_executable`, `FixtureRun`, and the existing `run_with_io` pattern. Add one helper that accepts `profile`, `format`, optional `session`, optional `resume`, and `max_mutants`, and constructs arguments in this order:

  ```rust
  [
      "hoimin", "run", "--root", root, "--source", "src", "--file", "src/focused.py",
      "--profile", profile, "--max-mutants", max_mutants, "--format", format,
      "--allow-best-effort-memory", "--", python, "-c",
      "from src.focused import decide; assert decide() == 7",
  ]
  ```

  Insert `--session PATH` and `--resume` before `--` only when requested. Keep all test commands direct argv elements; do not invoke a shell.

- [ ] **Step 5: Document profile selection in README**

  Add `--profile full|focused` to the run-option overview. Directly after the mutation operator section, add this text and command:

  ```markdown
  ## Mutation profiles

  `--profile full` is the default and considers every candidate produced by the selected
  operators. `--profile focused` suppresses candidates in Python `__main__` guards, bare
  `print(...)` calls, `assert` statements, and function default expressions. It is intended
  to reduce low-value survivors in focused change checks, but may omit useful mutants; use
  `full` when measuring the complete selected target.

  ```console
  hoimin run --profile focused --root . --source src -- python -m pytest -q
  ```
  ```

  In the sessions section, state that profile selection is part of session compatibility, so a focused run never resumes results from a full run and vice versa.

- [ ] **Step 6: Run E2E, schema, formatting, lint, and full test verification**

  Run:

  ```bash
  cargo test -p hoimin-cli --test run_e2e
  cargo test -p hoimin-cli --test report_handler
  cargo fmt --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  ```

  Expected: PASS. Existing JSON schema validation remains green because `normalized_config` permits new config fields; no schema file version changes are required.

- [ ] **Step 7: Commit integration tests and documentation**

  ```bash
  git add crates/hoimin-cli/tests/run_e2e.rs README.md
  git commit -m "docs: explain focused mutation profile"
  ```

### Task 5: Build the distributable and smoke-test the complete CLI

**Files:**

- Verify only: `Cargo.toml`, `pyproject.toml`, `tests/wheel_smoke.py`

**Consumes:** All commits from Tasks 1–4.

**Produces:** Evidence that the native wheel still packages and executes the extended CLI.

- [ ] **Step 1: Build the release wheel**

  Run:

  ```bash
  uv run maturin build --release
  ```

  Expected: PASS and create a release wheel under `target/wheels/`.

- [ ] **Step 2: Run the isolated wheel smoke test**

  Run:

  ```bash
  uv run python tests/wheel_smoke.py
  ```

  Expected: PASS. The smoke test installs the built wheel outside this checkout and runs the Rust CLI.

- [ ] **Step 3: Inspect the final worktree**

  Run:

  ```bash
  git status --short
  git log --oneline -4
  ```

  Expected: only the focused-profile implementation commits are present for this feature; preserve unrelated user files such as `.idea/` without staging or modifying them.
