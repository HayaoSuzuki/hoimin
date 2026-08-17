# Issue #330: Cleaning stop-event oracle report

## Result

The repaired Rust transition and Lean state-machine model agree that deadline
and cancellation events are exact no-ops while normal successful cleanup is
pending. Both new strict corpus cases match through the public Rust API. The
complete state-machine corpus now contains 16 strict matches, with no semantic
mismatches or adapter infrastructure errors.

The production repair is one guard change: every `RunPhase::Cleaning` state is
terminal for stop-event handling. The original cleanup effect remains pending,
its effect ID is not retired, the established exit code is unchanged, and its
`CleanupFinished` event proceeds to the final report.

## Durable claim and model boundary

Claim: once terminal cleanup is pending, a deadline or cancellation preserves
the complete modeled state and emits no effects until cleanup completes.

The Lean model includes phase, pending effect ID and kind, completed and
retired identities, stop cause, cleanup/final emission flags, accepted-result
count, and cleanup-to-final ordering. It excludes filesystem and database I/O,
wall-clock timing, Tokio task draining, process supervision, report writing,
and workspace-plan ownership.

The Rust adapter covers the model boundary through `RunState` and `transition`.
The separate Rust machine regression additionally observes exit code, pending
and retired effect identity, and successful final-report continuation.

## What Lean established

`stop_during_cleaning_is_noop` proves inside the model that, for either stop
cause and any state whose phase is `cleaning`, both the resulting state and
emitted-effect list are unchanged.

The theorem was added before the model repair and failed against the old rule:
a cleaning state with no stop cause replaced its pending effect, recorded a
stop cause, and emitted another cleanup. After `Model.stop` made `cleaning`
unconditionally terminal, the proof passed.

`normalCleaningStopWitnessDetected` compares the repaired model with a literal
broken variant that retains the former replacement-cleanup behavior. Corpus
generation refuses to proceed unless this and the existing broken-model
witnesses are all detected. No `sorry`, `admit`, or `axiom` occurs in the
changed model, proof, or case files.

These statements establish properties of the Lean model only; they do not
prove the Rust implementation.

## What the adapter observed

The adapter's `normal_cleaning_with_copy` scenario reaches normal cleanup by
executing public transitions for target resolution, preflight, run-start
output, worker creation, materialization verification, a successful baseline,
empty analysis, and pre-final verification. It does not construct private
`RunState` fields.

Lean owns the expected observations in generated
`formal/HoiminOracle/corpus/state-machine.jsonl`:

| Case | Lean expectation | Rust observation | Classification |
| --- | --- | --- | --- |
| `deadline_during_normal_cleanup_is_noop` | stop: cleaning/no emissions/1 pending; cleanup completion: final pending/final output/1 pending | same two observations | match |
| `cancel_during_normal_cleanup_is_noop` | stop: cleaning/no emissions/1 pending; cleanup completion: final pending/final output/1 pending | same two observations | match |

The focused Rust regression first failed because the old transition emitted a
replacement effect. With the repair, both stop events preserve exit code 0 and
the original pending cleanup ID; completing that effect emits a complete
successful final report.

## Prior contract decision

The 2026-08-09 oracle report intentionally excluded a first stop during normal
cleanup because the original model lacked the shell/workspace premise. Issue
#330 supplies that premise: the shell drains the in-flight blocking cleanup,
and cleanup consumes the workspace plan. Re-running finalization is therefore
neither interruptible nor valid. This change broadens the reviewed contract;
it does not weaken Lean to mirror production behavior.

## Resource safety

Every focused Lean command used a 20-second wall-clock limit, 768 MiB RSS
limit, 250 ms sampling, and one Lake job.

| Command boundary | Result | Elapsed | Peak RSS |
| --- | --- | ---: | ---: |
| Changed proofs | pass | 842 ms | 479,264 KiB |
| Changed cases | pass | 573 ms | 56,272 KiB |
| Corpus generation via `lean --run` | pass | 1,872 ms | 475,136 KiB |
| Corpus freshness via `lean --run` | pass | 2,169 ms | 484,432 KiB |

Three broader attempts were stopped at the fixed RSS boundary: combined cases
plus linked generator reached 808,432 KiB; the linked generator alone reached
792,240 KiB; the aggregate library build reached 926,064 KiB. The limit was
not raised and these stops are resource infrastructure outcomes, not semantic
evidence. Splitting changed targets and using Lean's source runner provided a
bounded generation and freshness path. No larger run was attempted.

## Reproduction

Run from `formal/HoiminOracle` for the bounded Lean checks:

```console
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 \
  --sample-ms 250 --stats /tmp/issue330-proofs.json -- \
  lake -Kjobs=1 build HoiminOracle.Proofs
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 \
  --sample-ms 250 --stats /tmp/issue330-cases.json -- \
  lake -Kjobs=1 build HoiminOracle.Cases
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 \
  --sample-ms 250 --stats /tmp/issue330-corpus.json -- \
  lake env lean --run Main.lean -- --check corpus/state-machine.jsonl
```

Run from the repository root for implementation correspondence:

```console
cargo test -p hoimin-core --test lean_oracle
HOIMIN_ORACLE_CASE=deadline_during_normal_cleanup_is_noop \
  cargo test -p hoimin-core --test lean_oracle \
  oracle_correspondence -- --exact --nocapture
HOIMIN_ORACLE_CASE=cancel_during_normal_cleanup_is_noop \
  cargo test -p hoimin-core --test lean_oracle \
  oracle_correspondence -- --exact --nocapture
cargo test -p hoimin-core --test machine \
  stop_signals_do_not_retire_normal_cleanup_or_change_success -- --exact
```

## Unresolved decisions

There are no unresolved semantic mismatches, ownership decisions, or adapter
infrastructure errors. The aggregate Lean build's resource requirement remains
outside this focused issue; the bounded changed-target and corpus checks are
the retained verification path.
