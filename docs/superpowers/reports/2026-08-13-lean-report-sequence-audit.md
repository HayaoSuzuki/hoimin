# Lean report-sequence correspondence audit

Date: 2026-08-13  
Branch: `audit/lean-report-sequence`  
Base: `origin/main` at `98d32a6`

## Result

The focused audit found no production mismatch. All 23 Lean-owned executable
cases matched the public Rust `ReportSequence::observe` behavior, including
the exact error variant, all contract-relevant error fields, and each supplied
post-rejection probe. Consequently no Rust production code or existing
`report_policy` test was changed, and no counterexample ledger was created.

This is a correspondence result for the finite executable corpus plus
kernel-checked facts about the Lean model. It is not a proof of the Rust
implementation.

## Claim and boundary

The audited claim is:

> An accepted public report-event trace has one run identity, strictly
> increasing report sequence numbers, unique stable mutant identities,
> matched mutant start/finish lifecycles, no active mutant at run finish, and
> no event after run finish. A rejected event does not affect the later public
> observations exercised by its probe.

Included are `RunStarted`, `MutantStarted`, `MutantFinished`, `Diagnostic`, and
`RunFinished`; run and mutant lifecycles; run identity; report sequence
monotonicity; mutant identity stability; status/termination correspondence;
rejection precedence; and transactional rejection.

Excluded are JSON/output-sink I/O, `RunState` scheduling and effect completion,
summary/score arithmetic, session/metrics/workspace/process lifecycles,
pre-event sequence allocation or overflow, and private Rust state equality.

Declared behavior comes from the public event types, `ReportSequenceError`,
`ReportSequence::observe` documentation, and existing `report_policy` tests.
The audit also makes these implementation-significant rules explicit:

- lifecycle and identity validation precede sequence validation;
- an active duplicate start, same-sequence reuse after finish, and
  changed-sequence reuse are distinct errors;
- a known mutant with the wrong sequence differs from an unknown/not-active
  mutant finish;
- absent termination skips classification, while present termination must map
  to the reported status;
- rejection is atomic even after multiple protocol dimensions were inspected.

## Correspondence worksheet

| Premise/observation | Lean | Public Rust configuration/observation | Final mode |
| --- | --- | --- | --- |
| event kind | `Event.kind` | `OutputEvent` variant; `observe` result | `strict` |
| report sequence | `Nat` | representable corpus `u64`; acceptance or `NotMonotonic` | `strict` |
| run identity | two normalized `RunId` values | two public `String` values; acceptance or `RunIdMismatch` | `strict` |
| mutant identity | two normalized `MutantId` values | two public candidate IDs; typed error | `strict` |
| mutant sequence | `0` or `1` in bounded inputs | public `u64`; typed error | `strict` |
| active lifecycle | active and seen lists | public start/finish prefix and later probe | `strict` |
| status/termination | all seven statuses and six termination classes | public enums; acceptance or exact mismatch fields | `strict` |
| rejection atomicity | exact rejected-state equality | valid follow-up event on the same owned `ReportSequence` | `strict` for the probe observation |
| complete private state equality | `State` equality | not publicly observable | theorem-only, `model-only` claim |

The JSONL contract is closed: schema version, modes, enum spellings, event
shape, error-field sets, unique IDs, and paired probes are validated before
replay. Rust maps inputs but does not recompute the expected semantics.

## Lean model and proofs

The pure transition validates in production order: lifecycle/run identity,
mutant lifecycle/identity/status, sequence monotonicity, then mutation. The
following theorems were kernel checked without `sorry`, `admit`, or audit
axioms:

| Theorem | Premise and conclusion |
| --- | --- |
| `rejected_preserves_state` | if `step` returns a rejection, its entire Lean state equals the input state |
| `accepted_sequence_advances` | with a previous sequence and an accepted step, the new sequence is strictly greater |
| `accepted_run_id_is_stable` | after a run ID is present, an accepted event carries that same ID |
| `seen_identity_is_stable` | an existing mutant-ID mapping remains the same after any accepted step |
| `finish_removes_active` | an accepted mutant finish erases exactly its `(ID, sequence)` active pair |
| `run_finish_requires_no_active` | an accepted run finish requires an empty active list |
| `finished_is_terminal` | every later event is rejected as `runAlreadyFinished` |
| `step_preserves_invariant` | a step preserves the named terminal-state invariant |
| `run_preserves_invariant` | every arbitrary Lean trace from `initial` preserves that invariant |

The named invariant is `finished = true -> active = []`. Identity stability,
sequence growth, exact rejection atomicity, and terminal behavior are separate
general theorems rather than hidden inside that predicate.

## Finite refutation and sensitivity

The executable alphabet has 42 events from two run IDs, two mutant IDs,
mutant sequences `0/1`, report sequences `0/1/2`, and representative start,
finish, diagnostic, and final events. Search depth is 4. The printed
`generated_traces=3,187,591` is the theoretical count through depth 4; the
state-deduplicated explorer retained 31 states and checked 3,738 outgoing
transitions. This bounded exploration is a finite check, not a proof.

Depth 4 was the first selected bound containing all fixed witnesses. It met
the resource limits and all named coverage goals, so no larger bound was
attempted or abandoned.

