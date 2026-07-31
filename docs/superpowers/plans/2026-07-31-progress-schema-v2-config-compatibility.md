# Progress Schema-v2 Config Compatibility Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `hoimin progress` accept every schema-version-2 run report whose `normalized_config` obeys the published additive object contract, including reports emitted by the first schema-v2 implementation.

**Architecture:** Keep the core `RunConfig` strict because it is an executable configuration used by run, plan, verify, and resume paths. At the progress input boundary, replace only the typed `run_started` event with a progress-specific projection that retains the event metadata progress validates and treats `normalized_config` as an opaque JSON value constrained to `null` or object; baseline, mutant, and summary events remain the existing typed `OutputEvent` values.

**Tech Stack:** Rust, serde/serde_json, hoimin-core report types, JSON Schema draft 2020-12, cargo tests.

## Global Constraints

- Preserve `REPORT_SCHEMA_VERSION == 2`; this is a reader compatibility fix, not a schema-version bump.
- Preserve strict typed deserialization for baseline, mutant, and summary events and all existing structure checks for event kind, nested schema version, run ID, sequence order, duplicate identities, and summary counts.
- Accept `run.normalized_config` only when it is JSON `null` or an object; arrays, strings, numbers, and booleans remain invalid.
- Treat every field inside a normalized-config object as opaque to progress. Missing current fields and unknown future fields are both accepted because the published schema declares no required config properties and permits additional properties.
- Do not add serde defaults to `RunConfig`: fabricated defaults would affect execution-oriented consumers and would still require every future additive field to remember a compatibility annotation.
- Do not change progress output, exit codes, comparison semantics, `docs/json-schema/progress-result.schema.json`, or core report serialization.
- Keep `#[serde(deny_unknown_fields)]` on the outer run-result document and on the projected `run_started` payload, so extensibility remains confined to `normalized_config`.
- The oldest supported fixture is a checked-in complete report matching the `RunConfig` fields present in commit `841387a` (the initial schema-v2 implementation): it intentionally omits `operators`, `profile`, `fingerprint_includes`, `fingerprint_files`, `fingerprint_inputs`, and `output.metrics`.

## File Structure

- `crates/hoimin-cli/tests/fixtures/reports/schema-v2-original.json`: immutable compatibility fixture representing the oldest published schema-v2 normalized config.
- `crates/hoimin-cli/tests/progress.rs`: reader acceptance and negative-shape tests at the public `read_report` boundary.
- `crates/hoimin-cli/tests/report_handler.rs`: proof that the oldest fixture is valid under the repository's published run-result/run-event schemas.
- `crates/hoimin-cli/src/progress/input.rs`: progress-only `run_started` projection and existing structural validation integration.
- `docs/json-schema/run-event.schema.json`: correct the normalized-config description from “schema version 1” to “schema version 2”; no structural schema change.

---

### Task 1: Pin the oldest schema-v2 report and expose the compatibility failure

**Files:**
- Create: `crates/hoimin-cli/tests/fixtures/reports/schema-v2-original.json`
- Modify: `crates/hoimin-cli/tests/progress.rs`
- Modify: `crates/hoimin-cli/tests/report_handler.rs`

**Interfaces:**
- Consumes: `read_report(path: &Path) -> Result<InputReport, ProgressError>`
- Consumes: the existing `assert_schema_valid(result_schema, instance, event_schema)` test helper
- Produces: a stable original-schema fixture and tests that distinguish the published wire contract from current `RunConfig`

- [ ] **Step 1: Add the complete oldest-schema fixture**

Create `crates/hoimin-cli/tests/fixtures/reports/schema-v2-original.json` with the following complete report. Keep the normalized config at the exact initial schema-v2 shape; in particular, do not add fields introduced by later features.

