# Progress result validation audit (issue 460)

## Claim and contract, recorded before modeling

A saved report enters progress comparison only if every mutant result satisfies the report writer's single-result contract. Output-close required fields precede diagnostic integrity, which precedes status classification. Complete output with absent termination retains legacy compatibility. Lifecycle events are outside saved-report validation.

Declared intent: `classify_mutant_result` and `ReportSequence::mutant_error` require status to match termination unless output close timed out, which requires error status, present termination/output, and exactly one matching error diagnostic with nonblank message. Implicit behavior: absent termination with complete output accepts every status; unrelated diagnostics are allowed. Both schemas must reject invalid results before a self-comparison can claim saturation.

## Correspondence worksheet and bounds

| Premise / observation | Lean representation | Production configuration / public observation | Evidence | Mode |
| --- | --- | --- | --- | --- |
| Seven statuses | existing `Status` | mutant status JSON | report.rs classifier | strict |
| Optional termination | absent, exit zero, exit nonzero, timeout, OOM, process limit, cancelled | null, Exit(0), Exit(1), termination enum JSON | ProcessTermination | strict |
| Output state | complete / close timeout | output_state; output spool present; canonical matching diagnostic for close timeout | output_state_matches_result / output_diagnostics_match | strict |
| Summary and baseline | existing input model | unchanged generated counts, complete/exit and baseline JSON | existing adapter | strict |
| Rejection | invalid disposition / no history | exit 2, path diagnostic, empty stdout | real `hoimin progress` | strict |
| Acceptance | disposition plus complete stable history observation | self-comparison with patience 1, schemas 2 and 3 | real CLI JSON | strict |
| Malformed diagnostic payload / missing spool | outside finite model | direct Rust public input tests | report result validator | strict |

Finite domain: retain all 184 summary/baseline cases; add 7 statuses × 7 optional termination classes × 2 output states = 98 single-mutant cases. Each uses a coherent summary derived from the claimed status, passed baseline, and a present spool. Canonical diagnostics isolate status/termination/output semantics; missing required fields and diagnostic payload integrity use Rust tests. Two schemas produce 564 real CLI runs. Exit nonzero uses 1; all nonzero integers share the classifier branch. No timing, subprocess drain, disk I/O, concurrent transitions, or serialization proof is claimed. Expensive enumeration/serialization stays in the executable. Imported modules contain definitions and kernel proofs only.

Limits: serial Lean commands via `formal/HoiminOracle/tools/lean_resource_guard.py`, 30 s wall, 2048 MiB aggregate RSS, 250 ms sampling; package `-j1 -DElab.async=false`. No aggregate unbounded build and no limit increases.

## Findings and minimal witness

Before production edits, the generated adapter observed **158 strict mismatches** (79 invalid result combinations in each schema). No old summary case mismatched. The first case in stable enumeration order was `result_killed_exit_zero_complete`: one mutant, `status=killed`, `termination={"Exit":0}`, complete output with no close-timeout diagnostic; passed baseline; killed count 1, score 1, complete true, summary exit 0. Self-comparison with patience 1 produced `usable`, `saturated`, one consecutive stall, and one comparison. Lean expected invalid with no history. This is a confirmed implementation bug under the existing writer contract, not a new classification policy.

The focused malformed-result test also failed before production edits: v2 close-timeout error with absent termination was returned as an incomplete report instead of rejected. Both failures were observed using the real saved-report reader; the corpus invokes the real CLI binary in isolated temporary directories with a 10-second process timeout.

After extraction and calls from both document validators, all 282 cases × two schemas matched. The corpus retains 19 accepted result combinations (13 complete-output, six close-timeout error overrides) and rejects 79. Accepted result combinations include all seven legacy absent-termination statuses for complete output. The existing 184 corpus lines remain byte-for-byte identical. The adapter merely maps optional result fields to wire values, supplies canonical output/diagnostics, and reads Lean's expected disposition/history; it does not classify results.

Minimal regression reproduction from the repository root:

```sh
CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-451/target cargo test -p hoimin-cli --test lean_progress_input_oracle killed_exit_zero_minimal_regression_follows_lean -- --exact
```

## What Lean establishes

Kernel-checked properties retain the existing summary coherence, producer coherence, and invalid-no-comparison theorems. Added properties establish that result-invalid input is invalid, complete output with absent termination accepts every status, close timeout requires error status, successful exit with output-close timeout legitimately accepts error status, and killed with successful exit is invalid. Fixed kernel witnesses show that skipping result validation falsely accepts the minimal killed/success input, and ignoring output state falsely rejects the legitimate error override.

These are properties of the independent model. No claim is made that Lean proves Rust, JSON decoding, process supervision, or arbitrary diagnostic payloads correct. The 564 CLI observations establish only exercised same-premise correspondence. Diagnostics and required output fields are separately exercised through public Rust input APIs for both schemas; direct typed validator/sequence tests check error precedence and sequence state preservation.

## Sensitivity and exclusions

