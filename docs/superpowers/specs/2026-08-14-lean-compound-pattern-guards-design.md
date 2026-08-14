# Lean Compound Pattern and Guard Audit Design

Date: 2026-08-14
Issue: #300, phase 4

## Audited claim

Hoimin must derive a match case's known-import environment from the reachable
outcomes of its compound pattern and guard. A successful OR arm contributes to
the case-body meet. A pattern failure carries bindings that may have occurred
before that failure to the next case. An AS alias, mapping rest capture, or
class capture cannot affect a failure that occurs before its binding point. A
false guard carries the successful pattern bindings and the guard's writes to
the next case. The final match join includes only reachable case completions
and the final unmatched path.

This phase audits Hoimin's static known-import transfer. It does not claim a
complete operational semantics for Python pattern matching. Lean proves the
reduced model. Rust tests compare fixed model expectations with production
observations at the same source sites.

## Included behavior

- valid two-arm OR patterns with the same capture set;
- the meet of successful OR-arm environments;
- failed OR-arm propagation to the next case;
- AS aliases after a refutable child pattern;
- mapping value patterns and `**rest` captures;
- class positional and keyword subpatterns;
- writes made by a false guard after a compound-pattern success;
- preservation of an unrelated supported typing import;
- complete internal known-import facts and public CLI candidate records; and
- a model-only unequal-arm witness because Python rejects unequal OR capture
  sets before Hoimin receives a valid AST.

## Excluded behavior

- parser correctness and recovery ASTs;
- runtime value extraction, equality, attribute lookup, and exceptions raised
  while matching;
- the exact persistence policy of CPython locals after a failed match;
- sequence and star patterns except where a short refutable child gives AS or
  OR a binding point;
- nested match exit categories covered by phase 2;
- arbitrary syntax products, generated AST depth, and arbitrary name counts;
- `except*`, reserved for phase 5; and
- candidate ranking, mutation execution, and Python test-runner behavior.

## Approach

The audit uses a structural attempt algebra. Two alternatives were rejected.
A full Python pattern evaluator would require runtime object and exception
semantics that Hoimin does not model. Adding fixture rows to the old
`PatternResult` record would leave OR-arm and binding-point composition outside
the proof.

The structural model gives each pattern attempt:

- `matched : List Env`, one environment per reachable successful route;
- `failed : List Env`, one environment per reachable failed route; and
- a closed pattern constructor that determines how child attempts compose.

Primitive tests may succeed or fail without writing a name. A capture changes
successful environments and has no failure of its own. Sequential composition
retains failures from the completed prefix and runs the next component only
from prefix successes. AS runs the child before binding its alias. Mapping and
class patterns use the same sequence operator around their structural test and
ordered child patterns.

OR starts each arm from the same incoming environment. The case body receives
the meet of all reachable arm successes. The next case receives the meet of
the reachable arm failures. Valid Python OR rows bind the same names on each
successful arm. A fixed model-only witness gives the arms different binding
sets and detects an implementation that keeps one arm instead of meeting both.

Guard evaluation starts from the successful-pattern meet. A true guard reaches
the body after its writes. A false guard adds that post-guard environment to
the pattern-failure frontier for the next case. The model reuses
`BindingFlow.Env`, `meetOption`, and the ordered case sequencing established by
`ExceptionMatchBinding`.

## Proof obligations

The imported Lean proof module will establish these results with visible
premises:

1. sequential composition retains a prefix failure without applying a later
   capture;
2. AS binds its alias on each success and preserves child failures;
3. mapping rest capture affects success after required children complete;
4. a final class capture does not affect failures from an earlier class test or
   child;
5. every reachable OR success participates in the body-entry meet;
6. every reachable OR failure participates in the next-case meet;
7. a name retained by the OR success meet appears with the same fact in each
   reachable arm success;
8. false-guard propagation uses the post-pattern, post-guard environment;
9. unrelated names survive captures and guard writes; and
10. unreachable pattern outcomes do not participate in a meet.

Non-trivial declarations use a local finite `maxHeartbeats`. Imported modules
may not contain `native_decide`, serialization, `sorry`, `admit`, a custom
axiom, or `maxHeartbeats 0`.

## Correspondence worksheet

The adapter will use exact Python sources and one unique marker per row. It
will call the production `AnnotationCollector` or the public `plan` path. It
will not reproduce pattern traversal.

| Boundary | Lean representation | Production configuration | Complete observation | Mode |
|---|---|---|---|---|
| OR success from list and mapping arms | Two arm attempts start from one environment and bind `Sequence` | `case [0, Sequence] \| {"item": Sequence}` | Case-body entry facts at a unique annotation marker | `internal-fixture` |
| OR failure reaches the following case | Reachable failures from both arms meet before the successor | The same OR pattern followed by `case _` | Following-case entry facts | `internal-fixture` |
| AS child fails before alias binding | Refutable child failures precede `capture Sequence` | `case [0] as Sequence`, followed by `case _` | Following-case entry facts | `internal-fixture` |
| AS failure keeps a public candidate | The failed environment retains known `Sequence` | One following-case `Sequence[int]` annotation | Candidate count, path, byte span, operator, original, replacement, and symbol | `strict` |
| Mapping required child fails before rest capture | Required child failure precedes `**Sequence` | `case {"tag": 0, **Sequence}`, followed by `case _` | Following-case entry facts | `internal-fixture` |
| Mapping-rest failure keeps a public candidate | The failed environment retains known `Sequence` | One following-case `Sequence[int]` annotation | Complete overlapping candidate record | `strict` |
| Class test or early child fails before final capture | Earlier failures precede the final `Sequence` capture | `case Point(0, tail=Sequence)`, followed by `case _` | Following-case entry facts | `internal-fixture` |
| Class failure keeps a public candidate | The failed environment retains known `Sequence` | One following-case `Sequence[int]` annotation | Complete overlapping candidate record | `strict` |
| False guard combines pattern and guard writes | Pattern success binds `Mapping`; guard assignment binds `Sequence` and returns false | Compound capture plus a walrus guard, followed by `case _` | Following-case entry facts for both names | `internal-fixture` |
| Unrelated import survives | Pattern and guard write `Sequence` only | A source also imports `Mapping` | Full fact set, including known `Mapping` | `internal-fixture` |
| Unequal OR arm capture sets | Two abstract arms produce different successful facts | Python rejects the source before a valid production AST exists | Lean witness only | `model-only` |

