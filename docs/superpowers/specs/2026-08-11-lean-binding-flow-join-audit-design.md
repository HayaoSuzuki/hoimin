# Lean Binding-Flow Join Audit Design

## Goal

Audit Hoimin's scope-dependent AST binding propagation across control-flow
joins. The audit covers both annotation-site `KnownImports` snapshots and the
builtin/exception `NameResolutionIndex`, using one safety contract: a mutation
candidate may be emitted only when every source and replacement spelling is
known to resolve to the intended semantic target on every reachable path.

This audit is deliberately broader than the existing scope-resolution Lean
model. It includes branch joins, abrupt exits, `finally`, loop back-edges and
fixed points, match failure paths, lexical scope boundaries, and agreement
between the two production resolution systems.

## Existing implementation boundary

Hoimin currently has two related binding analyses in
`crates/hoimin-cli/src/analyzer/rust.rs`:

- `KnownImports` attaches a typing-import environment to each annotation site.
  Compound statements propagate separate fallthrough, break, continue, and
  terminate environments, intersect agreeing facts at joins, and compute loop
  heads to a fixed point.
- `NameResolutionIndex` records tracked builtin and exception occurrences in a
  scope graph and classifies each occurrence as `DefinitelyBuiltin`,
  `Shadowed`, or `Unknown`.

The existing `ScopeResolutionModel.lean` covers local scope lookup and
source/destination gating but does not model structured control flow, exit
categories, loop fixed points, or consistency with `KnownImports` joins.

Parsing recovery, Ruff AST correctness, arbitrary Python runtime mutation of
`builtins`, aliasing through dynamic objects, candidate ranking, execution of
mutants, and presentation formats are excluded.

## Considered approaches

### Unified knowledge lattice and structured control flow (selected)

Project both production analyses into a small common resolution lattice, then
model structured statements, lexical scopes, exit categories, joins, and loop
fixed points. Prove algebraic and safety properties, enumerate small structured
programs, retain minimized broken-model witnesses, and generate implementation
expectations for real Python fixtures.

This is the only option that can expose disagreement between the two resolver
families while keeping the semantic contract explicit.

### Separate Lean audits for each resolver

Independent models would be smaller and cheaper, but they could both be
internally consistent while applying different join or scope rules to the same
source shape. That misses the requested cross-resolver audit.

### Lean lattice laws plus Rust-only property tests

This would minimize Lean cost, but it would leave `finally`, exit routing, and
loop fixed points outside the formal transition model. It is useful as a
fallback if resource limits prevent structured enumeration, not as the initial
design.

## Correspondence worksheet

| Premise or observation | Lean representation | Production configuration | Observation | Evidence | Mode |
| --- | --- | --- | --- | --- | --- |
| All paths retain the same typing import | equal facts joined by `meet` | generated Python branches passed through public `hoimin plan` | exact manifest replacement | CLI corpus adapter | `strict` |
| One path rebinds or loses an import | disagreeing path facts | generated `if`, loop, try, or match project | import-dependent candidate is absent from the manifest | CLI corpus adapter | `strict` |
| Builtin/exception source and destination both resolve safely | two projected resolution facts | bare calls or exception names in a generated project | exact manifest candidate presence or absence | CLI corpus adapter | `strict` |
| Whole-block function local and class non-closure rules | function/class scope frames | generated nested function and class project | manifest candidates and scope symbols | CLI corpus adapter | `strict` |
| Comprehension leftmost iterable boundary | outer then comprehension frame | generated comprehension project | manifest sites inside and outside the implicit scope | CLI corpus adapter | `strict` |
| Internal fallthrough/break/continue/terminate environments | categorized `Exits` | same source is constructible, but snapshots are private | test-only environment snapshots | owned unit seam | `internal-fixture` |
| Loop-head fixed point | repeated monotone transfer until stable | same loop source is constructible, but head state is private | test-only fixed-point state | owned unit seam | `internal-fixture` |
| Reduced names, scope depth, and structured-program bound | finite Lean domain | no production option selects the abstract bound | Lean states and counterexamples only | executable audit | `model-only` |

Infrastructure setup, parse failure, timeout, RSS guard termination, panic, or
incomplete observation is `infrastructure-error` and yields no semantic
conclusion.

## Lean model

### Resolution state

Use two representative tracked names so source and destination safety cannot
collapse into one boolean. Each name has a small fact distinguishing an
expected builtin, an expected supported typing import, a definite competing
binding, absence, and uncertainty. Projection functions expose the views used
by `KnownImports` and `NameResolutionIndex`.

The common meet retains a fact only when every reachable path agrees on the
same intended target. Disagreement becomes uncertainty. Candidate gates require
both source and destination projections to resolve to the requested target.

### Scope and statements

Scopes are module, function, class, and comprehension. The model makes these
Python rules explicit:

- function locals are determined for the complete block;
- a nested ordinary function skips an intervening class namespace;
- class-body expressions use ordered class bindings;
- a comprehension's leftmost iterable uses its enclosing scope, while targets,
  filters, later generators, and results use the implicit comprehension scope;
