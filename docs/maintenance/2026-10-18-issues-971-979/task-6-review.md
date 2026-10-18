# Spec Compliance

- ❌ Minor spec gaps: discovery ownership, persistence, versioning, identity and oracle behavior satisfy the resolved design; current development documentation still claims old fingerprint byte compatibility, and several new finite test cases lack explicit names (`docs/development.md:1774`; `crates/hoimin-cli/tests/cli_config.rs:1947`, `:1992`; `crates/hoimin-core/tests/resume_policy.rs:1049`). See Minor findings 1–2.
- ✅ Clarified diagnostic scope: the controller's Task6 Ruling permits a different first diagnostic for independently invalid plan options. Mandatory selector/argv checks still run before discovery normalization (`crates/hoimin-cli/src/cli.rs:573`, `:653`), individual discovery errors retain flag attribution (`crates/hoimin-cli/tests/cli_config.rs:1990`), and discovery semantic validation precedes project I/O (`crates/hoimin-cli/src/plan.rs:335`). No compatibility normalization layer is needed.
- ⚠️ Controller scope: Windows native argv/process behavior and final integrated workspace suites remain unverified here, as explicitly recorded in `task-6-report.md:24`. Final cross-issue OKF reconciliation belongs to Task 10 (`task-6-report.md:32`). No additional suites were run by this reviewer.

# Strengths

- The actual Clap option lives in `RawPlanArgs`, with no run-time rejection workaround or compatibility forwarding API (`crates/hoimin-cli/src/cli.rs:293`). The common run normalizer remains shared (`crates/hoimin-cli/src/cli.rs:446`).
- Required, strict `DiscoveryConfig` and strict raw/normalized execution limits implement the saved ownership boundary; creation validates the ceiling before project work, and verify reads the saved discovery setting (`crates/hoimin-core/src/config.rs:16`, `:532`, `:669`; `crates/hoimin-cli/src/plan.rs:335`, `:964`). The focused saved-plan tests cover missing/null/zero/unknown/oversized settings and obsolete nested fields (`crates/hoimin-core/tests/plan_config.rs:563`).
- Fingerprint encoding removes only the discovery duration, and a real two-plan test compares ordered candidate descriptors/IDs, execution fingerprints and differing manifest bytes (`crates/hoimin-core/src/resume.rs:399`; `crates/hoimin-cli/tests/plan.rs:5339`). Schema-12 hash vectors accompany explicit rejection of old vectors (`crates/hoimin-core/tests/resume_policy.rs:1007`, `:1047`).
- Plan/report/fingerprint versions are 10/8/12; session storage stays at 5, and tests retain historical fingerprint metadata while checking rejection without DB rewriting (`crates/hoimin-cli/src/plan.rs:40`; `crates/hoimin-core/src/report.rs:11`; `crates/hoimin-core/src/resume.rs:12`; `crates/hoimin-cli/tests/session_handler.rs:153`, `:1036`). Historical report/event fixture files are not modified by this diff.
- The plan-scoped Lean model and Rust adapter preserve the timeout field attribution without pretending that raw execution config still owns analyzer timeout (`formal/HoiminOracle/HoiminOracle/TimeoutLimitModel.lean:10`; `crates/hoimin-core/tests/lean_timeout_limit_oracle.rs:156`).
- The report records five distinct passes for each required stage, including concrete fixes, recovered test failures, platform limits and actual final hook outcomes (`task-6-report.md:3`, `:12`, `:19`, `:87`, `:96`).

# Issues

## Critical

- None found.

## Important

- None under the controller's explicit Task6 Ruling.

## Minor