| Risk family | Deliberately broken behavior | Minimal fixed witness | Detected |
| --- | --- | --- | --- |
| atomicity | mutate/advance before validation | start; reject cross-run sequence 2; valid same-run probe sequence 2 | yes |
| uniqueness | allow mutant-ID reuse | start run; start alpha; finish alpha; start alpha again | yes |
| equality boundary | accept equal report sequence | run start at 1; diagnostic at 1 | yes |
| precedence | check sequence before lifecycle | run start at 1; cross-run diagnostic at 1 | yes |

Corpus totals are 23 fixed cases: 23 `strict`, 0 `model-only`, 0
`internal-fixture`. They comprise 17 protocol/lifecycle cases and 6 complete
termination-to-status mappings. All four sensitivity booleans were `true`.

## Rust observations and reconciliation

Every corpus row was first replayed individually with
`HOIMIN_REPORT_SEQUENCE_CASE=<id>` and then as one suite. Every strict target
and probe matched. The focused adapter has five passing tests in default
configuration: four closed-schema tests and one 23-row correspondence test.
With `contracts`, the four schema tests pass; runtime-invalid replay is
intentionally excluded because that feature turns rejected contract inputs
into panics rather than `Result` observations.

There were no semantic non-matches to classify as a confirmed bug,
specification ambiguity, or model defect. Therefore the reconciliation
decision is explicit no-repair: changing `crates/hoimin-core/src/report.rs`
would be speculative and was not done.

Two setup incidents were classified as infrastructure errors and resolved
without semantic conclusions:

- the initial worktree test lacked its local `.venv` link; the standard
  untracked `.venv -> ../../.venv` link restored the green baseline;
- the first sandboxed resource-monitor invocation exited 126; rerunning the
  same guarded commands with the required monitor permission produced the
  successful measurements below.

No infrastructure error remained in final correspondence execution.

## Resource evidence

Each Lean command used a 20 s wall timeout, 768 MiB aggregate RSS limit,
250 ms sampling, and single-job Lake builds. All final guarded commands exited
0 with reason `child_exit`.

| Command group | Elapsed | Peak RSS |
| --- | ---: | ---: |
| model build | 327 ms | 3,568 KiB |
| proof build | 320 ms | 2,800 KiB |
| external proof consumer | 581 ms | 570,752 KiB |
| sensitivity | 570 ms | 675,296 KiB |
| case listing | 561 ms | 666,128 KiB |
| bounded statistics | 567 ms | 665,232 KiB |
| corpus freshness | 553 ms | 351,344 KiB |

The higher executable RSS values include Lake/Lean child processes and remain
below the aggregate 786,432 KiB cap.

## Verification

Final results:

- guarded Lean model, proof, consumer, sensitivity, cases, stats, and corpus
  freshness: pass;
- `cargo test -p hoimin-core --test lean_report_sequence_oracle`: 5/5 pass;
- `cargo test -p hoimin-core --test report_policy`: 22/22 pass;
- `cargo test -p hoimin-core --all-features --test report_policy`: 13/13 pass;
- `cargo fmt --all -- --check`: pass;
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: pass;
- `cargo test --workspace --all-features --quiet`: pass (no failures);
- no unresolved placeholders or proof escape hatches were found;
  `git diff --check` passes.

Exact focused reproduction from `formal/HoiminOracle`:

```bash
python3 ../../tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/report-sequence-model.json -- lake -Kjobs=1 build HoiminOracle.ReportSequenceModel
python3 ../../tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/report-sequence-proofs.json -- lake -Kjobs=1 build HoiminOracle.ReportSequenceProofs
python3 ../../tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/report-sequence-consumer.json -- lake env lean /tmp/hoimin-report-sequence-proof-consumer.lean
python3 ../../tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/report-sequence-sensitivity.json -- lake exe generate_report_sequence -- --sensitivity
python3 ../../tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/report-sequence-cases.json -- lake exe generate_report_sequence -- --cases
python3 ../../tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/report-sequence-stats.json -- lake exe generate_report_sequence -- --stats
python3 ../../tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/report-sequence-freshness.json -- lake exe generate_report_sequence -- --check corpus/report-sequence.jsonl
```

Exact Rust and repository reproduction from the worktree root:

```bash
cargo test -p hoimin-core --test lean_report_sequence_oracle -- --nocapture
cargo test -p hoimin-core --test report_policy
cargo test -p hoimin-core --all-features --test report_policy
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features --quiet
rg -n "sorry|admit|axiom" formal/HoiminOracle/HoiminOracle/ReportSequence\*.lean formal/HoiminOracle/ReportSequenceAuditMain.lean
git diff --check
```

To replay one strict row, replace `<id>` with a corpus ID:

```bash
HOIMIN_REPORT_SEQUENCE_CASE=<id> cargo test -p hoimin-core --test lean_report_sequence_oracle correspondence::strict_public_report_sequence_observations_match_lean -- --exact --nocapture
```

## Limitations

- The executable domain normalizes identities to two roles and bounds input
  report sequences to `0/1/2` and mutant sequences to `0/1`; general Lean
  proofs cover natural numbers, but Rust replay covers only corpus values.
- The fixed cases cover all public rejection constructors and all termination
  mappings, not every possible trace or payload value.
- Probe equality establishes the selected complete public continuation, not
  byte-for-byte equality of private Rust state.
- The correspondence adapter tests in-band `Result` rejection semantics and
  deliberately does not reinterpret `contracts`-feature panics as results.
- JSON serialization, reporting sinks, concurrency, scheduling, recovery, and
  summary accounting remain outside this slice.
