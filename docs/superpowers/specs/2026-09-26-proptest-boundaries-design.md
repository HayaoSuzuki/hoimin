# Proptest boundary coverage design

Add bounded generative coverage to existing source-encoding, Git/Python row conversion, and streaming workspace operations. Proptest 1.11.0 is already locked; no dependency or production API change is required. Keep existing fixed examples and Lean oracles: these tests explore larger combinations, not a proof of arbitrary inputs.

## Properties

- Latin-1 decoding preserves bytes on re-encoding and maps every scalar boundary to the independently rendered Unicode string; interior UTF-8 bytes and out-of-bounds offsets are rejected. UTF-8 identity mapping includes arbitrary Unicode and an optional BOM.
- Sorted disjoint Git row selections map to Python row intervals computed by materializing newline-normalized Git rows. Empty sources, adjacent selections, gaps, CR/LF/CRLF mixtures and out-of-source intervals are included. Expectations must not use production line-index helpers.
- Stream comparison equals whole-slice equality regardless of independent short-read sizes and Interrupted errors, and accounts for both complete streams even after differences. Streamed hash/copy equals original bytes, length and one-shot BLAKE3. Consumer and terminal read errors must propagate. Generate small arbitrary bytes plus exact 64 KiB buffer-edge lengths.

## Execution and reproduction

Run through normal cargo test discovery. Bound source sizes, read fragment sizes and shrink iterations; retain default random seeds and failure persistence. Allow PROPTEST_CASES and PROPTEST_RNG_SEED overrides. Document commands, save real regression seeds, and check deliberate fault sensitivity without shipping those temporary faults or their artificial regression seeds.

## Design self-reviews

1. Scope: tests target production consumers with independent expected values; no new general testing framework, dependency upgrade or filesystem/process fuzzing is needed.
2. Boundaries: generation must include equal and different streams deliberately, EOF/empty data, 64 KiB crossings, all byte values, multibyte Unicode and newline combinations. Avoid mostly-rejected generators.
3. Limits: property tests do not prove correctness or replace Lean/fixed fixtures. Persistent random failures are reproducible; finite read schedules prevent accidental infinite interruptions. Existing main 5f2ae31 has 2484 passing tests and 22 ignored; recheck affected baseline targets before edits.
