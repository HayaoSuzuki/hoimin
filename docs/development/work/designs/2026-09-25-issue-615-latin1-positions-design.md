# Issue 615: compact Latin-1 source positions

Reduce retained position-index heap without changing Python encoding support, raw-byte identity, scalar boundaries, physical newlines, or mutation writeback. The existing decoder stores two usize coordinates per expansion; validation additionally stores two u32 values for the same expansions.

## Design

Store only each non-ASCII raw start in the decoder's Vec<usize>. Entry i has decoded end raw_start + i + 2. Count starts below the raw boundary for forward mapping; binary-search derived ends for reverse mapping after checking the decoded UTF-8 boundary. Keep checked arithmetic at public mapping boundaries. Keep usize storage: the standalone public decoder currently has no u32 source-size restriction.

Give CandidateValidationContext a private location-index enum: Unicode(PythonSourceIndex) for UTF-8/ASCII, Latin1(Vec<u32>) for original physical-line starts. Each Latin-1 byte is one scalar, so its raw offset minus its raw line start is its Python column. Preserve both existing raw-length and decoded-length u32 checks before constructing indexes. Keep raw-to-decoded span checks and validation error precedence. The public PythonSourceIndex remains unchanged; its separate transient analyzer instance is outside this bounded change.

For the issue's 524,290-byte fixture, the unchanged observed capacities predict context retained bytes falling from 13,631,572 to 5,242,964 (61.5%). This is a hypothesis requiring allocation measurements, not a claimed result. ASCII retains zero Unicode corrections. Construction remains linear and queries logarithmic; no prefix rescans.

Alternatives: u32-only decoder entries would lower the standalone decoder limit; bitset/rank indexes add density thresholds and word-boundary machinery; sharing the analyzer index requires broader lifetime/API changes. Defer these. Do not change source bytes, supported codecs, schemas, limits, Git changed-line mapping (issue 612), or report allocation work.

## Design reviews, before implementation

1. Algebra/boundary review: entry zero ends at raw_start+2, not +1; reverse search must count ends <= the boundary, forward search starts < it. Adjacent expansions and scalar-interior bytes need exhaustive independent checks. Added these to the plan.
2. Compatibility review: raw-line columns are correct only for Latin-1, not arbitrary UTF-8. A context-only enum isolates the optimization. Bypassing PythonSourceIndex would accidentally remove the decoded-size rejection; explicitly preserve that check. UTF-8 BOM handling remains in the existing index.
3. Resource/scope review: removing only a decoder coordinate leaves a duplicate correction table. Include both changes and measure decoded/context separately. Public plan still has a transient analyzer index, so RSS need not match retained-heap savings. Benchmarks must keep inputs/candidate count fixed and report lookup latency separately.
