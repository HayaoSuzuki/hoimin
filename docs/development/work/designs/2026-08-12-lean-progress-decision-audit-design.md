# Lean progress decision audit design

## Goal

Formally audit the decision core behind `hoimin progress`, compare a
Lean-generated corpus with the public CLI under the same premises, and repair
any confirmed Rust mismatch with a failing regression test first.

The durable claim is:

> Progress compares only adjacent usable reports. A comparable pair is
> regressing when any conclusive mutant regresses, improving when at least one
> improves and none regress, stalled when conclusive common mutants exist and
> none change, and indeterminate otherwise. Saturation occurs exactly when the
> latest comparison ends a stalled suffix whose length reaches patience.

This work is isolated in `.worktrees/lean-progress-decision` on branch
`audit/lean-progress-decision`. The specification, implementation plan, Lean
artifacts, generated corpus, Rust adapter, regression repair if needed, and
audit report all belong to that worktree and branch.

## Scope

Included behavior:

- adjacent usable versus unusable report histories;
- matching, different, and duplicate candidate-ID sets;
- stable-ID joining when candidate sets match;
- content-key ambiguity when candidate sets differ;
- killed, survived, and the semantically equivalent inconclusive class;
- common, added, removed, ambiguous, inconclusive, improvement, regression,
  and carried-survivor counts;
- regression precedence over simultaneous improvement;
- stalled-suffix reset and patience saturation boundaries;
- public JSON progress observations derived from owned schema-v2 reports.

Excluded behavior:

- report JSON parsing mechanics and schema validation, except as adapter
  infrastructure;
- mutation execution, report production, filesystem timing, and rendering
  prose;
- exact floating-point behavior for counts beyond exact `f64` integer
  representation;
- cross-platform path normalization and case folding;
- histories that a public report parser rejects, except as disclosed
  `model-only` or `internal-fixture` sensitivity evidence.

The strict corpus uses small counts whose rational scores are represented
exactly (`0`, `1/2`, and `1`).

## Correspondence worksheet

| Premise or observation | Lean representation | Production configuration | Public observation | Evidence | Mode |
| --- | --- | --- | --- | --- | --- |
| usable report | `Report.usable mutants` | complete schema-v2 report with successful baseline | public progress JSON | owned report fixture through CLI | `strict` |
| unusable gap | `Report.unusable` | missing/failed baseline or incomplete report | latest state and comparison omission | owned report fixture through CLI | `strict` |
| matching unique candidate IDs | two unique role sets | candidate IDs in report mutants | counts and state | public CLI | `strict` |
| different candidate IDs | unequal unique role sets | distinct candidate IDs | indeterminate state and counts | public CLI | `strict` |
| duplicate candidate ID | duplicate role in one report | public parser rejects it before comparison | no semantic observation | parser evidence only | `model-only` |
| duplicate content key under different IDs | repeated abstract content role | different unique IDs sharing candidate content | ambiguous count and state | public CLI | `strict` |
| conclusive status | `Status.killed` / `Status.survived` | corresponding mutation status | transition counts and scores | public CLI | `strict` |
| inconclusive status | `Status.inconclusive` | timeout, OOM, process-limit, error, or not-run | inconclusive count and indeterminate state | one representative per concrete status | `strict` |
| positive patience | `patience : Nat`, `0 < patience` | CLI positive integer | suffix count and saturated flag | public CLI | `strict` |
| exact score | rational killed/decidable pair | small killed/survived counts | JSON `f64` score | exact `0`, `0.5`, `1` cases | `strict` |

Uncertain cases begin `model-only` and are promoted only when the same public
premise and complete observation are available. Parser failures, process
failures, timeouts, and malformed output are `infrastructure-error`, never
semantic mismatches.

## Architecture

### Pair decision model

A focused Lean module represents mutants with candidate-ID and content-key
roles plus a reduced status. It computes candidate-set eligibility, selects
the stable-ID or content-key join, excludes ambiguous content keys and
inconclusive pairs, and produces exact integer observations plus rational score
numerators and denominators.

The pair classifier applies this precedence:

1. nonmatching eligibility or zero comparable common mutants is
   `indeterminate`;
2. any regression is `regressing`;
3. otherwise any improvement is `improving`;
4. otherwise the result is `stalled`.

### History decision model

A second pure layer consumes reports oldest to newest. It compares only
adjacent usable reports. Unusable adjacency produces no comparison and resets
the stalled suffix. Improving, regressing, and indeterminate comparisons also
reset it. Stalled comparisons increment it, and the public latest state becomes
`saturated` once the positive patience threshold is reached.

### Executable boundary

