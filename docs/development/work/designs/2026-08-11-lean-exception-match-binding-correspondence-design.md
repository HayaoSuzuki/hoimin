# Lean Exception and Match Binding Correspondence Audit Design

Date: 2026-08-11

## Purpose

Audit Hoimin's scope-dependent typing-import propagation at Python exception
handlers and structural-pattern-matching cases. The audit will use a small Lean
model to generate exact expectations and marker-addressed Rust correspondence
tests to observe the implementation at the same expression sites.

The primary questions are:

- Is an exception type expression evaluated before the handler target is bound?
- Is the exception target visible throughout the handler body and removed from
  every handler exit state?
- Are refutable-pattern and guard failures propagated conservatively to the
  following case?
- Does an irrefutable case eliminate the unmatched path?

## Scope

The exception-handler portion covers a supported typing import used as the
handler target, observations in the handler type and body, target cleanup after
fallthrough, `return`, `raise`, `break`, and `continue`, and the conservative
join between a handled path and a path on which that handler does not run.

The match portion covers capture visibility in a case body, partial bindings on
pattern failure, bindings introduced before guard failure, propagation to the
next case, and unmatched-path removal after an irrefutable case.

Observations will distinguish:

- private, marker-addressed snapshots of `NameResolutionBuilder` or
  `AnnotationCollector` state at exact expression sites; and
- public `hoimin plan` candidate decisions over fixtures built from the same
  premises.

The audit excludes arbitrary Python exception and pattern syntax, parser
correctness, exception-group (`except*`) semantics unless already represented
by the same implementation path, runtime exception-object lifetime details,
mutation ranking and execution, and exploration beyond the fixed cases below.

## Semantic Model

The Lean model will represent only the binding facts required by the audit. An
environment records whether the audited name still denotes the supported
typing import. A path state additionally records its exit kind:

- fallthrough;
- return;
- raise;
- break; or
- continue.

Handler evaluation is modeled in this order:

1. evaluate the handler type in the incoming environment;
2. bind the handler target for the handler body;
3. evaluate the body and classify every resulting exit;
4. delete the handler target from every exit environment; and
5. conservatively meet compatible control-flow paths at their join.

Match evaluation is modeled as ordered cases. A refutable pattern produces a
success environment for its body or guard and a failure environment for the
next case. The failure environment conservatively retains possible writes that
may have occurred before failure. A false guard passes the post-pattern,
post-guard environment to the next case. An irrefutable case has no unmatched
successor. Joins retain a typing-import fact only when every reaching path has
that exact fact.

This is an explicit reduced model. It will not claim a proof of the Rust
implementation or of Python generally; correspondence tests will establish
agreement for the fixed same-premise observations.

## Fixed Cases and Expected Observations

Exception cases will observe:

1. the handler type before target binding;
2. the handler body after target binding;
3. normal continuation after target deletion;
4. each non-fallthrough exit after target deletion;
5. a join with a path where the handler is not selected; and
6. an unrelated binding preserved across handler cleanup.

Match cases will observe:

1. a capture visible in its successful case body;
2. a partially bound name reaching the following case after pattern failure;
3. pattern and guard writes reaching the following case after guard failure;
4. a refutable unmatched path participating in the final join;
5. an irrefutable case removing that unmatched path; and
6. an unrelated binding preserved through case sequencing.

Cases will be fixed and human-readable. Structured enumeration, if retained at
all, stays at depth 2 and may not become a prerequisite for the core result.

## Lean Artifacts

The audit will add a focused model, proofs, executable case generator, and
checked-in JSONL corpus under `formal/HoiminOracle`. Proofs will cover the
ordering and cleanup invariants, meet behavior, failed-case propagation, and
irrefutable-case exhaustion. New declarations may not use `sorry`, `admit`, a
custom `axiom`, or unlimited heartbeats.

The executable will support deterministic case output, corpus freshness
checking, statistics, and literal broken-model sensitivity checks. Sensitivity
families will include at least:

- binding the exception target before evaluating its type;
- failing to delete the target from one or more handler exits;
- using union instead of meet at a handler join;
- discarding partial bindings on pattern failure;
- discarding bindings on guard failure; and
- retaining an unmatched path after an irrefutable case.

## Rust Correspondence Tests

A focused integration test will parse each corpus row, construct the exact
fixture, and compare the Lean expectation with either a private marker-addressed
snapshot or the public plan manifest. The schema will reject unknown fields,
duplicate IDs, missing or duplicate markers, unsupported scenario/observation
pairs, malformed manifests, and nonzero public command exits as infrastructure
errors rather than semantic mismatches.

Private projections will be test-only and as narrow as possible. Production
behavior will change only if a same-premise mismatch demonstrates a defect. If
the implementation is correct but an exact site is unobservable, the preferred
change is a test-only projection rather than broad production instrumentation.

## Test-Driven Workflow

Implementation will proceed in red-green-refactor order:

1. add schema and fixture tests that fail without the new corpus support;
2. add fixed Lean expectations and proofs;
3. generate and freshness-check the corpus;
4. add internal exact-site correspondence observations;
5. add public candidate observations where the behavior is externally visible;
6. run sensitivity variants and focused Rust tests; and
7. run the proportionate workspace verification suite.

If a mismatch is found, first minimize it and determine whether it is a model,
adapter, observation-site, or production defect before changing production
code.

## Resource Safety

Every Lean command will run alone through
`formal/HoiminOracle/tools/lean_resource_guard.py` with:

- a 20-second deadline;
- a 768 MiB root-plus-descendant RSS limit;
- 250 ms sampling; and
- `lake -Kjobs=1` for builds.

No Lean commands will run concurrently. The retained executable path will use
`lake env lean --run` rather than a native linked executable. The prior
full-library and native-link attempts that exceeded the budget will not be
retried. A limit stop is an infrastructure result, not a semantic failure, and
will not cause the memory ceiling or exploration depth to be raised.

## Deliverables and Completion Criteria

The completed audit will contain:

- committed Lean model, proofs, executable cases, and corpus;
- focused Rust correspondence tests with exact-site and public observations;
- literal broken-model sensitivity evidence;
- a report stating the proven reduced-model claims, implementation agreement,
  exclusions, command ledger, and any production fix; and
- passing formatting, lint, focused, and workspace tests appropriate to the
  changed surface.

Completion requires all retained corpus observations to agree with Hoimin or a
confirmed mismatch to be fixed and covered by regression tests. The work will
then receive code review, be submitted as a PR, pass CI, and be merged.
