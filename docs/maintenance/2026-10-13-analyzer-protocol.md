# Retire the unused analyzer JSONL parser

## Purpose and decision

Issue #851 starts at `1b296f4` on `refactor/851-analyzer-protocol`.
The branch was created without an upstream from freshly fetched `origin/main`.
Both origin URLs are `https://github.com/HayaoSuzuki/hoimin.git`; push must
explicitly target the dedicated branch, never `main`.
The current handler invokes the in-process Rust analyzer and consumes typed
candidates and diagnostics. Repository searches found the JSONL parser used only
by its dedicated integration tests and fuzz target. The [architecture record](../knowledge/design/architecture.md) and
[C4 module description](../architecture/c4-model.md) confirm that the former Python/LibCST helper was replaced by Rust/Ruff. No current
transport or documented compatibility purpose was found for `AnalyzerProtocol`.
This does not establish that no external Rust consumer exists.

Remove the parser and its dedicated checks. Preserve the three shared result
types, their fields, derives and public `hoimin_cli::analyzer` exports in
`types.rs`. Keep candidate validation, spool storage/replay, candidate ordering,
and truncation on the current handler path unchanged. Future multilingual work
in #743 is not a reason to preserve an unused transport today.

## Design self-reviews

1. **Purpose:** checked runtime callers, tests, fuzz consumers and migration
   records before choosing removal; exports alone do not establish runtime use.
2. **Ownership:** separate shared result data from the retired wire parser;
   preserve the existing analyzer/validation/store responsibilities.
3. **Compatibility:** enumerate removed Rust exports explicitly; distinguish
   this internal transport from supported JSON/JSONL reports and stored spools.
4. **Scope:** preserve all analyzer algorithms, limits, diagnostics and schema
   versions. Do not add a replacement transport, facade or speculative plugin API.
5. **Complexity:** use the philosophy skill to remove an abstraction with no
   current consumer. A plain shared data module needs no new generic framework.

## Plan and planning self-reviews

1. **Baseline:** run the existing CLI library, handler, Rust analyzer and Lean
   corpus adapters before edits; use their observable behavior as regression evidence.
2. **Migration:** move the three definitions verbatim and update every direct
   path consumer. Delete only the 19 parser-specific handler tests.
3. **Fuzz lifecycle:** remove the parser bin, harness and five committed seeds;
   update the campaign expectation and documentation. CI enumerates the manifest
   dynamically, so its execution logic and cache handling need no change.
4. **Evidence:** run workspace tests, contracts, strict Clippy and docs checks;
   compile remaining fuzz bins and replay Python analyzer seeds. Preserve
   historical audit evidence while updating current OKF concepts and sources.
5. **Resources and completion:** use the dedicated target with two build/test
   workers, no incremental artifacts or debug symbols. Sample process-group RSS
   and target size, stopping above 8 GiB RSS/12 GiB artifacts, below 20 GiB free,
   or after 40 minutes per command. Obtain independent review, commit with hooks,
   clean that target, and create a PR against `HayaoSuzuki/hoimin:main`.

## Compatibility

Removed internal Rust exports: `AnalyzerProtocol`, `AnalyzerRecord`,
`AnalyzerSummary`, `AnalysisStatus`, `ProtocolError`,
`DEFAULT_MAX_ANALYZER_LINE_BYTES`, and `DEFAULT_MAX_ANALYZER_OUTPUT_BYTES`.
External Rust consumers of these unsupported APIs must remove that integration
or retain the prior revision; no replacement JSONL parser is provided.
`AnalyzerCandidate`, `AnalyzerDiagnostic`, and `AnalyzerDiagnosticCode` retain
their public names and definitions. CLI options, defaults, reports, candidate
IDs, saved plans, session formats, supported platforms and packages do not change.
Under `docs/compatibility.md`, the version floor remains 0.4.0; no CLI invocation,
report reader, plan or session migration is required.

The retired `analyzer_protocol` fuzz command is no longer available. Use the
remaining `python_analyzer` and `candidate_validation` targets for the live
analysis path. Old cache directories are inert: CI discovers targets from
`fuzz/Cargo.toml`. Historical #805 measurements, the July codebase audit and earlier design plans
remain historical evidence, not a current target inventory.

## Implementation self-reviews

1. **Definitions:** a direct comparison against the base verifies that all three
   shared type definitions are byte-for-byte unchanged. Public re-exports keep
   their names; direct test and fuzz module includes now resolve `types.rs`.
2. **Runtime:** the handler diff changes only its module contract, declarations
   and re-exports. Rust analysis, validation and store implementations are untouched.
3. **Tests:** the removed block contains exactly 19 parser-only tests; the
   following spool tests are byte-for-byte unchanged. Live handler tests remain.
4. **Fuzz/CI:** searches find no executable consumer of the retired API or target.
   The manifest has seven targets, matching the campaign assertion and guide.
   Only parser seeds are removed; the Python analyzer seed corpus is unchanged.
5. **Documentation:** the entry contract describes typed in-process results.
   Current OKF concepts cite this record; historical source hashes and measured
   reports remain untouched. Compatibility explicitly names removed Rust APIs.

## Philosophy assessment

For the changed analyzer boundary, seven of the eight diagnostic rows pass
(approximately 8.75/10): purpose, interface size, contract comments, review of
complexity, ownership of decisions, discoverable boundaries and deliberate
maintenance. The remaining limitation is the existing test/fuzz use of direct
`#[path]` includes: changing source layout still requires updating those callers.
A focused reusable analyzer crate could remove that coupling, but adds a package
boundary outside this issue. This change preserves that test strategy without
introducing a new facade. The score is a scoped design judgment, not a repository
quality measurement or proof.