Fixture creation, parsing, marker selection, timeout, panic, malformed output,
and missing observation failures use `infrastructure-error`. They make no
semantic claim.

## Fixed sensitivity families

The executable will distinguish the correct model from these broken variants:

| Family | Broken transition |
|---|---|
| `pre-pattern-failure` | Send the untouched incoming environment to the next case. |
| `late-capture-on-failure` | Apply an AS alias, mapping rest name, or final class capture to an earlier failure. |
| `or-keep-first-success` | Use only the first reachable OR success. |
| `or-keep-last-success` | Use only the last reachable OR success. |
| `or-drop-failure` | Omit one reachable OR failure from the next-case meet. |
| `pre-guard-failure` | Send the pre-guard environment after a false guard. |
| `unreachable-outcome` | Include an absent success or failure in a meet. |
| `overbroad-cleanup` | Remove an unrelated `Mapping` fact while binding `Sequence`. |

The prior phase-0 witness remains authoritative for generic refutable
unmatched-path retention. Phase 2 owns exit-category preservation. This phase
will reference those results and avoid duplicate cases.

## Lean artifacts

The phase will add focused files under `formal/HoiminOracle`:

- `HoiminOracle/CompoundPatternGuardModel.lean` for attempts and composition;
- `HoiminOracle/CompoundPatternGuardProofs.lean` for kernel-checked results;
- `HoiminOracle/CompoundPatternGuardCases.lean` for fixed rows and sensitivity;
- `CompoundPatternGuardAuditMain.lean` for JSONL rendering, freshness, and
  statistics; and
- `corpus/compound-pattern-guards.jsonl` for generated expectations.

The library root imports the model, proofs, and cheap fixed definitions. It
does not import the executable. The executable reports zero generated depth,
zero event alphabet, zero explored states, and zero transitions.

## Rust correspondence and repair rule

The internal test parses the Lean-owned corpus, validates closed row identity,
and observes production state at the marked case body or following case. A
test-only projection may expose the matched and failed environments if marker
observations cannot distinguish them. That projection must read the real
`visit_match` transfer state.

The public test runs `hoimin_cli::run_with_io` against one-file fixtures with
only `type_list_sequence`. It compares the full overlapping candidate record.
Setup and process failures remain separate from semantic mismatches.

For each difference, the implementation work will:

1. run the single corpus row;
2. record the source, expected and actual observations, and differing fields;
3. classify the difference as confirmed bug, specification ambiguity, model
   defect, or infrastructure error; and
4. change production semantics only after a same-premise Rust regression
   fails for the retained Lean row.

If Lean exposes a confirmed defect, the Rust fix will replace only the
over-broad pattern transfer needed by the failing rows. The fix will preserve
the public analyzer API and existing match-case sequencing.

## Corpus and error contract

Lean owns a deterministic schema-1 JSONL corpus. Each row has one approved
mode, one observation kind, one closed identity, and one unique marker. Rust
rejects unknown fields, modes, observation kinds, duplicate IDs, changed
source premises, crossed family/expectation groups, and inconsistent public
records.

The generator supports `--cases`, `--sensitivity`, `--stats`, `--output`, and
`--check`. The freshness command compares generated bytes with the checked-in
file. No test or adapter may hand-code a second expected semantic result.

## Resource limits and verification

Each Lean or Lake command runs alone through
`formal/HoiminOracle/tools/lean_resource_guard.py` with a 20-second deadline,
a 786,432 KiB root-plus-descendant RSS cap, and 250 ms sampling. Lake builds
use `-Kjobs=1`. A timeout, RSS stop, or monitor failure stays in the report as
an infrastructure result. The audit will reduce the model instead of raising
these limits.

Verification runs in this order:

1. guarded focused proof build and proof consumer;
2. guarded fixed cases, sensitivity, corpus generation, freshness, and stats;
3. focused internal Rust correspondence tests;
4. strict public CLI correspondence tests;
5. formatting and workspace Clippy with warnings denied;
6. workspace tests on the completed tree; and
7. PR CI on macOS, Ubuntu, Windows, MSRV, randomized order, wheel smoke,
   contracts, and dependency-boundary jobs.

The final report records theorem premises, source fixtures, corpus and mode
counts, sensitivity results, mismatch decisions, exact commands, elapsed time,
peak RSS, and exclusions. PR merge requires green CI. The phase-specific
worktree is removed after merge, while Issue #300 remains open for phase 5.