| Family | Broken variant / evidence | Result |
| --- | --- | --- |
| Boundary / missing validation | `brokenSkipResultValidation`, killed + Exit(0) + complete | detected; fixed witness plus full result matrix |
| Precedence / output override | `brokenIgnoreOutputState`, error + Exit(0) + close timeout | detected; fixed witness plus full result matrix |
| Existing summary boundaries | blind complete trust; omitted inconclusive; reversed error precedence | all detected on `summaryCases` alone |
| Atomicity / transactionality | Pure single-result predicate has no partial state or transitions | inapplicable to this model; Rust sequence test verifies rejection neither advances sequence nor consumes active mutant |
| Uniqueness / idempotency | No identity allocation, durable effect, replay, or mutation in this model | inapplicable to this model; existing report-sequence oracle remains in focused verification |

The search is a flat 98-case Cartesian product, not a trace exploration; there is one observation per configured input and no transition-depth claim. Status and termination ordering give a stable smallest witness. Nonzero exits are reduced to one semantic class; all actual classifier nonzero values share that branch. Canonical diagnostics deliberately exclude payload validation from the Lean abstraction without implying that malformed diagnostics are acceptable.

## Review passes

Implementation review 1: compared extraction line-by-line against `mutant_error`. Required fields, diagnostic integrity, then classification retain the original precedence; lifecycle checks remain before the extracted call. Existing report-sequence oracle and default report-policy suite passed.

Implementation review 2: inspected both saved-report paths. Validation runs after their existing structure/count checks and before usable/unusable classification; errors own the existing result error as source and add the input path. No started events or synthetic lifecycle are introduced. Existing count-mismatch tests remain unchanged and green.

Implementation review 3: checked allocation and policy duplication. Result validation clones candidate identity only when constructing an error; diagnostic filtering now uses an iterator instead of collecting a temporary vector. A dedicated test accepts unrelated diagnostics before/after the close diagnostic and rejects a duplicate, protecting the iterator conversion. Progress callers delegate through one contextual wrapper.

Tests review 1: preserved all existing corpus assertions and the 184 original lines; added separate size assertions for the original and result partitions. The matrix contains every requested status/termination/output combination and runs each in both schema versions.

Tests review 2: inspected rejection observability. CLI adapter requires exit 2, empty stdout, structural diagnostic and input path; direct malformed-input tests additionally require candidate identity and the exact error family. Those cover absent termination/spool, wrong status, absent/duplicate/misidentified/misleveled/miscoded/blank close diagnostics, and a close diagnostic on complete output.

Tests review 3: checked positive counterexamples and sequence invariants. Legitimate error override and legacy null termination are accepted. Direct precedence test presents multiple faults and resolves them in order. Sequence test rejects invalid input, then accepts corrected input at the same sequence, proving no active-mutant consumption or sequence advancement. Tests that deliberately violate sequence contracts retain the repository's `not(feature = "contracts")` guard, and were run under default features.

Formal review 1: worksheet and finite bounds were recorded before model edits. Generated expectations are exclusively Lean-derived; original cases are byte-for-byte preserved. Corrected initial constructor-arity errors after adding the optional field; this was a proof compilation error, not a semantic mismatch.

Formal review 2: narrowed old sensitivity searches to `summaryCases` so result-invalid cases cannot accidentally satisfy old summary gates. Fixed proofs accompany both newly broken variants. Definitions live in Model and Proofs retains its original Model-only import; no CI module-order change is required.

Formal review 3: checked the correspondence boundary and cost. Every corpus case uses actual schemas 2 and 3 with canonical payloads; malformed JSON payload tests are classified strict because they call the public reader. Model-only claims are not reported as implementation proofs. Monitor failure is separately classified below and retained limits were never raised.

## Infrastructure and measured cost

A copied worktree-local `.lake` cache seeded compilation; the main cache was never symlinked or mutated. The first guarded Model invocation exited 126 with `reason=monitor_error`, elapsed 36 ms, peak RSS unobserved (0); the sandbox blocked `ps` process monitoring. Repeating the same guarded command with approved sandbox escalation succeeded. This is an infrastructure error and contributes no semantic result.

The initial proof build exited 1 due to constructor arity and a now-redundant tactic after adding the optional field. Corrected source passed; no bound increase was needed. Successful guarded runs measured: Model 3,807 ms / 688,640 KiB; Cases 559 ms / 638,448 KiB; Proofs with witnesses 3,593 ms / 676,352 KiB; generator build 5,172 ms / 723,024 KiB; generation 1,122 ms / 57,184 KiB; freshness 835 ms / 80,048 KiB; sensitivity 294 ms / 2,416 KiB; stats 298 ms / 3,008 KiB. Sampling can miss sub-250 ms peaks. The longest observed command and highest sampled RSS were the generator build, safely below 30 seconds / 2 GiB. Final import-preserving recompilation and freshness also passed.

