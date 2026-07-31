# Verification Selection Policy Schema Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every populated `verification_selection` emitted by JSON and JSONL reports conform to the published schemas by declaring its required, typed `policy` field and adding regression coverage for both output contracts.

**Architecture:** Keep the Rust report model and producer output unchanged; they already serialize a non-optional `VerificationSelectionPolicy`. Correct the shared `verificationSelection` definition in `run-event.schema.json`, which also governs `run-result.schema.json` through its existing `$ref`s, and strengthen the programmatic documentation fixture so both a JSON result and the corresponding JSONL lifecycle events carry and validate the same populated selection.

**Tech Stack:** Rust, Serde/serde_json, JSON Schema draft 2020-12, Cargo integration tests

## Global Constraints

- Preserve `REPORT_SCHEMA_VERSION == 2`; this is a schema correction for an already-emitted field, not a producer-shape change.
- Preserve `VerificationSelection` serialization and all selection behavior; do not change `crates/hoimin-core/src/report.rs`, CLI selection logic, or report writers.
- A populated `verification_selection` must require `policy`; the outer `verification_selection` property remains optional for ordinary `hoimin run` reports.
- `policy` accepts exactly `explicit_candidates`, `strict`, and `file_round_robin_v1`, matching `VerificationSelectionPolicy`'s `snake_case` Serde representation.
- Keep `additionalProperties: false` on `verificationSelection`.
- Do not duplicate the definition in `run-result.schema.json`; its `run` and `summary` properties already reference `runStarted` and `runFinished` from `run-event.schema.json`.
- Do not add a new dependency or replace the repository's focused schema validator in this bug fix.

---

## File Map

| File | Responsibility | Planned change |
| --- | --- | --- |
| `crates/hoimin-cli/tests/report_handler.rs` | Produces representative JSON/JSONL reports and validates them against the checked-in schemas in `documentation_contract` | Give the shared documented fixture a populated `VerificationSelection`, explicitly prove the field reaches both event kinds and both run-result references, and reject missing/unknown policies. |
| `docs/json-schema/run-event.schema.json` | Canonical event schema and owner of the shared `$defs.verificationSelection` definition | Add required `policy` with the three serialized enum values. |
| `docs/json-schema/run-result.schema.json` | JSON document schema that references event definitions | No edit: verify its existing `runStarted`/`runFinished` references inherit the corrected definition. |

### Task 1: Lock the populated selection contract and correct the shared schema

**Files:**
- Modify: `crates/hoimin-cli/tests/report_handler.rs:49-215` — documentation/schema assertions.
- Modify: `crates/hoimin-cli/tests/report_handler.rs:316-473` — shared JSON and JSONL fixture.
- Modify: `docs/json-schema/run-event.schema.json:126-140` — shared verification-selection definition.
- Verify unchanged: `docs/json-schema/run-result.schema.json:12-33` — existing references to `runStarted` and `runFinished`.

**Interfaces:**
- Consumes: `VerificationSelection { mode, policy, requested, selected, scope, plan_truncated }` from `hoimin_core`, `actual_documented_reports() -> (serde_json::Value, Vec<serde_json::Value>)`, and the existing `assert_schema_valid` / `assert_schema_invalid` helpers.
- Produces: a schema definition in which `policy` is required and limited to the exact Serde values; a documentation fixture whose JSON `run`/`summary` and JSONL `run_started`/`run_finished` selections all exercise that definition.

- [ ] **Step 1: Add a populated verification-selection fixture used by both report formats**

In `crates/hoimin-cli/tests/report_handler.rs`, add this focused helper near `documented_events`:

```rust
fn documented_verification_selection() -> VerificationSelection {
    VerificationSelection {
        mode: VerificationSelectionMode::Top,
        policy: VerificationSelectionPolicy::FileRoundRobinV1,
        requested: 3,
        selected: 3,
        scope: VerificationSelectionScope::RetainedCandidates,
        plan_truncated: false,
    }
}
```

Build the first fixture event explicitly instead of using the unmodified minimal value:

```rust
let mut run_started = RunStarted::minimal("documented-run", 1);
run_started.verification_selection = Some(documented_verification_selection());
let mut events = vec![
    OutputEvent::RunStarted(run_started),
    OutputEvent::BaselineFinished(BaselineFinished {
        schema_version: REPORT_SCHEMA_VERSION,
        sequence: 2,
        run_id: "documented-run".to_owned(),
        termination: ProcessTermination::Exit(0),
        elapsed_ms: 10,
        resource_mode: ResourceMode::Hard,
        output: output_ref(),
    }),
];
```

Set the terminal fixture to the same populated value:

```rust
events.push(OutputEvent::RunFinished(RunSummary {
    schema_version: REPORT_SCHEMA_VERSION,
    sequence: event_sequence,
    run_id: "documented-run".to_owned(),
    counts: summary,
    complete: false,
    exit_code: 2,
    verification_selection: Some(documented_verification_selection()),
}));
```

This one typed fixture is consumed by `actual_documented_reports()` twice, so JSON serialization and JSONL serialization cannot silently diverge.

- [ ] **Step 2: Add explicit run-result and run-event policy assertions**

