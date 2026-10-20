# Issue #1027: strict current report wire acceptance

## Scope and result

The six event variants now consume their `kind` discriminator through the same raw-preserving map envelope before decoding closed payloads. Unknown fields are rejected in schema-closed event, candidate, span, output, count, version, resource, disk, cleanup, capability, secondary-evidence and resume objects. Existing closed outcome, janitor and sampling decoders remain in use. Progress validates recognized closed configuration descendants (`environment`, `session`, `session.config`, `selection.changed`) without requiring a complete executable configuration or closing the schema-open outer configuration and selection objects. Review still uses its existing full `RunConfig` validation.

Duplicate keys reach typed deserializers unchanged; production decoding does not construct `serde_json::Value` first. A raw event discriminator probe checks duplicate `kind` keys, and the map envelope removes only that checked discriminator. Nested `RawValue` fields remain attached to the original JSON parser. Public API enum unit variants retain their existing representation; empty struct wire variants enforce closure for `portable_guard` and `resumed`.

The review usage text now states schema v9. No compatibility path or schema migration behavior was added. Candidate identity, derived status, run/event sequence and summary checks were left intact.

## Design review passes

1. **Schema inventory:** inspected every `additionalProperties: false` location in `run-event.schema.json`, separating the open outer config/selection/verification metadata from closed children. Existing janitor, outcome, duration, sampling, session and environment validators already enforce their owned boundaries.
2. **Discriminator ownership:** inspected all six `OutputEvent` branches. Five decoded the unconsumed `kind` as part of a permissive payload; the sixth already used an internally tagged envelope. A blanket attribute change would reject valid events, so every branch now uses one discriminator-consuming boundary.
3. **Duplicate preservation:** traced raw bytes through dispatch, JSON document decoding, JSONL first-line decoding, subsequent events and review. The new envelope forwards `MapAccess::next_value_seed` directly rather than constructing a map. Unknown configuration layout stays opaque while known closed descendants are typed before any Value conversion.
4. **Nested raw values:** inspected `DiskFilesystemReport`'s raw nullable integer fields. A normal internally tagged Serde enum buffers through `Content` and cannot preserve these raw JSON fields; this ruled out a superficially shorter enum-only implementation. The chosen adapter preserves large signed deltas and null handling.
5. **Consumer policy:** checked existing opaque-config and missing-config progress regressions. Progress must accept null and arbitrary unknown object members, reject nonobjects and missing `normalized_config`, and validate recognized closed descendants when present. Review retains canonicalization, full config validation and complete passing-baseline requirements.

## Implementation review passes

1. **Root-cause RED/GREEN:** the first 24-case direct event matrix produced five expected unknown-field failures, then passed after the envelope and closure changes. Existing duplicate discriminator and payload checks remained green.
2. **Nested closure audit:** the expanded nested matrix found that `deny_unknown_fields` alone does not reject extra fields on internally tagged unit variants. `portable_guard` failed; a separate `resumed` regression also failed. Both now decode through empty struct wire variants. Struct variants and registered aggregate evidence checks remain unchanged.
3. **Configuration boundary:** replaced progress's Value config with a map-or-null visitor and partial typed descendants. Unknown fields in open objects are ignored by policy. Reused `EnvironmentSnapshot`, `SessionMode`, `ChangedSelection`, `VerificationSelection` and `MutationOutcome` validators rather than duplicating their invariants.
4. **Required-field audit:** an initial `Option<ProgressConfig>` representation allowed absent `normalized_config`. The existing `input_rejects_a_missing_normalized_config` test caught it. A concrete custom type accepts explicit null while preserving required-field rejection. The progress suite then passed all 144 tests.
5. **Final diff and mutation review:** checked discriminator handling for leading/trailing/escaped keys, duplicate payloads before and after tags, nested filesystem raw values, status projection, and shared JSON/JSONL paths. Added independent raw-key mutations and positive controls rather than using Value insertion for duplicate cases. No producer format, ID, status or sequence logic changed.

## Verification review passes

1. **All variants and tag positions:** `cargo test -p hoimin-core --test report_closed_wire --test report_status --test report_resume --test janitor_report --test report_policy --test lean_report_sequence_oracle --quiet` passed: 140 + 119 + 32 + 4 + 6 + 4 tests. The new suite covers valid producer serialization roundtrips, unknown fields, duplicate known fields, duplicate/escaped discriminators, tags after payloads, all nested closed report objects, both disk enforcement struct variants and the portable unit, fresh/resumed outcomes, sampling/reused outcomes, and a raw integer delta above i64.
2. **Shared reader consumers:** `cargo test -p hoimin-cli --lib closed_wire_tests --quiet` passed the initial 75-case matrix, then 93 after adding every JSONL event variant's valid/duplicate/tag cases. The final run passed 108 cases after adding fifteen review-duplicate cases. JSONL diagnostics and starts are validated even though they are not retained in the usable report.
3. **Public exit behavior and existing invariants:** `cargo test -p hoimin-cli --test artifact_inputs --test review_cli --test progress --quiet` passed 25 + 28 + 144 tests. Added CLI checks require exit 2 and empty stdout for unknown fields in each JSONL event and the JSON/review payloads. Existing progress tests include ID/status/sequence/summary rejection, unsupported schemas, opaque configs, incomplete classifications and actual producer JSON/JSONL reads.
4. **Producer and schema contract:** `cargo test -p hoimin-cli --test report_handler --quiet` passed all 146 tests, including golden current report/event schema validation and producer report handling. Python Draft 2020-12 schema checks and the whole workspace run are owned by the parent task and must be reported separately with their actual results.
5. **Final hygiene:** owned Rust files were formatted with rustfmt. The final 108 shared-reader cases passed after the last changes; `cargo clippy -p hoimin-core --all-targets -- -D warnings` also passed. Whole-workspace formatting, clippy and tests are the parent's integration gate; this report does not infer those outcomes from focused checks.

## Observed failures and limits

- The whole workspace run exposed a test that decoded ranked plan rows directly
  as `MutationCandidate`, relying on ignored `rank`, `score`, and ranking fields.
  It now decodes `PlanManifest` and compares each row's typed `candidate` member.
  The focused normalized-selector comparison passed; the closed report candidate
  contract remains unchanged.

- Expected TDD failures: five permissive event variants; the portable enforcement unit; resumed outcome unit.
- A real implementation regression was caught and corrected: absent `normalized_config` acceptance through an Option type.
- Two early CLI attempts could not compile during simultaneous runtime edits (`resolve_config_controlled` / `prepare_shell_setup_controlled` not yet available; subsequently a changed janitor helper arity). These were transient shared-worktree integration states; later focused CLI runs passed.
- This change aligns closed-field and duplicate-key acceptance. It does not claim a general proof that every JSON Schema numeric, required-property or semantic constraint equals every consumer's acceptance policy. The schema-open parts stay open, and the deliberate opaque progress configuration contract remains.
