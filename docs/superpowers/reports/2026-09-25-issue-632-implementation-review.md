# Issue 632 implementation and test reviews

Design and plan each received three self-reviews before their initial commit (`76d7aee`); findings are recorded in those documents. The following reviews concern the final Rust implementation and executable tests. Formal correspondence and Lean execution evidence are recorded separately by the formal reviewer.

## Implementation self-review

1. **Propagation and validation:** traced `RawMutationArgs -> RunArgs -> RawRunConfig -> Selection -> TargetHandler -> Git`, plus `PlanConfig -> RunConfig` during verify. CLI rejects explicit context without changed selection, including zero; normalized/raw validation rejects nonzero context without changed and values above 1073741823. Historical manifests omit the field and deserialize to zero. Both run and plan exercise these paths.
2. **Boundary and compatibility:** examined Git unified diff and existing hunk parser behavior for deletion gaps, no surviving lines, unchanged files and explicit restrictions. Kept default `--unified=0`, deterministic diff flags, binary exclusions and the public zero-context Git effect API. Git performs clipping and hunk merging without new file reads. Effective target ranges already enter fingerprints. The run-event schema permits additive normalized-config fields, so no schema-version change is needed.
3. **Final diff and user contract:** checked README, top-level help, generated completions, ranking reason and saved-plan behavior. Documented that `changed_line` includes requested neighboring lines. Added a sibling function to the public example test and exercised maximum context so symbol restriction cannot accidentally disappear behind a one-function fixture. No unrelated bug fixes or changes to existing ranking weights. A source-level portability check found that Git 2.43 doubles context in C long; reduced the initial maximum to 1073741823, so this expression remains representable on platforms with 32-bit long. See the design portability ruling for the source citation.

## Test self-review

1. **Acceptance mapping:** CLI/config tests cover defaults, explicit zero, maximum, overflow, negatives, malformed input, dependency validation and historical JSON. Real Git tests cover modification, deletion at beginning/middle/end, empty files, maximum context and intersection with an explicit single line. Public plan tests cover both issue examples with symbol selection, diff-base, ranking and verify restoration.
2. **Oracle independence:** the original RED target assertion expected [1,3] while production returned [2,2]; CLI failed with UnknownArgument; core round-trip returned Null and accepted an invalid nonzero context. These were observed before production changes. Git fixtures assert manually specified current-line ranges. The separate Lean adapter derives expected lines from Nat intervals rather than parsing Git output in the test. Tests exercise real Git and public APIs without replacing dependencies with mocks.
3. **Negative neighbors and integration:** checked that unchanged files remain absent, explicit lines and symbols remain restrictions, and N=0 preserves empty deletion selection. Strengthened raw and normalized run validation assertions in addition to plan validation, plus generated completion coverage. Existing complete config/plan/target suites pass; final workspace and lint results are listed below.

## Validation

- RED: core plan_config changed_context tests failed on missing serialization and missing validation; CLI failed on unsupported option; TargetHandler failed on unexpanded line range.
- GREEN: complete cli_config (59), plan (79 passed, 1 ignored), target_handler (45), core config_json (7) and core plan_config (18) suites passed before the final coverage additions.
- `cargo test --workspace`: exit 0; 2294 passed and 22 ignored across 96 suite results. This ran before the final constant-only portability correction.
- After the correction, focused CLI/config/target/plan tests and the 44-case Lean public adapter passed (7 Rust tests in total). Both issue fixtures ran at N=0, N=1 and N=1073741823, with a sibling function excluded by the symbol selector.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: exit 0 after correcting new-test variable naming and integer literal style.
- `cargo fmt --all -- --check` and `git diff --check`: exit 0.
- Local execution logs: `/tmp/issue632-workspace.log`, `/tmp/issue632-final-focused.log`, `/tmp/issue632-clippy.log`. These temporary logs are evidence locations for this checkout; the committed tests are reproducible without them.

These checks establish the stated selection contract for the tested Git/Python fixtures. They do not prove Git internals, CR-only line coordinate behavior, or arbitrary semantic influence beyond the configured neighborhood.


## CI contract follow-up

The remote wheel smoke job exposed a missing entry in `tests/test_ci_workflow.py`'s closed Lean executable-to-corpus registry. Local unittest reproduction failed exactly one of 40 workflow contract tests; product Rust and Lean execution were unaffected. Added the new generator in lakefile order. Review pass 1 compared the executable and corpus names with lakefile and CI. Pass 2 checked that the default sensitivity set includes the generator and its supported flag. Pass 3 checked the one-entry diff and reran all 40 workflow tests. The registration repair does not change production Python; mutation testing is not applicable to this test-only registry.

The first registry repair exposed a second contract assertion: the CI generator list must follow lakefile order, not merely contain the same entries. Moved the new generator gate to the end, matching lakefile and the closed registry. No Lean compilation or proof order changed.
