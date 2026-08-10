# Lean result lifecycle consistency audit design

## Objective

Audit whether an accepted mutant result keeps one identity and one
classification while it crosses Hoimin's machine, optional session
persistence, report output, final summary, and metrics accounting boundaries.
The audit is evidence-only: it may add a Lean model, generated corpus, Rust
correspondence adapter, tests, and reports, but it does not repair production
behavior. A confirmed implementation mismatch receives a detailed
counterexample ledger and a separately scoped Issue.

This audit follows the completed state-machine, session recovery, shutdown,
workspace, and budget audits. Those audits proved or exercised important local
contracts, but deliberately excluded full correspondence for individual mutant
identities and accepted-result accounting across components.

## Approaches considered

### Recommended: a dedicated compositional result-lifecycle model

Add a small `ResultLifecycle` namespace to the existing pinned Lean project.
Model only result acceptance, persistence, report emission, summary recording,
metrics execution accounting, stop interleavings, and finalization. Generate a
versioned corpus and compare its stable projection with public CLI output,
SQLite session state, and the metrics sidecar. This keeps the audit claim
narrow while exercising real component composition.

### Extend the existing state-machine or shutdown model

This would reuse transition definitions, but both models already abstract away
the individual result identity or the complete external observations needed by
this audit. Adding session rows, report events, and metrics would turn either
model into a second implementation of the whole runner and make
correspondence defects harder to distinguish from model defects.

### Use Rust property tests without a Lean oracle

Property tests would be cheaper and useful as follow-up regression coverage,
but they would encode schedules and expected results in the same language and
test layer as the implementation. They would not provide an independently
checked contract, deterministic generated corpus, or theorem-level statement
of the surviving invariant.

## Owned contract

The central claim is:

> Once a mutant result is accepted, later persistence, output, stop, cleanup,
> session finish, report, and metrics events neither lose nor duplicate its
> identity, and every observable status-bearing surface reports the same
> classification.

The audit expands the claim into these properties:

- an executed mutant identity is accepted at most once;
- session persistence precedes summary recording when a session is active;
- persistence rejection does not create a durable row or silently discard the
  accepted classification from the incomplete report;
- a successful persisted result has the same mutant identity and status as the
  corresponding `mutant_finished` report event;
- a report event for a real execution is emitted at most once;
- stopping while a result is persisting or finishing preserves that result and
  does not replace it with `not_run`;
- stopped candidates that never produced a process result may be reported as
  `not_run`, but do not increase the executed metric;
- final summary counts equal the multiset of emitted `mutant_finished`
  statuses;
- metrics `executed` equals the number of accepted real mutant process
  completions, independent of later persistence or report failures;
- report and metrics write failures do not change the already selected run
  exit result or accepted classifications;
- rejected semantic events preserve the complete modeled state.

The model excludes candidate discovery and ranking, baseline classification,
workspace mutation correctness, process termination classification itself,
wall-clock durations, output bytes, OS resource-control internals, filesystem
durability below successful SQLite or file operations, allocator failure, and
machine power loss. Those are separate contracts. The audit treats the
`ProcessTermination -> MutationStatus` classification as an input mapping and
checks only that the chosen status is conserved afterwards.

## Correspondence worksheet

Every generated or selected case has exactly one mode. A different-premise
observation is never classified as an implementation mismatch.