```json
{
  "schema_version": 2,
  "run": {
    "kind": "run_started",
    "schema_version": 2,
    "sequence": 1,
    "run_id": "schema-v2-original",
    "normalized_config": {
      "root": ".",
      "selection": {
        "root": ".",
        "sources": ["src"],
        "files": ["src/example.py"],
        "lines": [],
        "symbols": [],
        "changed": false,
        "diff_base": null,
        "includes": [],
        "excludes": []
      },
      "limits": {
        "jobs": 1,
        "max_mutants": 100,
        "max_candidates": 10000,
        "analyzer_timeout": { "secs": 30, "nanos": 0 },
        "baseline_timeout": { "secs": 60, "nanos": 0 },
        "mutant_timeout": "Auto",
        "total_timeout": { "secs": 300, "nanos": 0 },
        "max_memory": 1073741824,
        "max_output": 1048576,
        "max_copy_size": 1073741824,
        "max_processes": 64
      },
      "test_argv": [{ "Unix": [112, 121, 116, 104, 111, 110] }],
      "output": { "format": "json" },
      "session": null,
      "allow_best_effort_memory": true,
      "resume": false
    },
    "versions": { "os": "fixture", "hoimin": "schema-v2-original" },
    "resource_control": { "mode": "hard", "mechanism": "fixture" }
  },
  "baseline": {
    "kind": "baseline_finished",
    "schema_version": 2,
    "sequence": 2,
    "run_id": "schema-v2-original",
    "termination": { "Exit": 0 },
    "elapsed_ms": 1,
    "resource_mode": "hard",
    "output": { "token": "baseline", "retained": 0, "observed": 0 }
  },
  "mutants": [{
    "kind": "mutant_finished",
    "schema_version": 2,
    "sequence": 3,
    "run_id": "schema-v2-original",
    "candidate": {
      "id": "mutant-1",
      "sequence": 1,
      "path": "src/example.py",
      "span": { "start": 0, "length": 1 },
      "original": "1",
      "replacement": "0",
      "operator": "integer_literal",
      "line": 1,
      "column": 0,
      "symbol": null,
      "file_hash": "fixture-hash"
    },
    "status": "killed",
    "termination": { "Exit": 1 },
    "elapsed_ms": 1,
    "resource_mode": "hard",
    "output": { "token": "mutant-1", "retained": 0, "observed": 0 }
  }],
  "summary": {
    "kind": "run_finished",
    "schema_version": 2,
    "sequence": 4,
    "run_id": "schema-v2-original",
    "counts": {
      "killed": 1,
      "survived": 0,
      "timeout": 0,
      "out_of_memory": 0,
      "process_limit": 0,
      "error": 0,
      "not_run": 0,
      "inconclusive": 0,
      "score": 1.0
    },
    "complete": true,
    "exit_code": 0
  }
}
```

- [ ] **Step 2: Add reader tests for the oldest and future-additive shapes**

In `crates/hoimin-cli/tests/progress.rs`, add a path helper and these tests near the other input tests:

```rust
fn original_schema_v2_report() -> PathBuf {
    repo_root().join("crates/hoimin-cli/tests/fixtures/reports/schema-v2-original.json")
}

#[test]
fn input_accepts_the_oldest_schema_v2_normalized_config() {
    let report = original_schema_v2_report();

    assert!(matches!(read_report(&report), Ok(InputReport::Usable(_))));
}

#[test]
fn input_accepts_an_additive_future_normalized_config_object() {
    let fixture = tempfile::tempdir().unwrap();
    let mut document = valid_report();
    document["run"]["normalized_config"] = json!({
        "future_field": { "nested": [true, 7, null] }
    });
    let report = write_json(&fixture, "future-config.json", &document);

    assert!(matches!(read_report(&report), Ok(InputReport::Usable(_))));
}
```

The future-object control deliberately contains none of the current `RunConfig` fields. That is valid under `run-event.schema.json`, whose normalized config is an object with no required properties and `additionalProperties: true`.

- [ ] **Step 3: Add negative normalized-config shape tests**

Add a table-driven test proving that leniency does not escape the schema's object-or-null boundary:

```rust
#[test]
fn input_rejects_non_object_non_null_normalized_configs() {
    let fixture = tempfile::tempdir().unwrap();
    for (name, value) in [
        ("array", json!([])),
        ("string", json!("config")),
        ("number", json!(2)),
        ("boolean", json!(true)),
    ] {
        let mut document = valid_report();
        document["run"]["normalized_config"] = value;
        let report = write_json(&fixture, &format!("{name}.json"), &document);

        assert!(read_report(&report).is_err());
    }
}

#[test]
fn input_rejects_a_missing_normalized_config() {
    let fixture = tempfile::tempdir().unwrap();
    let mut document = valid_report();
    document["run"].as_object_mut().unwrap().remove("normalized_config");
    let report = write_json(&fixture, "missing-config.json", &document);

    assert!(read_report(&report).is_err());
}
```

- [ ] **Step 4: Prove the compatibility fixture matches the published schemas**

