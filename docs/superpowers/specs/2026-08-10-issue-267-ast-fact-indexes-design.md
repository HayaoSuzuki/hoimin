# Issue 267 AST Fact Indexes Design

## Goal

Remove repeated full-vector scans from the Rust Python analyzer without
changing which candidates are emitted, their order, exact spans, symbols,
profiles, or limits. Fact construction remains one AST walk; hot lookups become
logarithmic or amortized constant time.

The current `AstFacts` representation performs a linear search for every arid
range query, annotation containment query, `not` operand query, and lexical
scope lookup. Large generated modules can therefore multiply thousands of
facts by thousands of token and expression queries.

## Scope

This change indexes four existing fact families:

- annotation and focused-profile arid containment;
- unary `not` operands keyed by operator byte offset;
- the innermost definition symbol at a byte offset;
- the annotation guard in the token loop, after a cheap spelling check.

Candidate collection, mutation operator semantics, parser traversal, profile
membership, ordering, duplicate handling, and report formats are unchanged.
This does not replace the line/column index from Issue #129 or broaden the
analyzer's syntax coverage.

## Considered Approaches

### Specialized immutable indexes (selected)

Build small indexes after AST visitation: a prefix-maximum containment index,
a hash lookup for `not`, and disjoint scope segments produced by an event
sweep. Each representation encodes the exact query it serves and can be
checked against the old linear semantics with independent tests.

### General interval tree

A generic interval tree can answer all range queries but carries balancing,
ownership, and overlapping-interval complexity that these immutable fact sets
do not require. It would also obscure the different containment and
innermost-scope semantics.

### Traversal cursor

A mutable cursor can make ordered queries nearly linear overall, but it couples
correctness to call order across the token loop and AST visitor. Future query
reordering could silently return stale results, so this issue keeps lookups
order-independent.

## Containment Index

Annotation and arid facts use sorted starts plus a prefix maximum of ends. To
answer whether any fact fully contains query `[start, end)`, binary-search the
last recorded start at or before `start`; the corresponding prefix maximum
contains the query exactly when it is at least `end`.

This remains exact for overlapping or nested annotations. Arid ranges retain
their existing sort-and-merge normalization before index construction, so
touching ranges keep the same containment behavior. Empty indexes return
false. Construction is `O(n log n)`, storage is `O(n)`, and each lookup is
`O(log n)`.

## `not` Operand Index

The visitor records each unary `not` as
`operator_start -> (operand_start, operand_end)` in a `HashMap`. Python AST
nodes cannot produce two unary operators at the same byte start; construction
asserts or tests this invariant rather than defining a new overwrite policy.
Lookup is amortized `O(1)`.

## Scope Segment Index

Definition ranges can nest, and after an inner definition ends the surrounding
definition must become visible again. A sweep converts the ranges into
disjoint segments that name the active scope with the greatest start offset,
matching the existing `max_by_key(scope.start)` query.

For every scope, construction emits a start and an end event. At each distinct
offset:

1. emit `[previous_offset, offset)` for the previously active innermost scope;
2. remove ending scopes;
3. insert starting scopes;
4. continue with the active scope having the greatest `(start, ordinal)` key.

The ordinal preserves the previous iterator's last-on-equal-start behavior.
Adjacent segments naming the same scope may be coalesced. Query uses binary
search for the last segment start at or before the offset, then verifies its
exclusive end. Construction is `O(n log n)`, storage is `O(n)`, and lookup is
`O(log n)`.

## Construction Lifecycle

`AstFacts::from_module` first gathers the same raw facts as today. A finalization
step normalizes arid ranges and builds every immutable index before candidate
collection begins. Raw vectors needed only during visitation are not consulted
by hot queries. This makes a missing finalization step structurally visible in
tests rather than allowing a slow fallback.

The token loop tests whether the source spelling is one of `&`, `|`, `<<`, or
`>>` before querying annotation containment. Other AST-proven operator tokens
therefore avoid an irrelevant logarithmic lookup.

## Complexity Instrumentation and Benchmark

Test-only counters record query counts and binary-search comparisons for range
and scope indexes, plus keyed `not` lookups. They never participate in candidate
selection and are absent from production output.

An ignored release-mode benchmark generates thousands of scopes, annotations,
unary `not` expressions, and focused-profile arid regions. It asserts:

- exact candidate counts and stable representative spans/order;
- lookup counts expected from the fixture;
- comparison ceilings proportional to
  `queries * (ceil(log2(facts + 1)) + constant)`;
- no legacy full-vector scan counter activity.

Elapsed time is printed for local profiling but is not a pass/fail threshold.
Smaller deterministic property tests compare every indexed answer with an
independent literal implementation of the legacy linear semantics, including
overlap, nesting, gaps, equal starts, and boundary offsets.

## Lean Design Analysis

`FactIndexModel.lean` defines source ranges, containment, prefix summaries,
scope ranges, and scope segments independently of Rust storage. The proof
module establishes that a valid prefix summary answers containment exactly and
that a valid scope segment returns the same greatest-start containing scope as
the source ranges.

Fixed sensitivity witnesses exercise two tempting broken indexes:

- checking only the last eligible range misses `[0, 10)` for query `[7, 8)`
  when a later nested range is `[5, 6)`;
- returning the last-started scope without its segment end returns a finished
  inner scope where the outer scope should resume.

Atomicity and idempotency sensitivity are not applicable: the indexes are
immutable pure-query structures with no persistent effect or state transition.

### Correspondence boundary

| Premise or observation | Lean representation | Production configuration | Observation | Evidence | Mode |
| --- | --- | --- | --- | --- | --- |
| Prefix containment equivalence | ranges and valid prefix summaries | abstract lists are not a public input | Lean result only | theorem and fixed witness | `model-only` |
| Innermost-scope segment equivalence | ranges, segments, and validity predicate | abstract lists are not a public input | Lean result only | theorem and fixed witness | `model-only` |
| Indexed answers equal legacy scans | generated private facts | owned analyzer test fixtures | returned ranges and symbols | Rust property/regression tests | `internal-fixture` |
| Lookup growth is logarithmic/constant | test-only comparison counters | adversarial generated Python module | counters and candidates | ignored release benchmark | `internal-fixture` |

Lean fixes the semantic contracts but does not prove the Rust sorting, sweep,
or binary-search implementation. Rust correspondence tests exercise those
owned implementation seams. No public corpus adapter is needed.

## Testing and Verification

The change adds unit tests for each index, property-style comparisons against
legacy linear queries, candidate snapshot regressions, operation counters, and
the release benchmark. Verification includes the focused analyzer suite, Lean
build and forbidden-placeholder scan, formatting, Clippy, the full workspace,
E2E tests, and skill contract tests.