| Premise or observation | Lean representation | Production configuration | Stable observation | Evidence | Mode |
| --- | --- | --- | --- | --- | --- |
| Complete sessionless run with one or two determinate results | accepted result roles followed by output and finalization | public `hoimin run` against an isolated Python fixture | JSON/JSONL mutant events and final counts | public CLI adapter | `strict` |
| Complete run with a session | persist before output and complete finish | public `--session` run | JSON report plus SQLite candidates, results, and run flags | public CLI adapter | `strict` |
| Resume reuses a determinate stored result | load stored result without a new process completion | public incomplete session followed by `--resume` | report status, unchanged durable result, metrics `executed` | public CLI adapter | `strict` |
| Total timeout or cancellation after at least one accepted result | stop interleaved after result acceptance | public timeout or real signal fixture | incomplete JSON report, SQLite state, metrics sidecar, exit code | public CLI adapter on supported platforms | `strict` |
| Metrics destination failure | metrics write fails after semantic run completion | public `--metrics` path that cannot be replaced | unchanged report and exit code plus warning | public CLI adapter | `strict` |
| Session persistence failure before result acknowledgement | persist returns a typed failure | public SQLite trigger rejects result insertion | report event, final summary, empty durable result rows, diagnostic | public CLI adapter | `strict` |
| Stop exactly between process completion acceptance and persistence acknowledgement | split persist boundary | existing in-process control and handler seam | machine summary, emitted events, session rows | owned Rust fixture | `internal-fixture` |
| Stop exactly between summary recording and report acknowledgement | split output boundary | public API cannot pause this private effect identity boundary | model state and generated witness only | Lean exploration | `model-only` |
| Duplicate or stale completion identity injection | duplicate semantic event | public CLI cannot inject a private `EffectId` | model verdict and state | Lean plus existing machine tests | `model-only` |
| Corpus parse, fixture setup, timeout, signal delivery, SQLite query, or adapter panic | no semantic transition | not applicable | harness diagnostic | adapter isolation boundary | `infrastructure-error` |

Only the same projection is compared. For example, metrics has no per-status
fields, so strict metrics correspondence compares `run_id`, `discovered`,
`executed`, and completed worker process totals, not a status classification
that the file cannot expose.

## Formal model

Add a dependency-free result-lifecycle model to `formal/HoiminOracle` with:

- two semantic mutant identities;
- status roles `killed`, `survived`, `timeout`, `out_of_memory`,
  `process_limit`, `error`, and `not_run`;
- per-mutant states for undispatched, running, accepted, persistence pending,
  persisted, report pending, reported, and stopped-not-run;
- optional session and metrics configuration;
- durable session rows keyed by mutant identity;
- ordered report events carrying identity and status;
- a summary multiset by status;
- discovered, accepted-process, and metrics-executed counts;
- stop state, final completeness, exit selection, and failure diagnostics;
- explicit verdicts for rejected, duplicate, stale, or wrong-identity events.

Events cover discovery, dispatch, process acceptance, persistence success or
failure, report success or failure, cancellation, deadline, stopped-candidate
drain, session finish, metrics finish or failure, and final return. The correct
transition makes public milestones atomic. Separate model-only events expose
the minimum internal boundaries required for sensitivity and stop races.

The explorer enumerates traces shortest-first in stable event order. The
initial bound is two mutant identities, the seven status roles, and depth nine.
The bound may be reduced only if all fixed witnesses remain reachable and the
report records the new boundary. Exact-state deduplication happens only after
all outgoing transitions for the current frontier have been checked. No
symmetry reduction may remove either retained fixed witness.

## Invariants and proofs

Imported Lean modules keep only cheap semantics and kernel-checked proofs. The
named invariant includes:

- durable session rows are unique by mutant identity;
- report events are unique by mutant identity;
- every durable or reported result is backed by one accepted result with the
  same status;
- each summary count equals the number of reported results of that status;
- metrics executed never exceeds accepted real process completions;
- stopped-not-run results never increase executed;
- final complete state has no accepted result left unreported;
- an installed stop cause and accepted result status are not overwritten;
- rejected events preserve the complete state.

The audit proves transition-local preservation and lifts the structural
invariant across arbitrary finite traces from the initial state. Properties
that quantify over the full combined safety predicate may remain bounded, but
the report must distinguish those checks from unbounded theorems.

## Refutation and sensitivity

Before trusting the correct model, fixed broken variants must be detected for
all applicable risk families:

- **atomicity/transactionality:** record the summary before persistence, then
  drop the accepted result when persistence fails;
- **uniqueness/idempotency:** accept or report the same mutant identity twice;
- **boundary/precedence:** convert an accepted result to `not_run` when a stop
  arrives during persistence or report delivery;
