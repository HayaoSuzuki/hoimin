# Task 3 (#1004): checked normalized configuration

Base: `6d78789eca05fad577b2a8dcce83077f9389ee06`.
Commit: `8720b41a0d08f375d98bda7cfa6202be779f3226`
(`fix(config): enforce immutable checked execution settings`).
Architecture: controller-approved `task-3-design.md`, ruling R6.
Implementation, focused behavioral verification and targeted lint are complete.
The controller owns the independent review gate before Task 4 begins.

## Result and transition map

`RunConfig<E>` now privately owns `RunConfigData<E>`. Checked construction and
typed deserialization enforce the existing selection, import-root, operator,
argv, and limits rules. Shared `Deref` preserves field reads; no mutable accessor,
`DerefMut`, unchecked constructor, or test bypass exists. Arbitrary edits consume
`into_data()` and require `TryFrom` before execution. The private wrapper has two
construction sites: validated data acceptance and the typed environment
transition, which preserves common fields and checks the snapshot's names.

```text
RawRunConfig -- normalize + checked TryFrom --> RunConfig<EnvironmentRequest>
RunConfigData<E> -- checked TryFrom ---------> RunConfig<E>
original serde map -- data + checked TryFrom -> RunConfig<E>
checked request -- capture + with_environment -> RunConfig<EnvironmentSnapshot>
checked snapshot -- fingerprint preparation --> checked snapshot with records
checked snapshot -- into_plan_config --------> mutable persisted PlanConfig
PlanConfig -- fallible into_run_config ------> checked snapshot, sessions disabled
checked snapshot ---------------------------> RunState / FingerprintInput
```

Strict review retains the typed configuration decoder and now rejects invalid
semantics before root canonicalization. Progress still decodes opaque object/null
configuration. No saved shape, schema version, fingerprint encoding, candidate
identity, exit policy, or production Creusot code changed.

The guarantee covers normalized semantic validity, not input freshness,
canonical project paths, output destination safety, or successful discovery.
Current preparation and verification owners retain those responsibilities.

## Downstream Rust migration

- Read through `&RunConfig` as before. To change a semantic field, consume
  `into_data()`, edit the data, and call `RunConfig::try_from`.
- `PlanConfig::into_run_config(output)` now returns
  `Result<RunConfig, ConfigError>`. Existing header checks keep their order;
  direct Rust conversion rejects malformed data without requiring a CLI header.
- `with_environment` retains the request/snapshot type transition and exact name
  check. Execution and fingerprint consumers still require snapshots.
- `with_output(output)` and `with_fingerprint_inputs(records)` consume and return
  the checked owner. They replace fields outside the existing semantic validator;
  neither certifies destination safety or fingerprint freshness.
- `VerifiedPlan::with_metrics` retains its narrow public override. Session
  preparation returns resolved path plus exclusions and no longer mutates its
  local config. Worker-root fingerprint rechecks edit owned data and check it
  before resolving inputs.
- Tests that constructed invalid checked configs now edit data or serialized
  input and assert failed acceptance. Field-moving assertions use shared borrows
  or explicit data extraction. No test-only unchecked constructor was added.

Task 4 can use the consuming output operation but must decide how runtime
verbosity relates to `OutputConfig` serialized inside both RunConfig and
PlanConfig. This task adds no verbosity setting and keeps report schema 8.

## Five requirements/design review passes

1. **Entry points:** mapped raw conversion, normalized construction, serde,
   plan conversion, preparation, machine and fingerprint consumers. Added the
   fallible plan boundary because mutable persisted data can bypass CLI headers.
2. **Immutability:** inspected nested collection/operator mutators and found the
   additional session-path and worker-root writes. Kept one immutable read view;
   migrated local derived session data instead of exposing a session setter.
3. **Ordering:** checked strict report decode before canonicalization and checked
   preparation input before capture/fingerprint work. R6 explicitly approved
   construction-to-preparation tests instead of an unchecked execution overload.
4. **Wire consumers:** retained original-map deserialization so duplicate known
   keys remain visible. Kept PlanConfig header error precedence and progress's
   distinct opaque consumer.