In `crates/hoimin-cli/tests/report_handler.rs`, add this test beside the existing schema contract test. Reuse its local schema loader and validator rather than introducing another JSON Schema dependency.

```rust
#[test]
fn original_schema_v2_report_fixture_matches_the_published_schema() {
    let root = repo_root();
    let event_schema = read_schema(&root.join("docs/json-schema/run-event.schema.json"));
    let result_schema = read_schema(&root.join("docs/json-schema/run-result.schema.json"));
    let report = read_schema(
        &root.join("crates/hoimin-cli/tests/fixtures/reports/schema-v2-original.json"),
    );

    assert_schema_valid(&result_schema, &report, &event_schema);
}
```

- [ ] **Step 5: Run the focused tests and confirm the red state is specific**

Run:

```bash
cargo test -p hoimin-cli --test progress input_accepts_the_oldest_schema_v2_normalized_config -- --exact
cargo test -p hoimin-cli --test progress input_accepts_an_additive_future_normalized_config_object -- --exact
cargo test -p hoimin-cli --test progress input_rejects_non_object_non_null_normalized_configs -- --exact
cargo test -p hoimin-cli --test progress input_rejects_a_missing_normalized_config -- --exact
cargo test -p hoimin-cli --test report_handler original_schema_v2_report_fixture_matches_the_published_schema -- --exact
```

Expected before production changes:

- Both acceptance tests fail with `ProgressError::Parse` because serde tries to construct the current `RunConfig` and reports a missing current field such as `fingerprint_includes` or `fingerprint_files`.
- The scalar/array negative-shape cases pass, establishing behavior that the new projection must preserve. The missing-field case fails because serde currently defaults an absent `Option<RunConfig>` to `None`; the projection must make the schema-required member mandatory.
- The schema-contract fixture test passes, proving that the reader currently rejects a document its own published schema accepts.

- [ ] **Step 6: Commit the red compatibility fixture and tests**

```bash
git add crates/hoimin-cli/tests/fixtures/reports/schema-v2-original.json crates/hoimin-cli/tests/progress.rs crates/hoimin-cli/tests/report_handler.rs
git commit -m "test: pin original schema v2 progress report"
```

---

### Task 2: Deserialize only the run-started subset progress uses

**Files:**
- Modify: `crates/hoimin-cli/src/progress/input.rs`

**Interfaces:**
- Consumes: `ReportVersions`, `ResourceControl`, and `VerificationSelection` from `hoimin_core`
- Produces: private `ProgressRunEvent` and `ProgressRunStarted` wire projections with `schema_version()`, `sequence()`, and `run_id()` accessors
- Preserves: `RunReportDocument` typed `OutputEvent` fields for baseline, mutants, and summary

- [ ] **Step 1: Add the progress-only run-started projection**

Extend imports with `ReportVersions`, `ResourceControl`, and `VerificationSelection`. Keep using `serde_json::Value` by its qualified name so the opaque boundary is conspicuous. Add these private types above `RunReportDocument`:

```rust
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ProgressRunEvent {
    RunStarted(ProgressRunStarted),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProgressRunStarted {
    schema_version: u32,
    sequence: u64,
    run_id: String,
    normalized_config: serde_json::Value,
    #[serde(rename = "versions")]
    _versions: ReportVersions,
    #[serde(rename = "resource_control")]
    _resource_control: ResourceControl,
    #[serde(default, rename = "verification_selection")]
    _verification_selection: Option<VerificationSelection>,
}

impl ProgressRunEvent {
    fn value(&self) -> &ProgressRunStarted {
        match self {
            Self::RunStarted(value) => value,
        }
    }

    fn schema_version(&self) -> u32 {
        self.value().schema_version
    }

    fn sequence(&self) -> u64 {
        self.value().sequence
    }

    fn run_id(&self) -> &str {
        &self.value().run_id
    }
}
```

Keep `versions`, `resource_control`, and `verification_selection` typed even though progress does not read them. The explicit serde renames preserve their wire names while underscore-prefixed Rust fields document the projection and avoid dead-field warnings.

- [ ] **Step 2: Replace only the document's run field**

Change the private document projection while leaving the other event collections untouched:

```rust
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RunReportDocument {
    schema_version: u32,
    run: ProgressRunEvent,
    baseline: Option<OutputEvent>,
    mutants: Vec<OutputEvent>,
    summary: OutputEvent,
}
```

