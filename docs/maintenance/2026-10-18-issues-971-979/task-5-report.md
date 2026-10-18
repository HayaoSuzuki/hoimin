# Task 5 report: #971 CLI raw argument composition

Base: `4e99249c5158462ddbbafbf45afc2310256ea4e5`.
Scope: preserve syntax and behavior while eliminating common raw field declarations/copying. The controller owns the final broad suite; this task owns only its focused suites and required commit hooks. All Cargo and commit commands inherit `CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0`. Existing `.venv` is not modified.

## Requirements/design: five distinct self-reviews

1. Ownership review: the approved issue and Task 5 prepared design both permit renaming existing `RawRunArgs` to public `RunArgs`. Its private mutation member will own the parsed shared data; remove the old duplicate declaration and full-field transfer. No aliases, forwarding API, builder or framework.
2. Boundary review: retain exactly one raw-to-normalized `raw_config` and core `RawRunConfig -> RunConfig<EnvironmentRequest>` conversion; public production callers already consume the owned run value and need no extra interface.
3. Diagnostics review: share only required-input validation between run and plan; selector absence must precede argv absence, and both precede later normalization errors. Preserve duration/byte parsing and path rejection ordering.
4. Native values review: retain `Vec<OsString>` directly through command parsing until existing `command_arg` conversion. Characterize run and plan Unix bytes and Windows unpaired UTF-16 units; path and text-selector rejection remain observable behavior.
5. Scope review: run retains output/metrics/session/resume, plan uses fixed JSON/no metrics/no session/no resume and rejects those run-only options. Analyzer timeout declaration and syntax remain unchanged for Task 6. Existing field-layout tests will migrate to normalized behavior; touched multiple-scenario tests split. No controller plan or ledger changes.

## Characterization approach

This is the explicitly authorized pure refactor baseline approach, not a manufactured RED test. Before production edits, migrate public-field assertions to normalized defaults/selector behavior; split touched compound scenarios; add common-option equivalence, required-input ordering, malformed duration/byte, run-only option restrictions, and native argv/path behavior. Each has a concrete potential regression: lost common ownership field, reordered check, changed normalizer, lossy argv conversion, or leaked plan execution option. Expectations are literal normalized values or independently specified native units. Named finite cases cover commands/options without combining unrelated behaviors.

Baseline command and results will be recorded after completion.

Baseline adjustment: the first characterization attempt reported 74 pass / 1 fail because my migration tried to normalize a symbol-only request. Existing core semantics intentionally require `--source` for symbol normalization, whereas the original test exercised only parser-level selector presence. Corrected that scenario to assert the successful parsed command without accessing raw fields, retaining the separate source-plus-symbol normalization test. This was a test migration mistake, not a production regression or feature RED.

Baseline: `CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --offline --locked -p hoimin-cli --test cli_config -- --test-threads=2` exited 0: **75 passed, 0 failed** before any production code edits. Log: `/tmp/hoimin-design-audit-5d3aee95/task-5-baseline.log`. Windows-specific wide-unit branch is cfg-gated and awaits Windows CI; Unix native-byte branch executes here.

## Implementation: five distinct self-reviews

1. Declaration/dataflow review: inspected full production diff. `Command::Run` now parses directly into public `RunArgs` (existing Clap composition), with the private shared `RawMutationArgs`; old duplicate public declaration and 38-field assembly are gone. Only the small plan run-only default assembly remains.
2. Validation review: both command branches call the shared borrowed `validate_mutation_inputs` once, preserving selector-before-argv ordering; no normalization moved earlier. Existing path/session/metrics/limit/import validation order is unchanged.
3. Normalization review: `raw_config` destructures the owned composition and keeps the original conversion expressions. The only common member-name adjustments are raw singular `fingerprint_include` and `fingerprint_file` to the existing normalized plural members. The sole core conversion is still in `run_config_from_args`.
4. Interface/native review: production `lib.rs`, `PlanArgs::into_run_config`, and `parse_config_from` already consume `RunArgs` by value and need no edits. Existing run-only public fields remain public; raw common fields remain private. Test argv remains native until `command_arg`; no string/shell conversion or aliases introduced.
5. Scope/test review: analyzer timeout declarations/defaults and command syntax are unchanged. Tests use normalized defaults rather than duplicating the raw structure, split touched metrics/plan/selector scenarios, and preserve Windows expectations as literal code units. No schema, session, core implementation, generated documentation or controller files changed. Existing `cli.rs` is large (previously ~1000 lines); this scoped change removes duplication without reorganizing unrelated commands.

## Verification commands and results

All commands ran in the issue worktree with `CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0`.

