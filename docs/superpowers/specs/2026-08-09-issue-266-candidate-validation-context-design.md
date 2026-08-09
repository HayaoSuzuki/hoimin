# Issue 266 Candidate Validation Context Design

## Goal

Make analyzer candidate conversion scale with source size plus candidate count,
rather than source size multiplied by candidate count, while preserving strict
candidate validation and byte-for-byte stable IDs at external boundaries.

Today the CLI bridge hashes the complete source while constructing every
descriptor, and core validation hashes it again and scans the complete prefix
to validate line and column. A 307,200-byte file with 7,680 candidates therefore
feeds roughly 4.7 GB through the two hashes before the prefix scans are counted.

## Scope

The change covers:

- reusable validation facts for one immutable source byte slice;
- once-per-file hashing in both analyzer conversion paths;
- indexed line lookup and current-line Unicode column counting;
- preservation of the existing strict single-candidate validation API;
- an ignored end-to-end conversion and spool-manifest benchmark.

The following remain unchanged:

- candidate schema, JSON shape, stable-ID domain and ID inputs;
- validation result and error precedence for every descriptor/source pair;
- candidate order, sequence numbers, profiles, operators and limits;
- strict source revalidation while loading external plans;
- analyzer parsing and candidate discovery.

## Considered Approaches

### Borrowed core validation context (selected)

Add `CandidateValidationContext<'source>` in `hoimin-core`. It borrows the
immutable source and owns its hexadecimal BLAKE3 hash and byte offsets for the
start of every line. Analyzer code constructs one context after analyzing each
file and reuses it for every candidate from that file.

This centralizes the optimized and strict algorithms in core, gives internal
batch conversion no authority to bypass validation, and leaves the existing
external API intact.

### Trusted analyzer conversion

Let the analyzer construct `MutationCandidate` directly and compute stable IDs
without validation. This is faster but creates a second policy implementation;
an analyzer regression could then emit stale spans, originals, or locations
that only fail much later.

### Cache inside `validate_candidate`

Use a global or thread-local cache keyed by source identity. This obscures
ownership and invalidation, can retain large source-derived data, and makes
performance depend on pointer reuse. An explicit borrowed context makes the
lifetime and reuse boundary deterministic.

## Core API and Invariants

`CandidateValidationContext::new(source)` is infallible. This is necessary to
preserve the current validation error order: an invalid schema, path, hash, or
mutation must still be reported before invalid source UTF-8. The context stores
the source bytes and attempts UTF-8 decoding lazily from the caller's point of
view; construction never returns a validation error.

The context exposes its exact lowercase hexadecimal source hash for descriptor
construction. `validate_candidate_with_context(&context, descriptor)` performs
the same checks, in the same order, as today's `validate_candidate`:

1. schema version;
2. normalized relative path;
3. source hash;
4. non-empty operator and changed replacement;
5. checked byte-span bounds;
6. original source bytes;
7. UTF-8 validity and character boundaries;
8. one-based line and Unicode-scalar column;
9. stable ID construction.

`validate_candidate(source, descriptor)` remains the strict public convenience
function. It constructs a fresh context and delegates to the context-aware
function, so external plan ingestion always recomputes facts from the supplied
source and receives identical semantics.

## Indexed Location Lookup

The context records `[0]` plus the byte immediately following every newline.
For a candidate start offset, a partition-point lookup finds the greatest line
start not exceeding the offset. Its index yields the one-based line number.
Column counting decodes and counts characters only between that line start and
the candidate start.

This preserves the existing definition of columns as Unicode scalar counts,
including CRLF behavior, while replacing a full-prefix newline and character
scan with `O(log lines + current line length)` work.

## Analyzer Integration

Both batch paths construct exactly one context per source:

- `discover_targets_blocking`, once for each target file;
- `analyze_and_store`, once for the request source.

`mutation_candidate` accepts the context rather than raw source. It copies the
context hash into the descriptor and validates through the context-aware core
API. No descriptor or stable-ID field is recomputed by a separate fast path.

The plan loader continues to call `validate_candidate(source, descriptor)`.
It does not accept cached analyzer facts because persisted metadata is an
external trust boundary.

## Errors and Compatibility

No validation error variant is added or removed. Tests pin error precedence
with descriptors containing multiple simultaneous faults, including invalid
UTF-8 sources. Existing ID fixtures are reused to prove exact stability.

The context is tied to its source through a Rust borrow, so it cannot outlive
or silently switch the byte slice used for validation. A descriptor carrying a
hash, original, span, or location from another source is rejected normally.

## Testing and Measurement

Core tests first compare the legacy wrapper and context-aware function over
valid, stale, Unicode, CRLF, invalid-UTF-8, and multiply-invalid cases. CLI
tests pin batch conversion against stale metadata and compare emitted candidates
with the pre-change fixtures.

An ignored release-mode benchmark builds the 307,200-byte/7,680-candidate
fixture, includes analyzer output conversion, validation, JSON spool writes,
and spool finalization, and prints source bytes, candidate count, spool records,
and elapsed milliseconds. This extends the existing analyzer-only benchmark to
cover the bridge named in the issue.

Lean is not used for this change. The relevant properties are a pure lookup
equivalence and an explicit lifetime-bound cache, not a state machine, retry,
or asynchronous interleaving. Independent equivalence tests and benchmarks are
the more direct oracle.
