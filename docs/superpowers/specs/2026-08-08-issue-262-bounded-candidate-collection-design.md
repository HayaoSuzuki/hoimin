# Issue 262 Bounded Candidate Collection Design

## Goal

Make `--max-candidates` bound the number of mutation candidates retained while
analyzing one Python file. Preserve the exact candidate prefix, diagnostics,
selection behavior, and deterministic ordering produced by the current
analyzer.

## Current failure

The Rust analyzer currently constructs every selected token candidate, extends
that vector with every selected AST candidate and type-annotation candidate,
then applies the focused-profile filter, deduplication, sorting, and final
truncation. A small `--max-candidates` therefore limits output and spool size but
does not limit the analyzer's peak candidate memory.

The parsed module, token stream, AST facts, and source text are required for
analysis and remain proportional to source size. This issue specifically
bounds retained `AnalyzerCandidate` values and their deduplication keys.

## Considered approaches

### One global bounded sink

All producers could push directly into one top-prefix accumulator. This gives
the smallest constant factor, but couples token traversal, the AST visitor, and
type-annotation traversal to one shared mutable interface. It also makes
producer ordering and cancellation behavior harder to review.

### Per-producer bounded prefixes (chosen)

Token, AST, and type-annotation producers each retain only their first
`max_candidates + 1` eligible unique candidates under the final ordering. The
three bounded prefixes are then concatenated in their existing producer order,
globally deduplicated, stably sorted, and truncated.

This follows the existing architecture and bounds retained candidates by at
most approximately `3 * (max_candidates + 1)` before the final merge.

### Chunked or external sorting

Chunking candidates or spilling them to disk could support arbitrary producer
sizes but adds I/O, cleanup, and serialization complexity. The fixed three
in-memory producers make that unnecessary.

## Chosen design

### Bounded prefix accumulator

Add a reusable internal accumulator that owns:

- a limit of `max_candidates.saturating_add(1)`;
- a max-heap or equivalent bounded ordered set containing the currently
  retained prefix;
- a bounded set of deduplication identities
  `(span.start, replacement, operator)` for retained candidates;
- a monotonically increasing producer-local emission sequence used only to
  preserve stable ordering when `span.start` and `operator` compare equal;
- test-only high-water statistics.

Each eligible candidate is compared by the analyzer's current final key:
`span.start`, then `operator`, then producer-local emission sequence. If the
accumulator is full, only a candidate earlier than the retained maximum can
replace it. Eviction also removes its deduplication identity, so both candidate
and key retention stay bounded. Exact duplicates of a retained candidate are
discarded.

At finish, the accumulator returns its retained candidates in final stable
order and whether it observed more eligible unique candidates than it could
retain.

### Filtering order

Selection by path, line, symbol, and operator remains in `make_candidate`.
Focused-profile filtering moves to the producer boundary and runs before a
candidate enters the bounded accumulator. Type-annotation candidates remain
eligible under the focused profile exactly as today.

This preserves the contract that filtered arid candidates do not consume the
effective candidate limit.

### Producer integration and merge

The token loop, `AstCandidateCollector`, and type-annotation traversal each use
one accumulator with the same prefix capacity. AST traversal and token
traversal keep their existing cooperative cancellation checks.

The returned prefixes are concatenated in the existing order: token, AST,
then type annotation. The existing global deduplication identity and stable
`span.start`/`operator` sort are retained as a defensive cross-producer merge.
The final result is truncated to `max_candidates`.

The output is marked truncated when either:

- a producer observed eligible unique candidates beyond its prefix; or
- the merged global unique prefix contains more than `max_candidates` entries.

A producer that overflows has itself observed more than `max_candidates`
unique candidates, so cross-producer duplication cannot turn that overflow
into a non-truncated result.

### Prefix correctness

For a requested output size `k`, retaining the first `k + 1` unique candidates
from every producer is sufficient. If a discarded candidate from one producer
could enter the global first `k + 1`, that producer already had `k + 1`
distinct earlier candidates. Those candidates are present in the merge, so the
discarded candidate cannot be in the global prefix. Cross-producer duplicates
do not invalidate this: the `k + 1` candidates within that producer are unique
under the same global deduplication identity.

Stable producer concatenation and producer-local emission sequence preserve
ties under the current stable final sort.

### Extreme limits

`saturating_add(1)` avoids overflow for `usize::MAX`. A zero remaining limit is
valid for internal multi-file discovery: producers retain one candidate only
to establish truncation, while the final output remains empty. Default and
large limits use the same path and preserve current output.

## Testing

Add deterministic analyzer tests that:

- generate a high-candidate source spanning token, AST, and type-annotation
  producers;
- compare a small-limit result byte-semantically with the same source analyzed
  under a limit large enough to retain all candidates;
- assert every producer high-water mark is at most `max_candidates + 1` and
  the combined retained peak is bounded by the three-producer constant;
- assert the overflow diagnostic is unchanged;
- prove focused filtering occurs before retention with many arid candidates
  preceding eligible candidates;
- cover zero remaining capacity and candidates tied on the existing sort key;
- retain existing cancellation, ordering, reparsing, selection, and default
  profile tests.

Test-only retention statistics must be returned locally from an analysis call;
they must not use global state or affect production serialization.

## Documentation

Clarify in the README and development guide that `--max-candidates` bounds
retained candidate records during each producer and that parsed source/AST
memory remains proportional to source size. Do not claim that the option is a
general memory limit; `--max-memory` continues to govern descendants rather
than Hoimin itself.

## Non-goals

- Changing candidate IDs, ordering, deduplication identity, profiles, operator
  selection, or candidate-limit diagnostics.
- Bounding parser, AST, token, source-text, or temporary replacement-string
  memory.
- Changing the candidate spool, multi-file sequencing, or resource backends.
- Adding a new CLI option or schema field.