- `cargo test --offline --locked -p hoimin-cli --test cli_config -- --test-threads=2`: **75 passed, 0 failed**, exit 0. Log: `/tmp/hoimin-design-audit-5d3aee95/task-5-cli-config.log`.
- `cargo test --offline --locked -p hoimin-core --test plan_config --test config_json -- --test-threads=2`: **25 + 11 passed, 0 failed**, exit 0. Log: `/tmp/hoimin-design-audit-5d3aee95/task-5-core-config.log`.
- `cargo test --offline --locked -p hoimin-cli --example generate_cli_reference -- --test-threads=2`: **7 passed, 0 failed**, exit 0. Log: `/tmp/hoimin-design-audit-5d3aee95/task-5-cli-reference-tests.log`.
- `cargo run --offline --locked -p hoimin-cli --example generate_cli_reference -- --check`: exit 0; existing generated reference is current, no documentation changes required. Log: `/tmp/hoimin-design-audit-5d3aee95/task-5-cli-reference-check.log`.
- Real `target/debug/hoimin completions` subprocesses for **bash, zsh, fish, powershell**: each exit 0, nonempty Hoimin script and empty stderr. Sizes 29203, 20263, 20090, 24206 bytes respectively. Log: `/tmp/hoimin-design-audit-5d3aee95/task-5-completions.log`.
- `cargo fmt --all` applied repository formatting; `git diff --check` exit 0. Required commit hooks still to run.

## Verification: five distinct self-reviews

1. Behavior review: read full CLI test output and confirmed 75/75, including common run/plan equivalence, native argv bytes, path/text UTF-8 rejection, exact malformed duration/byte diagnostics, defaults, runtime typed validation, help, and explicit run-only option rejection.
2. Boundary review: read both core configuration suites: 36/36 pass, including normalized configuration validation, timeout/resource limits, resume/session constraints and config serialization contracts. No core behavior changed.
3. Generated-interface review: 7/7 generator tests and freshness check pass. Real four-shell generation produces scripts without stderr; parser/help tests pass. Existing documentation remains correct without regeneration.
4. Evidence-quality review: baseline and post-refactor tests execute real parsing/normalization (no mocked structure checks). The initial symbol-only migration mistake is recorded, corrected before baseline success and before production edits; no failed command is passed off as successful. Windows branch has literal unpaired-code-unit expectations, but this host executes only Unix branches.
5. Scope/resource review: only two tracked files changed; ignored report stays local. All Cargo uses one existing target directory, two build jobs, debug info disabled, incremental disabled, offline/locked flags and two test threads where supported. No Python environment sync/install, controller files, push, or broad final suite run. The controller owns that final suite; mandatory commit hooks will validate all compile targets/features.

## Commit and mandatory hooks

Created **`f6e00d71 refactor(cli): compose shared raw mutation arguments (#971)`**, with only `crates/hoimin-cli/src/cli.rs` and `crates/hoimin-cli/tests/cli_config.rs` staged. The requested sandbox escalation was used for index writes and commit; no hooks were bypassed. Commit inherited all four required Cargo resource variables plus `CARGO_NET_OFFLINE=true`.

Mandatory hooks all **Passed**:

- cargo fmt (workspace)
- cargo fmt (vendored parser)
- cargo clippy (workspace), all targets/features, deny warnings
- cargo clippy (vendored parser), deny warnings

Commit output: `/tmp/hoimin-design-audit-5d3aee95/task-5-commit.log`. No repeated test suites were run after passing, except the required hooks' own checks.

## Completion: five distinct self-reviews

1. Requirements completion review: acceptance ownership goal satisfied (one raw declaration/owned composition, no full transfer); run/plan common normalization remains shared; required and malformed diagnostics, native argv and rejected invalid UTF-8, and run-only restrictions have passing behavioral evidence. Timeout contract and saved schemas remain unchanged for Task 6.
2. Committed-artifact review: inspected `git show --stat --oneline HEAD` and confirmed exactly the two intended files, commit subject and SHA. `git diff HEAD^ HEAD --check` passes and `git status --short` is empty. No controller plan/ledger or ignored report entered the index.
3. Quality completion review: re-read the production diff and test migrations. No legacy aliases, field-forwarding wrappers, additional normalization, shell conversion, broad abstractions or unrelated reorganization. Shared validation borrows, owns no copied raw values, and preserves existing error ordering. Public production callers compiled through the all-target hook.
4. Verification completion review: final evidence is **118 focused tests passing** (75 CLI + 36 core + 7 generator), freshness check current, four real completion outputs valid, and all four mandatory hooks passing. Baseline characterization 75/75 preceded production edits. Initial test migration failure and correction were disclosed. Windows-specific branches require Windows CI; no claim of executing Windows on this Unix host.
5. Delivery/resource completion review: local commit exists, working tree clean, full report remains ignored/local, no push, no environment installation, no controller edits, and no duplicate broad suite. All requested work for Task 5 is complete. The Cargo slot can return to the controller.

## Files changed

- `crates/hoimin-cli/src/cli.rs`: reuse the existing composed parser args as public `RunArgs`, remove duplicate shared raw fields and copying, share required-input checks, destructure for normalization.
- `crates/hoimin-cli/tests/cli_config.rs`: migrate raw-field checks to normalized behavior; split touched scenarios; characterize equivalence, diagnostics, native argv/UTF-8 rejection and plan/run restrictions.

## Concerns

None requiring a design decision. Windows cfg-specific assertions were preserved/expanded with literal units, but were not executed on this Unix host. Broad final test suite is intentionally controller-owned. An optional process inspection (`ps`) was denied by the workspace sandbox; the commit session and mandatory hook outputs were checked directly and completed successfully.

**Status: DONE**.