Imported modules contain only pure semantics and kernel-checked proofs. Fixed
cases, bounded enumeration, shrinking, sensitivity checks, statistics, and
JSONL serialization live in `ProgressDecisionAuditMain.lean`, which is not
imported by the library root except through its cheap case definitions where
required by existing project convention.

## Lean theorems

The proof module will establish with explicit premises:

- the final `consecutiveStalls` equals the length of the trailing stalled
  comparison suffix since the latest reset boundary;
- unusable adjacency, improving, regressing, and indeterminate results reset
  the suffix to zero;
- saturation implies the latest semantic comparison is stalled and the suffix
  length is at least patience;
- a stalled suffix reaching patience yields saturation;
- individual pair comparisons never directly produce `saturated`;
- regression takes precedence over simultaneous improvement;
- matching unique candidate-ID sets use ID correspondence independent of
  duplicate content keys;
- inconclusive pairs never contribute improvements, regressions, or score
  denominators.

Theorems use local `maxHeartbeats 100000` limits where nontrivial. Proofs about
the Lean model are not claims about Rust.

## Bounded refutation

Start with the semantic domain:

- two candidate-ID roles;
- two content-key roles;
- statuses `killed`, `survived`, `inconclusive`;
- report histories of length at most four;
- patience values one through three.

Generate histories shortest-first in stable role/status order. Deduplicate
semantic states, preserve fixed sensitivity witnesses outside any pruning, and
record report count, pair count, explored histories/states, elapsed time, and
peak RSS. Do not increase the history bound unless an uncovered stated claim
requires it and the measured prior bound remains safe.

Bounded exploration checks the stated finite domain; it is not an unbounded
proof.

## Refutation sensitivity

The executable must distinguish these broken variants before it may write or
check the corpus:

- classify simultaneous regression and improvement as improving;
- retain the stalled suffix across an indeterminate comparison;
- retain the suffix across an unusable adjacency;
- saturate only when suffix length is strictly greater than patience;
- join matching ID sets by content key instead of stable ID;
- treat duplicate content keys as unique when ID sets differ;
- count inconclusive transitions in score or directional judgments.

Uniqueness/idempotency applies through duplicate ID/content correspondence and
is covered. Boundary/precedence applies through patience and regression
priority and is covered. Atomicity/transactionality does not apply because the
audited computation is pure and has no partial state mutation or persistence.

## Corpus and public adapter

Lean defines every expected case once and emits deterministic JSON Lines.
Strict cases include:

- one, two, and three adjacent stalls at patience boundaries;
- regression or improvement between stalled comparisons;
- simultaneous improvement and regression;
- an inconclusive transition between stalls;
- a usable/unusable/usable gap;
- matching stable IDs with duplicate content keys;
- changed candidate-ID sets;
- duplicate content keys under different ID sets;
- added and removed mutants;
- each concrete Rust inconclusive status;
- empty comparable common set;
- exact `0`, `0.5`, and `1` score projections.

The Rust integration adapter generates isolated owned schema-v2 report files,
invokes the public `hoimin progress --format json` entry point, and compares
state, suffix count, saturated flag, counts, and exact small scores. It does
not independently encode expected decisions. Every case runs in isolation.

## Rust repair policy

If strict correspondence finds a mismatch:

1. preserve the smallest Lean witness in the generated corpus;
2. add the same public-report Rust regression test and observe the semantic
   failure;
3. make the smallest production correction;
4. rerun focused and full verification without weakening Lean.

If strict correspondence matches, production behavior is not changed merely
to produce a code diff.

## Resource limits and baseline

Run one Lean command at a time with a 20-second wall-clock deadline, a 768 MiB
(786,432 KiB) process-tree RSS ceiling, 250 ms samples, and one Lake build job.
Never use unlimited heartbeats or raise the cap after a resource failure.

The worktree baseline is:

- `cargo build --workspace`: pass;
- `cargo test -p hoimin-cli --test progress`: 57 passed;
- clean aggregate `lake -Kjobs=1 build`: stopped by the fixed RSS guard after
  586 ms at 950,352 KiB (`infrastructure-error`, no semantic conclusion).

The aggregate Lean build is not retried with a larger cap. New evidence uses
focused module builds, a proof consumer, the audit executable, and corpus
freshness checks individually under the same fixed limits. The aggregate
resource boundary and every focused measurement are retained in the report.

## Deliverables

- Lean pair/history model, proof, and fixed-case modules;
- resource-bounded executable with cases, sensitivity, statistics, corpus
  generation, and freshness commands;
- versioned JSONL corpus;
- public Rust CLI correspondence adapter;
- Lean-witnessed Rust regression and minimal repair if a mismatch is confirmed;
- this specification, an implementation plan, and a self-contained audit
  report with exact commands and a counterexample ledger.