The first default-parallel workspace all-feature run stopped at the unrelated `shell::tests::rollback_contention_marks_root_for_immediate_janitor_recovery`: 591 passed, one failed, nine ignored in the CLI library; expected one reclaimed root, observed zero with one preserved root. The exact isolated all-feature reproduction passed. The inspected test exercises coordinator locking and managed-root rollback, not the changed result-validation path. A serial all-feature rerun was used to complete verification; no unrelated production code was changed.

## Reproduction commands

From `formal/HoiminOracle`, each command below was run serially with the same guard. The stats names identify distinct retained `/tmp/issue460-*.json` records during this audit:

```sh
python3 tools/lean_resource_guard.py --timeout-seconds 30 --rss-limit-mib 2048 --sample-ms 250 --stats /tmp/issue460-model-final.json -- lake build HoiminOracle.ProgressInputModel
python3 tools/lean_resource_guard.py --timeout-seconds 30 --rss-limit-mib 2048 --sample-ms 250 --stats /tmp/issue460-proofs-dag.json -- lake build HoiminOracle.ProgressInputProofs
python3 tools/lean_resource_guard.py --timeout-seconds 30 --rss-limit-mib 2048 --sample-ms 250 --stats /tmp/issue460-cases-final.json -- lake build HoiminOracle.ProgressInputCases
python3 tools/lean_resource_guard.py --timeout-seconds 30 --rss-limit-mib 2048 --sample-ms 250 --stats /tmp/issue460-generator-final.json -- lake build generate_progress_input
python3 tools/lean_resource_guard.py --timeout-seconds 30 --rss-limit-mib 2048 --sample-ms 250 --stats /tmp/issue460-output.json -- lake exe generate_progress_input -- --output corpus/progress-input.jsonl
python3 tools/lean_resource_guard.py --timeout-seconds 30 --rss-limit-mib 2048 --sample-ms 250 --stats /tmp/issue460-check-final.json -- lake exe generate_progress_input -- --check corpus/progress-input.jsonl
python3 tools/lean_resource_guard.py --timeout-seconds 30 --rss-limit-mib 2048 --sample-ms 250 --stats /tmp/issue460-sensitivity.json -- lake exe generate_progress_input -- --sensitivity
python3 tools/lean_resource_guard.py --timeout-seconds 30 --rss-limit-mib 2048 --sample-ms 250 --stats /tmp/issue460-stats.json -- lake exe generate_progress_input -- --stats
```

Rust commands from the repository root (environment applies to every Cargo command):

```sh
export CARGO_INCREMENTAL=0
export CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-451/target
cargo test -p hoimin-cli --test lean_progress_input_oracle
cargo test -p hoimin-cli --test progress input_rejects_malformed_result_fields
cargo test -p hoimin-cli --test lean_progress_input_oracle --test progress
cargo test -p hoimin-core --test report_policy --test lean_report_sequence_oracle
cargo fmt --all
cargo test --workspace --all-features
cargo test -p hoimin-cli --all-features --lib shell::tests::rollback_contention_marks_root_for_immediate_janitor_recovery -- --exact
cargo test --workspace --all-features -- --test-threads=1
```

## Resolved counterexample ledger

| Input / failure | Classification | Resolution / owner decision |
| --- | --- | --- |
| killed + Exit(0) + complete, both schemas | confirmed bug | existing writer contract reused; invalid input rejected; minimal regression retained |
| remaining 78 invalid result classes, both schemas | confirmed bug | shared validation closes all 158 observed mismatches |
| close-timeout with missing termination/spool or malformed diagnostics | confirmed bug | public input tests reject with path, candidate and result-error context |
| sandbox monitor exit 126 | infrastructure error | approved guarded rerun succeeded, no safety limit change |
| initial Lean constructor/tactic error | infrastructure error | fixed model source; kernel proof checks pass |
| parallel managed-root contention assertion | test execution failure outside modeled boundary | isolated reproduction passed; serial workspace rerun recorded below |

No unresolved result-policy ambiguity remains. Error override and absent-termination compatibility follow the pre-existing writer policy. Concurrency, resource cleanup, and arbitrary diagnostic semantics remain outside the Lean model.

## Final verification results

The serial workspace all-feature command exited 0: 1,613 tests passed, zero failed, 13 ignored across 69 test/doc-test groups. Both oracle tests passed, including the named minimal witness and all 564 corpus/schema executions. `cargo fmt --all -- --check` and `git diff --check` passed. Initial Clippy found `if_not_else` in the simplified sequence branch; the condition and branches were inverted equivalently, retaining lifecycle precedence. Final `cargo clippy --workspace --all-targets --all-features -- -D warnings` exited 0 in 6.99 seconds. Default report-policy and sequence-oracle tests were repeated after that style-only change. No all-feature sequence rejection coverage is inferred from tests excluded under the contracts feature.

Additional final commands:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test -p hoimin-core --test report_policy --test lean_report_sequence_oracle
git diff --check
```

The parallel managed-root test failure remains disclosed as a nonreproduced execution concern, not silently relabeled as a semantic success. Its exact isolated rerun and the serial full-suite run passed. No result-validation mismatch, proof failure, or infrastructure block remains.