- `global` and `nonlocal` redirect writes or make resolution uncertain when a
  unique target cannot be established.

Structured statements include supported import, binding, wildcard uncertainty,
sequence, conditional, loop, break, continue, return/raise, try/handler/else/
finally, and match cases with irrefutability and guards. This is a semantic AST,
not a model of Ruff parser nodes.

### Exits and fixed points

Evaluation returns categorized `Exits` containing optional fallthrough plus
lists of break, continue, and terminate states. A join intersects only states
that can reach the same continuation. `finally` executes once for every incoming
exit category; a falling-through final body preserves the incoming category,
while an abrupt final body replaces it.

Loops include the zero-iteration path. Their head is the descending fixed point
of initial entry plus fallthrough and continue back-edges. Break states bypass
loop `else`; natural exhaustion reaches it. The finite model uses a fuel bound
only in the executable; the theorem-facing definition proves stabilization on
the finite fact lattice rather than assuming a successful iteration count.

## Theorems

The imported proof module will establish, with explicit premises:

- meet is commutative, associative, and idempotent;
- a fact retained by a multi-path meet is present identically on every path;
- joining additional paths cannot invent stronger knowledge;
- allowed source/destination replacements resolve to their intended targets;
- unrelated sibling scopes cannot affect resolution;
- function whole-block locals and class skipping preserve the stated lookup
  boundary;
- a falling-through `finally` preserves each incoming exit category;
- loop-head iteration is descending and stabilizes on the finite lattice;
- every modeled statement preserves the conservative candidate-safety
  invariant.

Theorems prove the Lean model only. Implementation correspondence is established
separately by generated fixtures and internal tests.

## Bounded exploration and sensitivity

The executable enumerates canonical structured programs shortest/smallest first
over two tracked names. It starts at syntax depth 2, measures states, programs,
transitions, elapsed time, and peak observed RSS, then may advance one level at
a time through a predeclared maximum depth 4. It never raises the maximum depth
or state ceiling after a resource symptom.

The hard state ceiling is 1024 after semantic deduplication. Symmetry reduction
may rename the two tracked names only after retaining fixed asymmetric
source/destination witnesses.

Fixed sensitivity witnesses must distinguish these broken families:

1. union-like join that keeps a fact present on only one path;
2. function/class lookup that incorrectly treats a class namespace as a
   closure;
3. `finally` routing that merges or loses abrupt exit categories;
4. loop analysis that omits a continue or fallthrough back-edge;
5. candidate gating that checks only the source name.

If an additional production mismatch is found, its minimized witness is added
without replacing these fixed checker-sensitivity cases.

## Resource safety

Every potentially expensive Lean command runs alone with `-Kjobs=1`, a
20-second external wall-clock deadline, and local `maxHeartbeats 100000` on
expensive theorems. Unlimited heartbeats are forbidden.

macOS on this machine rejects attempts to set a usable `RLIMIT_AS` or data
segment limit, so the design does not claim a hard 2 GiB process limit. Instead,
a parent/descendant RSS watcher samples every 250 ms and terminates the complete
Lean/Lake process tree when combined RSS exceeds 768 MiB. Exploration is not
increased after any sample exceeds 512 MiB, after a state count grows by more
than four times the prior level, after a timeout, or after severe UI slowdown,
swap growth, or unexplained superlinear time.

An RSS guard is not equivalent to a kernel allocation limit; that limitation is
reported. The model will be split or replaced by an inductive lemma rather than
relaxing these thresholds.

## Generated corpus and Rust correspondence

Lean owns versioned cases and expected candidate decisions. A dedicated
executable writes deterministic JSONL, checks freshness, prints bounded-search
statistics, and reports sensitivity witnesses. Expensive enumeration stays out
of imported modules.

The Rust adapter parses the corpus with unknown-field rejection, unique case
IDs, exact modes, and exact scenario-to-premise validation. Strict cases build
an isolated real Python project, invoke the public `hoimin plan` path with an
exact operator selection, and inspect its generated manifest. They compare
candidate presence, operator, replacement, span role, and scope symbol. Each
case runs independently; parse failure, panic, timeout, or setup failure is
infrastructure failure.

Internal-fixture tests may invoke the owned analyzer module directly or expose
only the minimal `KnownImports`/exit projection needed to check exact
control-flow state. They do not replace the public CLI adapter. Production files
change only if a strict or internal same-premise mismatch is first reproduced
by a failing test.

## Verification and integration

Verification proceeds in this order:

1. theorem consumer and focused Lean proof build;
2. depth 2 statistics, followed by depth 3 and at most depth 4 only when every
   safety gate permits it;
3. all five broken-family witnesses;
4. fixed cases and corpus freshness;
5. Rust parser, strict adapter, and internal-fixture tests;
6. focused existing analyzer tests;
7. complete Lean build, Rust workspace tests, Clippy with warnings denied,
   formatting, and diff checks;
8. all applicable pull-request CI checks, then squash merge.

The final report records retained and abandoned bounds, observed peak RSS,
exact commands, correspondence mode per case, minimized mismatches, repairs,
limitations, and the next independent audit target.
