# Progress input summary coherence (#483)

## Contract and correspondence worksheet (before modeling)

The reader must reject contradictory complete/counts/exit fields before passing
reports to progress comparison. Counts and score already match mutant events.
The summary must be consistent with some run-level flags under the existing
MutationScoreExitPolicy contract; missing flags must not be reconstructed as
false. Baseline disposition retains its existing precedence after summary
validation. This does not validate individual mutant termination/output fields
(#460) or every possible relation between baseline events and run flags.

| Premise | Lean representation | Production configuration | Public observation | Mode |
| --- | --- | --- | --- | --- |
| Counts match statuses, small exact score | Seven existing Status values | JSON generated from Lean counts and rational score; unique IDs, ordered events | reader accepts structure before summary semantics | strict |
| Complete/exit may be corrupted | Input.reportedComplete/reportedExit | Set exactly those JSON fields, schema v2 and v3 | CLI exit 2, input path in error, no progress JSON | strict |
| Valid incomplete may have no inconclusive mutants | Counts plus existential run flags | Empty/killed/survived reports with failure exits | unusable, indeterminate | strict |
| Passed/failed/absent baseline | Baseline enum | Exit(0), Exit(1), null | usable or existing unusable reason | strict |
| Valid reports reach comparison | toProgressReport, existing ProgressDecision model | Two identical files, patience 1 | latest state and stall count | strict |
| Arbitrary natural counts | Counts | No unbounded machine input counterpart | Generic model proofs only | model-only |
| Setup/spawn/timeout/parse failure | No semantic observation | Isolated temp directory, bounded CLI | Adapter fails as infrastructure-error | infrastructure-error |

The oracle test first collected mismatches against the unfixed reader and then
became a passing regression gate after the fix. No internal-fixture cases are
needed in the new reader corpus. The generated JSONL owns expectations; the
adapter only translates fixtures and observes the public reader/CLI.

Exploration is a finite boundary matrix, not exhaustive JSON validation or an
unbounded execution proof. Sensitivity targets blind complete trust, omitted
inconclusive validation, and lower-priority failure overriding error. Atomicity
and concurrent interleavings do not apply to this pure, single-input validation.
Repeated identical inputs exercise the comparison boundary without claiming
general idempotency. Initial Lean checks ran serially with a 20-second deadline
and 768 MiB aggregate RSS cap, sampled every 250 ms. The user subsequently
authorized a local cap increase to 1 GiB for the native-generator follow-up below.

## Implementation and results

Implemented on base `623dd808612dbc34775e16814845eec0bc52dff9`, macOS arm64.
Issue #483 remains open pending integration.

Both schema readers now call `validate_summary_coherence` after checking counts.
Complete summaries require no inconclusive mutants and exit 0/1 matching survivor
presence. Incomplete summaries permit exit 2/130, or exit 3/4 when no mutant error
requires the higher-priority infrastructure exit. Missing run flags remain
existential: empty or entirely conclusive counts can legitimately be incomplete.
No summary fields are repaired or inferred on input. The implementation reuses
`ExitPolicy::from_summary` and `exit_code_for`; `exit_can_result_from_run_failure`
checks the possible run-failure policies. Lean expresses the same rule through
`exitCanResultFromRunFailure`. Fixture translation, reader bypass and observed
CLI results have named functions and a `CliObservation` type.

The new Lean model imports the existing MutationScoreExitPolicy and
ProgressDecision models. Its four generic theorems establish that an incoherent
summary is invalid, invalid input has no comparison input, complete requires
zero inconclusive counts, and every summary produced by the existing modeled
exit policy is coherent for arbitrary counts and run flags. These are model
proofs; they do not establish universal correctness of the Rust implementation.

The finite matrix has 11 status profiles (empty, seven single statuses, three
mixed), two complete values, eight exit values (-1, 0, 1, 2, 3, 4, 130, 255),
and eight failed/missing-baseline boundary cases: 184 rows. Every row runs against
both JSON schemas through the real CLI: **368 strict observations**. This is not
exhaustive JSON, input-size, or arbitrary-history exploration.

1. A deliberately blind model failed the minimal timeout/complete=true Lean
   example, then the correct model and generic proofs passed.
2. The real, unfixed CLI disagreed with **264 of 368** observations. These are
   manifestations of the same summary-validation bug, not 264 distinct bugs.
3. After the shared reader fix, all 368 observations matched. Invalid input
   returns exit 2, names the path, and emits no progress JSON. Valid incomplete
   reports remain unusable/indeterminate; valid conclusive reports retain their
   expected self-comparison and stall counts, including the empty-report case.
4. All three deliberately broken families were detected: blind complete trust,
   omitted inconclusive validation, and error losing to lower-priority exit 4.

The original mixed witness is retained in the machine-readable
[verification record](2026-09-11-progress-input-verification.json), together with
all 264 original mismatch observations and resource-guard measurements.
`killed_timeout_true_4` previously produced usable/saturated/stalls=1 on both
schemas; it now produces invalid input with no comparison output.

## Existing comparison adapter premise correction

Six existing ProgressDecision cases injected usable inconclusive reports through
raw JSON. That violates the reader contract now enforced. Their inputs and
expected comparisons are unchanged; Lean now classifies them as
`internal-fixture`, and the adapter explicitly constructs typed UsableReports
and calls the existing `compare_reports` API. They remain blocking tests.
The other 17 strict cases still run through the real CLI, and the existing one
duplicate-identity model-only case retains its classification.

The old fixture also hardcoded exit 0 for survivor summaries; it now supplies
exit 1. The null-score reader test now supplies legitimate incomplete input.
The comparable-score rendering test now uses a newly added killed candidate
instead of an invalid usable timeout: its intersection score remains 0.0 while
the full report score is 0.5. No production comparison rules were changed.

## Verification and resource limits

| Check | Result |
| --- | --- |
| Full Rust workspace before PR | 1621 passed, 13 ignored, 0 failed |
| New CLI oracle | 184 × 2 schema observations matched |
| Existing progress integration/property tests | 62 passed |
| Existing comparison oracle | 3 passed (17 strict, 6 internal-fixture, 1 model-only row retained) |
| Existing core mutation-score/exit-policy oracle | 4 passed |
| CI workflow contract tests | 26 passed |
| Workspace Clippy, all targets/features, warnings denied | passed |
| Formatting and diff whitespace | passed |
| New model generic proofs | passed |
| New/updated corpus freshness | passed through Lean interpreter and native generators |
| New/previous sensitivity families | 3/3 and 7/7 detected |

The initial native generator build hit the 768 MiB RSS monitoring threshold
during linking (sampled peak 806,368 KiB; guard exit 125). Generation, freshness
and sensitivity initially passed with `lake env lean --run` under the same guard.

With the user's subsequent authorization, the local RSS cap was raised to
1 GiB (1,048,576 KiB). The 20-second deadline, 250 ms sampling and serial execution
were retained. Both native generators then linked and passed corpus freshness
and sensitivity checks. These are incremental builds with the existing cache,
not measurements from an empty build cache. No corpus or model was changed.

| Native follow-up | Elapsed ms | Peak aggregate RSS KiB |
| --- | ---: | ---: |
| Reader native link and freshness | 6,030 | 779,888 |
| Reader native sensitivity | 834 | 82,384 |
| Comparison native link and freshness | 6,605 | 794,528 |
| Comparison native sensitivity | 827 | 86,384 |

The hosted [Lean audit](https://github.com/tokyogas-tech/hoimin/actions/runs/34569154027/job/103167651933)
also passed for implementation commit `7c6130e199fb97ac613d65999610716193ddc1aa`.
CI retains its existing 2 GiB limit and covers all registered modules and
generator gates. The workflow contract test verifies command coverage and
dependency ordering.

Run Lean commands from `formal/HoiminOracle`, each separately:

```sh
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/progress-input-proof.json -- lake build HoiminOracle.ProgressInputProofs
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/progress-input-check.json -- lake env lean -j1 -DElab.async=false --run ProgressInputAuditMain.lean --check corpus/progress-input.jsonl
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/progress-input-sensitivity.json -- lake env lean -j1 -DElab.async=false --run ProgressInputAuditMain.lean --sensitivity
```

From the repository root:

```sh
cargo test --offline -p hoimin-cli --test lean_progress_input_oracle --test lean_progress_decision_oracle --test progress
cargo test --offline -p hoimin-core --test lean_mutation_score_exit_policy_oracle
.venv/bin/python -m unittest tests.test_ci_workflow
cargo clippy --offline --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
git diff --check
```

Independent read-only review found no actionable issues and confirmed that only
the six intended modes changed in the existing corpus, with all comparison
expectations preserved. Further auditing of mutant result coherence (#460),
metadata propagation, analyzer boundaries, and scale remains separate work.

## Native follow-up commands

Run separately from `formal/HoiminOracle` with the user-authorized local cap:

```sh
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 1024 --sample-ms 250 --stats /tmp/hoimin-progress-input-native-1024-check.json -- lake exe generate_progress_input -- --check corpus/progress-input.jsonl
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 1024 --sample-ms 250 --stats /tmp/hoimin-progress-input-native-1024-sensitivity.json -- lake exe generate_progress_input -- --sensitivity
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 1024 --sample-ms 250 --stats /tmp/hoimin-progress-decision-native-1024-check.json -- lake exe generate_progress_decision -- --check corpus/progress-decision.jsonl
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 1024 --sample-ms 250 --stats /tmp/hoimin-progress-decision-native-1024-sensitivity.json -- lake exe generate_progress_decision -- --sensitivity
```