Immediately after `actual_documented_reports()` in `documentation_contract`, retain the existing schema-validation calls and add assertions proving all four public placements are populated:

```rust
assert_eq!(
    document["run"]["verification_selection"]["policy"],
    "file_round_robin_v1"
);
assert_eq!(
    document["summary"]["verification_selection"]["policy"],
    "file_round_robin_v1"
);
for kind in ["run_started", "run_finished"] {
    let event = jsonl_events
        .iter()
        .find(|event| event["kind"] == kind)
        .unwrap_or_else(|| panic!("missing {kind} fixture"));
    assert_eq!(
        event["verification_selection"]["policy"],
        "file_round_robin_v1"
    );
}
```

Then exercise the schema constraints rather than only the happy path. For the run-result contract, mutate the `run` selection; for the event contract, mutate `run_started`:

```rust
let mut missing_result_policy = document.clone();
missing_result_policy["run"]["verification_selection"]
    .as_object_mut()
    .unwrap()
    .remove("policy");
assert_schema_invalid(&result_schema, &missing_result_policy, &event_schema);

let mut unknown_event_policy = jsonl_events
    .iter()
    .find(|event| event["kind"] == "run_started")
    .unwrap()
    .clone();
unknown_event_policy["verification_selection"]["policy"] = serde_json::json!("weighted");
assert_schema_invalid(&event_schema, &unknown_event_policy, &event_schema);
```

These mutations cover both decisions: `required` is enforced through the run-result `$ref`, and the enum is enforced directly through the run-event schema. Keep the existing checks that the complete JSON result and every JSONL event are valid; those checks cover both `run`/`summary` and `run_started`/`run_finished` references.

- [ ] **Step 3: Run the focused contract test to verify RED**

Run:

```bash
cargo test -p hoimin-cli --test report_handler documentation_contract -- --exact
```

Expected: FAIL while validating the populated JSON run-result or JSONL event with a diagnostic ending in `unexpected property policy`. This demonstrates the fixture reproduces Issue #149 against the checked-in schema before the schema is edited.

- [ ] **Step 4: Add the required typed policy to the canonical schema**

In `docs/json-schema/run-event.schema.json`, change only `$defs.verificationSelection` so `policy` is required and enumerated alongside the existing fields:

```json
"verificationSelection": {
  "type": "object",
  "required": [
    "mode",
    "policy",
    "requested",
    "selected",
    "scope",
    "plan_truncated"
  ],
  "properties": {
    "mode": { "type": "string", "enum": ["candidate_ids", "top"] },
    "policy": {
      "type": "string",
      "enum": ["explicit_candidates", "strict", "file_round_robin_v1"]
    },
    "requested": { "type": "integer", "minimum": 1 },
    "selected": { "type": "integer", "minimum": 0 },
    "scope": {
      "type": "string",
      "enum": ["explicit_candidates", "retained_candidates"]
    },
    "plan_truncated": { "type": "boolean" }
  },
  "additionalProperties": false
}
```

Do not edit `run-result.schema.json`: its existing `run-event.schema.json#/$defs/runStarted` and `run-event.schema.json#/$defs/runFinished` references are the intended single source of truth.

- [ ] **Step 5: Run focused tests to verify GREEN**

Run:

```bash
cargo test -p hoimin-cli --test report_handler documentation_contract -- --exact
cargo test -p hoimin-cli --test report_handler
cargo test -p hoimin-core --test report_policy verification_selection_policy_uses_stable_snake_case_serialization -- --exact
```

Expected: PASS. The generated JSON result validates through both `run` and `summary` references, every generated JSONL event validates, removing `policy` from a populated selection is rejected, and an unknown policy value is rejected.

- [ ] **Step 6: Run repository quality gates**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
git diff --check
```

Expected: all commands PASS with no formatting, lint, test, or whitespace regressions.

- [ ] **Step 7: Confirm the change stayed within the schema bug boundary**

Run:

```bash
git diff -- crates/hoimin-cli/tests/report_handler.rs docs/json-schema/run-event.schema.json docs/json-schema/run-result.schema.json crates/hoimin-core/src/report.rs
```

Expected: the diff contains the populated fixture and contract assertions in `report_handler.rs` plus the shared schema correction in `run-event.schema.json`; `run-result.schema.json` and `report.rs` have no changes. Confirm the schema still declares draft 2020-12 and version `const: 2`.

- [ ] **Step 8: Commit the implementation**

```bash
git add crates/hoimin-cli/tests/report_handler.rs docs/json-schema/run-event.schema.json
git commit -m "fix: publish verification selection policy schema"
```

## Self-Review

- **Spec coverage:** The fixture reproduces populated selection output; JSON result and JSONL event contracts both validate it; `policy` is required; all three serialized enum values are declared; missing and unknown values are rejected; schema version and producer behavior remain unchanged.
- **Placeholder scan:** The plan contains exact file paths, fixture values, test mutations, schema content, commands, and expected RED/GREEN outcomes; it contains no deferred implementation steps.
- **Type consistency:** The helper returns the existing `VerificationSelection`; field and enum names exactly match `crates/hoimin-core/src/report.rs`; the schema property is the serialized snake-case `policy`; both run-result references resolve through `run-event.schema.json`.