- **cross-surface consistency:** persist one status but emit or summarize a
  different status;
- **metrics conservation:** increment executed for stopped-not-run work or omit
  a real accepted completion after drain.

Each broken family retains a deterministic minimal trace with intermediate
states. Failure to detect an applicable broken variant is an audit failure.
The executable reports depth, alphabet size, reachable states, checked
transitions, and each witness.

## Corpus and implementation adapter

Lean owns a deterministic JSONL corpus at
`formal/HoiminOracle/corpus/result-lifecycle.jsonl`. Every case declares its
schema, unique ID, exact mode, scenario, semantic schedule, and expected stable
projection. The corpus is generated by a dedicated executable and is never
hand-edited.

The Rust adapter belongs in `crates/hoimin-cli/tests` and invokes the real
public CLI for strict cases. It creates isolated projects, session databases,
and metrics paths; parses JSON or JSONL output; independently queries SQLite;
and compares only corpus-owned expected fields. It does not recompute the
expected summary or status. Each case runs in isolation and supports a
single-case environment variable. Panics, setup failures, timeouts, malformed
output, and unavailable platform primitives are reported as
`infrastructure-error`, not semantic mismatches.

The adapter first runs in report mode. Reviewed strict matches become blocking
correspondence cases. A strict mismatch remains explicit and reproducible; it
is not added to an allowlist merely to make CI green. Existing machine,
session, report, metrics, and shutdown tests remain independent evidence.

## Files and placement

The planned additions are:

- `formal/HoiminOracle/HoiminOracle/ResultLifecycleModel.lean` for semantics;
- `formal/HoiminOracle/HoiminOracle/ResultLifecycleProofs.lean` for proofs and
  fixed broken witnesses;
- `formal/HoiminOracle/HoiminOracle/ResultLifecycleCases.lean` for corpus cases
  and deterministic encoding;
- `formal/HoiminOracle/ResultLifecycleAuditMain.lean` for exploration,
  statistics, sensitivity, shrinking, and corpus generation;
- `formal/HoiminOracle/corpus/result-lifecycle.jsonl` generated by Lean;
- `crates/hoimin-cli/tests/lean_result_lifecycle_oracle.rs` for implementation
  correspondence;
- a self-contained report under `docs/superpowers/reports/`;
- a counterexample ledger when a mismatch is found;
- this design and a task-by-task implementation plan under
  `docs/superpowers/`.

`HoiminOracle.lean` imports the model, proofs, and cheap case definitions. The
library root does not import `ResultLifecycleAuditMain`; exhaustive search,
`native_decide`, shrinking, statistics, and serialization remain behind the
executable boundary.

## Verification

The independent gates are:

1. pinned Lean build and theorem checks;
2. fixed broken-family sensitivity checks;
3. deterministic corpus generation and freshness check;
4. adapter parser and projection tests;
5. report-mode execution of every generated case;
6. strict correspondence for reviewed cases;
7. single-case replay for every mismatch;
8. focused existing machine, session, report, metrics, and shutdown tests;
9. Rust formatting, Clippy, full workspace tests, and diff hygiene.

The final report records exact commands, elapsed cost, corpus size, finite
domain and depth, event alphabet, reachable states, checked transitions,
reduction rules, theorem premises, sensitivity witnesses, per-mode results,
model limitations, and owner decisions. It states what Lean established about
the model separately from what the adapter observed in Hoimin.

## Result handling and continuation

The audit classifies every witness as `confirmed bug`, `specification
ambiguity`, `model defect`, or `infrastructure error`. It does not weaken the
model to match current implementation behavior and does not change production
code. A confirmed bug is handed off with its shortest trace, intermediate
states, expected and actual observations, differing fields, likely source
locations, impact, exact reproduction command, and recommended repair scope.

After this audit is complete and its ledger is resolved or handed off, the
next independent worktree audits candidate spool/ranking/selection, followed
by schema migration concurrency, AST fact-flow joins, and process-output
accounting in the approved priority order.