1. **Current development documentation still promises old fingerprint byte compatibility.** `docs/development.md:1774` now says schema 12 retains field-13 framing, then claims pre-refactor digest goldens verify byte compatibility. The modified tests instead assert those schema-11 digests differ (`crates/hoimin-core/tests/resume_policy.rs:1007`), and separate vectors freeze schema 12. Clarify that only the environment field framing remains unchanged while the whole execution fingerprint is incompatible. The adjacent SQLite-schema-4 statement at `docs/development.md:1776` is also stale relative to the task's schema-5 contract.
2. **Some new finite cases lack the required explicit case names.** The added loops in `crates/hoimin-cli/tests/cli_config.rs:1947`, `:1992` and `crates/hoimin-core/tests/resume_policy.rs:1049` have no named parameter cases; the run-rejection loop also omits which spelling failed. Give the finite spellings, invalid-duration categories and environment variants explicit names and include them in assertion output. The named tables already used in `crates/hoimin-core/tests/plan_config.rs:565` show a suitable local pattern.
3. **The new shared-deadline test has a narrow scheduling margin.** `crates/hoimin-cli/src/analyzer/mod.rs:1146` consumes 750 ms of a one-second real deadline before asserting that target two started. A worker delayed by 250 ms can fail this setup despite correct deadline behavior. Prefer a controlled clock/handshake test seam, or a design that does not make successful setup depend on this small wall-clock margin. The observed analyzer run passed; this is a robustness concern, not a claimed reproduced failure.

# Evidence and Focused Checks

- Read the supplied 6,576-line diff once in chunks. Recovered the initially truncated first chunk; did not regenerate a branch diff or rerun suites.
- Named risk, diagnostic ordering: inspected only `validate_mutation_inputs`, shared raw parsing, selector parsing and core raw normalization (`crates/hoimin-cli/src/cli.rs:653`, `:702`, `:816`; `crates/hoimin-core/src/config.rs:1009`). Independently malformed options can report a different first error; the controller explicitly accepted this Task6-only change while retaining mandatory selector/argv and individual-error contracts. No Cargo invocation was needed.
- Named risk, oversized saved discovery bypass: the diff cuts off the persisted-plan validation caller, so inspected `crates/hoimin-cli/src/plan.rs:716` and confirmed `normalized_config.validate()` precedes source-record/candidate validation at line 736.
- Named risk, binary seeds hidden by the textual diff: read the five changed seed byte strings directly. Valid-intent events use schema 8 and the unsupported seed uses 999 (`fuzz/seeds/report_sequence/run-started:1`, `run-with-diagnostic:1`, `diagnostic-before-run:1`, `diagnostic-run-id-mismatch:1`, `unsupported-schema:1`). No fuzz run claimed.
- Observed final hook evidence: `/tmp/hoimin-design-audit-5d3aee95/task-6-commit-final.log:1` records all four required fmt/Clippy hooks passing and commit `32f50de0`.
- Observed focused result logs: `task-6-plan-unit-final.log:67` has 58 passed/3 ignored; `task-6-analyzer-unit.log:17` has 11 passed; `task-6-core-followup.log:24`, `:56`, `:93` have 17/26/31 passed; `task-6-python-schema.log:2` has 15 passed. All are under `/tmp/hoimin-design-audit-5d3aee95/`. No warnings found in these inspected final logs.
- Observed Lean evidence: `task-6-lean-build.log:1` records model/proof/cases build success, and `task-6-lean-sensitivity.log:1` records all four detections. Build/output/check/sensitivity stats each record exit 0 with a 30,000-ms wall limit and 2,097,152-KiB RSS limit; measured durations are 4,107/1,940/2,221/575 ms and peaks 673,984/652,240/675,392/679,152 KiB. Paths: `/tmp/hoimin-design-audit-5d3aee95/task-6-lean-{build,output,check,sensitivity}-stats.json:1`.

# Assessment

**Task quality: Approved.** The ownership boundary and cross-format migration are coherent and have substantial focused evidence. No blocking code-quality issue remains under the clarified diagnostic scope; correct the localized documentation and test-case spec gaps, and consider strengthening the deadline test's scheduling robustness.
