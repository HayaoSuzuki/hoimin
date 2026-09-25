# Issue 609: borrowed progress comparison keys

## Intent and scope

Comparison of adjacent usable reports must retain its current result while avoiding candidate text copies. At a fixed candidate count, increasing original/replacement text must not increase comparison-only peak live heap proportionally. Report parsing, history streaming, schemas and state policy are unchanged.

## Design

Use `MutantKey<'a>` with `&'a Utf8Path`, `&'a str` for original, replacement and operator, and `Option<&'a str>` for symbol. Use `ComparisonKey<'a>` with borrowed candidate IDs or borrowed content keys. Derive Copy, Clone, Eq, Hash and PartialEq. ReportMutants ties both keys and mutant references to the report lifetime. Duplicate and inconclusive sets copy only references. Preserve all five tuple fields and derived equality: content is compared by value, never pointer identity or a hash digest alone.

The eligibility decision continues to select stable IDs for matching ID sets and full content fallback otherwise, including duplicate IDs. A changed set remains indeterminate while common, added, removed, ambiguous, inconclusive and score information remains available. Keys cannot escape comparison; returned comparisons own only scalar output.

Alternatives considered: storing digests alone would introduce collision errors; interning or Arc would change report ownership and add scope without improving this local fix. Borrowed fields preserve the existing HashMap equality and hashing contract directly.

## Validation and resource limits

A dedicated integration-test binary with one test uses the existing allocator tracker, so concurrent tests cannot corrupt measurement. Prepare reports before measuring only compare_reports. Hold candidate count fixed and vary all borrowed text lengths, including ID controls. Exercise unique fallback, duplicate union and inconclusive union. Compare 32-byte and 256-KiB payloads with a 64-KiB allowance, far below one cloned large payload. Existing history and JSONL heap tests remain enabled.

Semantic tests cover each tuple component, optional symbol None versus empty, ignored location/ID fields, duplicate and inconclusive union counting, stable IDs with duplicate content, scores, warnings and stall adjacency. Public run output in JSON and JSONL is saved unchanged and consumed by progress after inserting a source comment to alter IDs. Existing Lean-generated progress fixtures remain the decision oracle; this ownership-only change does not add a state rule or require changing formal models.

Use one build job, debug info disabled, and the dedicated batch-progress target. Do not run Lean without root coordination; global bound is one process, 20 seconds and 2 GiB.

## Design self-reviews

1. Identity audit: a digest-only approach would lose collision equality. Selected borrowed full tuple with derived Eq/Hash; no content normalization or path-string conversion.
2. Lifetime/allocation audit: fixing only original/replacement would leave ID, path, operator and symbol copies and set clones. Borrow every field and make whole keys Copy; measurement includes unique/duplicate/inconclusive paths.
3. Contract audit: changed IDs are not evidence of progress. Retain existing eligibility/state path and test useful fallback counts plus indeterminate result. Reader and warning generation are intentionally untouched.