5. **API size:** compared wrapper, individual getters and a second checked type.
   Selected the existing RunConfig name with one construction-data owner and two
   current-purpose update operations. Flagged Task 4's saved-output consequences.

Full design findings and source anchors are in `task-3-design.md`.

## Five implementation review passes

1. **Unchecked construction search:** searched RunConfig construction and impl
   sites. Only `TryFrom<RunConfigData<E>>` and `with_environment` construct the
   private wrapper. Raw normalization delegates to checked TryFrom; its existing
   early raw diagnostics remain in place. Plan conversion checks discovery and
   then enters the same normalized-data boundary.
2. **Post-check writes:** searched field assignments, collection methods and
   mutable config borrows across core and CLI. Production writes now occur on
   construction data or inside the two narrow consuming operations. The session
   helper returns its resolved path; the preflight completion carries that path
   through the existing handoff. No mutable escape exists through shared Deref.
3. **Environment and preparation ownership:** retained private snapshot metadata,
   exact request-name checks, snapshot-only machine/fingerprint signatures,
   saved-plan recapture, and source/fingerprint ownership. Worker-root changes
   recheck the data before filesystem resolution. Existing environment and
   fingerprint tests cover the retained bytes and late input changes.
4. **Serde/error handling:** inspected direct data Deserialize followed by
   semantic TryFrom. Existing duplicate tests pass, including escaped equivalent
   jobs keys. Raised that property fixture's process cap to 256 so both generated
   jobs values remain valid in isolation. Semantic errors retain their detail
   through the existing report parse/error path.
5. **Cross-crate API/test complexity:** migrated actual construction/update sites
   and fallible plan calls without a getter family or generic update closure.
   Review found a metrics expectation that reused the new output-update method;
   changed it to independent construction data to retain fault sensitivity.
   Core and CLI API docs state the guarantee and filesystem limits.

## RED evidence

All initial regressions compiled and failed for the intended behavior before
implementation:

| Command scope | Observed failure | Log |
| --- | --- | --- |
| CLI `--test checked_config` | Six malformed inputs reached missing-fingerprint I/O; valid control passed | `/tmp/hoimin-priority-six-task3-red-preparation.log` |
| CLI `--lib artifacts::run_report::tests::review_` | Six malformed configs reached missing-project canonicalization; five existing/control tests passed | `/tmp/hoimin-priority-six-task3-red-review.log` |
| Core `--doc config::RunConfig` | Both nested mutation snippets compiled despite `compile_fail`; request/snapshot compile-fail control passed | `/tmp/hoimin-priority-six-task3-red-doc.log` |

Each negative suite has separately generated cases for jobs 257, jobs 65 with
max_processes 64, missing selector, empty operators, empty argv and `../escape`.
The absolute-cap fixture uses max_processes 300 to isolate the cap. Preparation
and review controls prove the filesystem fixture would fail if reached. The
public review command cases also require exit 2, empty stdout and no store file.

## Five validation review passes

1. **Negative branches:** all initial regressions pass after implementation.
   Added independent direct-data and direct-plan-conversion cases, including
   oversized discovery timeout, plus separate generated invalid-import-root
   cases. New cases use Arrange/Act/Assert and do not batch scenarios in loops.
2. **Independent fixtures:** failure expectations are literal errors or typed
   ConfigErrors. Poison filesystem inputs have valid controls. The metrics
   expectation uses data assignment rather than the production update method.
   Duplicate tests still manipulate raw JSON after serialization.
3. **Mutation/type guards:** the two mutation doctests changed RED to GREEN.
   Additional guards prevent construction data from entering RunState or
   fingerprint generation. Existing environment and VerifiedPlan guards pass.
4. **Valid paths:** valid-construction/serde property checks jobs, cross-field
   capacity, roots, argv and tracked/untracked snapshots. Existing config,
   machine, fingerprint, plan/verify, run, metrics and report suites pass. Exact
   report/fingerprint golden checks ran through those suites.
5. **Opaque progress:** all 128 progress integration tests pass, including raw
   opaque configuration, future objects, null, JSON/JSONL layouts, and
   usable/unusable classification. The strict reader's semantic acceptance did
   not change progress's decoder type.