This confines wire leniency to the one field progress does not consume. Do not change `hoimin_core::RunStarted`, `RunConfig`, `PlanConfig`, or any serde defaults.

- [ ] **Step 3: Integrate the projection into version and structure validation**

In `validate_schema_versions`, replace `document.run.schema_version()` with the projection's same-named accessor. In `validate_structure`, remove the obsolete `OutputEvent::RunStarted` match (the one-variant tagged projection now enforces the kind during parsing), then validate the config envelope before any usability classification:

```rust
let started = document.run.value();
if !started.normalized_config.is_null() && !started.normalized_config.is_object() {
    return Err(invalid_structure(
        path,
        "normalized_config must be null or an object",
    ));
}
```

For cross-event validation, replace calls on the old typed run event only:

```rust
let run_id = document.run.run_id();
let mut previous_sequence = Some(document.run.sequence());
let events = document
    .baseline
    .iter()
    .chain(document.mutants.iter())
    .chain(std::iter::once(&document.summary));
for event in events {
    if event.run_id() != run_id {
        return Err(invalid_structure(
            path,
            "all present events must share run.run_id",
        ));
    }
    if previous_sequence.is_some_and(|previous| event.sequence() <= previous) {
        return Err(invalid_structure(
            path,
            "event sequences must be strictly increasing in document order",
        ));
    }
    previous_sequence = Some(event.sequence());
}
```

Starting `previous_sequence` from the projected run preserves the current strict ordering check without converting it back into a core event.

- [ ] **Step 4: Run the focused compatibility and regression suite**

Run:

```bash
cargo test -p hoimin-cli --test progress
cargo test -p hoimin-cli --test report_handler original_schema_v2_report_fixture_matches_the_published_schema -- --exact
```

Expected: all tests pass. In particular, existing tests continue to reject unsupported top-level and nested event schema versions, wrong event kinds, mismatched run IDs, non-monotonic sequences, duplicate mutant identities, and inconsistent summaries.

- [ ] **Step 5: Commit the reader projection**

```bash
git add crates/hoimin-cli/src/progress/input.rs
git commit -m "fix: read additive schema v2 progress configs"
```

---

### Task 3: Clarify the schema contract and run repository gates

**Files:**
- Modify: `docs/json-schema/run-event.schema.json`

**Interfaces:**
- Consumes: the existing schema-v2 `normalized_config` definition
- Produces: accurate schema-version wording with unchanged validation semantics

- [ ] **Step 1: Correct the normalized-config schema description**

Change only the description string under `$defs.runStarted.properties.normalized_config`:

```json
"description": "Normalized invocation config. New config fields may be added in schema version 2."
```

Retain `"type": ["object", "null"]` and `"additionalProperties": true` exactly. Do not enumerate current `RunConfig` fields or add a `required` list; doing so would contradict the additive schema-v2 contract demonstrated by the fixture.

- [ ] **Step 2: Re-run the schema and focused reader tests**

Run:

```bash
cargo test -p hoimin-cli --test report_handler original_schema_v2_report_fixture_matches_the_published_schema -- --exact
cargo test -p hoimin-cli --test progress
```

Expected: PASS.

- [ ] **Step 3: Run formatting and lint gates**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Expected: PASS with no warnings.

- [ ] **Step 4: Run the full Rust test matrix**

Run:

```bash
cargo test --workspace --all-targets --all-features
```

Expected: PASS. This covers progress CLI error mapping, report serializers, core config consumers, and contracts-enabled code paths.

- [ ] **Step 5: Review the diff for accidental compatibility broadening**

Run:

```bash
git diff --check
git diff --stat origin/main...HEAD
git diff origin/main...HEAD -- crates/hoimin-core/src/config.rs docs/json-schema/progress-result.schema.json
```

Expected: no whitespace errors; the final command is empty, proving no `RunConfig` defaults or progress-output schema changes were introduced.

- [ ] **Step 6: Commit the contract clarification**

```bash
git add docs/json-schema/run-event.schema.json
git commit -m "docs: clarify additive schema v2 config contract"
```

- [ ] **Step 7: Request review with the compatibility boundary called out**

Ask the reviewer to verify all four points:

1. the original schema-v2 fixture is valid under the published schema and accepted by progress;
2. arbitrary future fields inside a normalized-config object cannot affect progress decisions;
3. scalar/array config values and malformed event structure remain rejected;
4. no serde defaults or relaxed deserialization leaked into executable `RunConfig` consumers.