## Verification scope

The pre-edit run passed the CLI library (830 passed, 18 ignored), analyzer handler
(41 passed), Rust analyzer integration (300 passed, 3 ignored), and valid Python
corpus adapter (285 passed, 3 ignored). Two additional library child-process
summaries are not counted twice.

No new Lean model or property is added: this removes an unused parser without
changing the retained algorithms or state transitions. Existing proptest cases
and Lean-derived corpus correspondence tests remain executable regression
evidence; their successful execution is not a new Lean proof or an exhaustive
compatibility claim. Python changes only remove the obsolete campaign target
expectation, so production-Python mutation testing is not applicable.

Post-edit checks on macOS/aarch64:

- `cargo test --locked --workspace --no-fail-fast`: 2,818 passed, 0 failed,
  29 ignored across 149 top-level harness summaries. Two separately printed
  child-process summaries are excluded from the total; one is split across
  lines by interleaved output. The handler now has 22
  passing tests (exactly 19 parser tests removed); the Rust analyzer and valid
  Python corpus retain 300 and 285 passing tests respectively.
- `python -m pytest tests/test_ci_fuzz.py tests/test_ci_workflow.py -q`: 197 passed.
- `python -m pytest tests/test_docs_site.py -q`: 16 passed.
- Changed Python test: Ruff lint and format checks pass. Workspace and fuzz
  formatting plus `git diff --check` pass.
- `python -m tools.docs_site --output site`: strict build and link checks pass.
  YAML checks validate 44 OKF concept headers, the root 0.2 header, and both new
  source links/footnotes. The initial ad hoc checker incorrectly required a
  concept `type` on the reserved root index; correcting the checker resolved it,
  with no change to repository metadata.
- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`:
  passes, including compilation of all targets/features.
- `cargo test --locked --workspace --features contracts --lib --no-fail-fast`:
  865 passed, 0 failed, 20 ignored (excluding two duplicate child summaries).
- `cargo clippy --locked --manifest-path fuzz/Cargo.toml --bins -- -D warnings`:
  all seven remaining fuzz bins compile and pass strict lint.
- Pinned-nightly AddressSanitizer replay passes for all 10 committed Python
  analyzer seeds (11 runs including libFuzzer initialization, reported RSS 76 MiB):

  ```console
  cargo +nightly-2026-10-08 fuzz run --dev --jobs 1 --target-dir "$PWD/target/fuzz" python_analyzer fuzz/seeds/python_analyzer -- -fork=0 -runs=0 -max_len=4096 -timeout=5 -rss_limit_mb=1024
  ```

  The first invocation used `--jobs 2`, which cargo-fuzz also translated to
  `-fork=2`; it performed 161 iterations in three seconds rather than a pure
  replay. It exited successfully. The 60 generated inputs were moved out of
  the worktree to a task-specific temporary directory. The corrected invocation
  above reports `10 files`, `INITED` and `DONE` at run 11; the committed seeds
  and both Cargo lockfiles remain unchanged. No long fuzz campaign is claimed.

## Verification self-reviews

1. **Regression coverage:** compare baseline and final handler counts; exactly
   the 19 retired parser tests disappear. Full workspace tests retain live
   analysis, validation, ordering, limit, diagnostic and spool behavior checks.
2. **Compilation surface:** strict Clippy compiles all shipping targets/features
   and all seven fuzz bins. Direct source inclusions compile against the same
   retained definitions; contracts-enabled library tests also pass.
3. **Replay integrity:** inspect the actual fuzz command and seed inventory,
   correct the unintended fork behavior, and verify the 10 original seeds with
   no generated input left in the diff. Distinguish replay from fresh exploration.
4. **Documentation and compatibility:** strict docs build and OKF source checks
   pass; remaining old protocol paths occur only in historical plans/audits.
   Version floors, lockfiles, reports and saved formats are unchanged.
5. **Evidence limits:** local execution is macOS/aarch64, not Linux or Windows.
   Ignored performance/fixture tests were not enabled. Existing Lean-derived
   tests and properties are bounded observations, not a fresh formal proof.
   Check resource summaries, obtain final independent review and clean the
   dedicated target after hooks before publishing the branch.

## Resources and independent review

Every monitored Cargo command completed without triggering the resource guard.
The largest sampled process-group RSS was 1,984,708,608 bytes (about 1.85 GiB)
for the baseline; workspace tests took 336.5 seconds. Sampling is periodic and
may miss short-lived peaks; the replay's own 76 MiB reading is more informative
than the two-second monitor for that short process. Limits are stop thresholds,
not a proof of hard OS-enforced memory containment.

An independent read-only source review against `1b296f4` reported no findings
requiring changes. It confirmed the retained definitions, 19-test removal,
seven-target inventory, compatibility notice and unchanged live implementations.
It did not independently run builds or establish external Rust API usage.

The final evidence review corrected the child-summary explanation above from
one to two (one summary was split by concurrent output); the 2,818 top-level
pass count was already correct. No source changes were requested.

The commit's workspace/vendor formatting and strict Clippy hooks passed. After
all checks, the dedicated target occupied about 4.7 GiB on disk.
`cargo clean --target-dir <worktree>/target` removed 5,791 files (Cargo reported
4.6 GiB); the target directory no longer exists. The shared checkout's build
artifacts were not cleaned.