## Verification commands and results

All Cargo commands use `CARGO_BUILD_JOBS=2` and
`CARGO_TARGET_DIR=/tmp/hoimin-priority-six-target`. Process/filesystem suites use
`--test-threads=2`; the existing `.venv` points to the frozen
`/tmp/hoimin-priority-six-venv`.

- `cargo test -p hoimin-core --test checked_config --test plan_config --test config_json --test operator_selection --test session_mode --test changed_selection --test resume_policy`:
  121 passed. Log: `/tmp/hoimin-priority-six-task3-green-core.log`.
- `cargo test -p hoimin-cli --test checked_config -- --test-threads=2`:
  7 passed. Log: `/tmp/hoimin-priority-six-task3-green-preparation.log`.
- `cargo test -p hoimin-cli --lib -- --test-threads=2`:
  757 passed, 25 existing ignored. Log: `/tmp/hoimin-priority-six-task3-green-cli-lib.log`.
- `cargo test -p hoimin-cli --test plan --test cli_config --test report_handler --test progress --test run_e2e --test fingerprint_inputs --test fingerprint_env --test fingerprint_env_plan --test target_handler --test lean_environment_fingerprint_oracle -- --test-threads=2`:
  666 passed, 1 existing ignored. Log: `/tmp/hoimin-priority-six-task3-green-cli-integration.log`.
- `cargo test -p hoimin-core --lib --test machine --test lean_oracle --test lean_timeout_limit_oracle --test lean_top_budget_projection_oracle`:
  167 passed, 2 existing ignored. Log: `/tmp/hoimin-priority-six-task3-green-machine.log`.
- `cargo test -p hoimin-core -p hoimin-cli --doc`:
  9 passed. Log: `/tmp/hoimin-priority-six-task3-green-doc.log`.
- Final changed-test rechecks: the independent metrics-preservation test passed
  (1 test), and the six review-command cases passed after the assertion-style
  correction. Logs: `/tmp/hoimin-priority-six-task3-green-metrics.log` and
  `/tmp/hoimin-priority-six-task3-green-command-recheck.log`.
- `cargo clippy -p hoimin-core -p hoimin-cli --all-targets --all-features -- -D warnings`:
  passed. Log: `/tmp/hoimin-priority-six-task3-clippy.log`.
- `cargo fmt --all` and `git diff --check`: passed. Commit hooks also check
  workspace/vendored formatting and workspace/all-feature lint.
- The frozen venv's safe YAML parser checked frontmatter mappings/nonempty
  `type` values in the two edited knowledge concepts. Checked the new citation
  definitions, linked source paths and current source hashes. This was a focused
  check, not a whole-catalog validation or refreshed historical evidence.
- The commit ran mandatory workspace formatting, vendored formatting,
  workspace all-target/all-feature Clippy, and vendored Clippy hooks: all passed
  with the shared target and two-job limit. Log:
  `/tmp/hoimin-priority-six-task3-commit.log`. Post-commit status is clean.

Migration compile checks exposed expected field writes/moves and unhandled
fallible plan calls; all were adapted. One new test initially referred to
`NonZeroDuration` through the wrong crate; corrected it to the existing
`hoimin_budget_math` owner. Initial Clippy found one empty-vector assertion
style violation; changed it to an assertion that prints the value on failure.
The first lint output is retained at
`/tmp/hoimin-priority-six-task3-clippy-first.log`.

The final software-design assessment for this boundary is 8/8 diagnostics,
10/10: each owner has one purpose, read and acceptance interfaces hide validation
details, common rules can change in config without handler changes, comments
state the guarantee, five design reviews examined complexity, config/environment
owners retain their decisions, the transition map explains the boundaries, and
the task included design work before implementation. This is an architectural
assessment, not a proof of runtime correctness. The initial 5/8 assessment and
the three corrected weaknesses remain recorded in the design report.

These are native macOS results. They do not establish native Linux/Windows
execution or rerun Creusot proofs. Final whole-workspace integration, combined
schema/release work and catalog metadata reconciliation remain with the
controller. Historical source hashes and verification claims were not refreshed.
