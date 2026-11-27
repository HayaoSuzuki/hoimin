# Proptest boundary coverage: review and evidence

Base: main 5f2ae319c99eafac1f6ccd8b86ed9ed08001ebaf. Design and plan with three reviews each were committed as 2ff26c9 before test changes. Both crates already declare proptest and Cargo.lock resolves 1.11.0. No dependency or runtime contract changes are included.

## Implementation self-reviews

1. Expected values: Latin-1 expected text/boundaries come from independently rendered scalars; UTF-8 uses standard string boundaries. Git row expectations materialize newline-normalized text rather than calling production line-index helpers. Streaming observations use complete input slices and one-shot BLAKE3, while invoking real stream/hash operations.
2. Generation and termination: constructive generators have no rejection filters; coordinate sources and selection masks are bounded. Stream data includes small arbitrary bytes and generated repeating patterns at 65535/65536/65537/131071/131072/131073 bytes. Readers have independent positive fragment sizes, at most one Interrupted before progress, and optional terminal errors. Equal, modified, truncated and independent right streams are deliberately generated. Shrinking stops after at most 2048 iterations; ordinary seeds and failure persistence remain at proptest defaults.
3. Scope and error contracts: only cfg(test) module registrations enter existing source files. New readers and oracles are test-only. Complete byte counts ensure that mismatches do not short-circuit accounting. Consumer errors stop further reads; terminal read errors retain operation, path and message. No new Lean proof, OS race coverage or native Windows observation is claimed.

## Test self-reviews

1. Coverage: existing fixed examples and Lean adapters remain. The new eight properties exercise generated combinations and every scalar boundary, including EOF/out-of-bounds/split UTF-8 scalars, normalized disjoint Git selections, short reads, buffer crossings and error propagation. Cookie recognition and malformed UTF-8 retain their existing dedicated tests; generated Unicode is not described as arbitrary invalid byte coverage.
2. Sensitivity: six temporary production defects were tested individually with PROPTEST_CASES=64 and PROPTEST_RNG_SEED=20260926. Each failed its named new property with proptest shrinking output. Defects were restored in finally blocks, and artificial failure persistence was disabled for these runs only. Initial Rust test-code compiler errors (temporary Cow borrows and braces in assertion message expansion) were corrected separately and are not counted as sensitivity evidence.
3. Reproduction and integration: default-case runs and a larger explicit-seed run verify ordinary execution and replay configuration. Run the full workspace and exact static gates after restoring production sources. Inspect the final diff for temporary defects and artificial seed files, and keep only cfg(test) module declarations as changes to existing Rust modules. Final executed results are recorded below.

## Sensitivity observations

| Temporary production defect | New property that failed |
| --- | --- |
| Latin-1 raw expansion lookup uses <= instead of < | latin1_coordinates_preserve_arbitrary_bytes |
| Bare CR is ignored as a Python newline | disjoint_git_rows_match_normalized_text |
| Comparison overwrites earlier mismatches at each chunk | stream_equality_and_counts_ignore_read_partitioning |
| Streamed hasher drops the last byte of every chunk | streamed_copy_and_hash_preserve_all_bytes |
| Consumer errors are discarded | consumer_error_stops_the_stream |
| Terminal read errors are treated as EOF | streams_propagate_terminal_read_errors |

These are seeded, bounded sensitivity experiments, not an exhaustive mutation score. Local logs and sensitivity.json are under /tmp/hoimin-proptest-20260926; raw logs are not committed. Repeatable normal test commands and persistence guidance are in docs/development/guides/contributing.md. A shrunk input is not asserted to be globally minimal when the shrink iteration limit is reached.

## Independent review

The read-only reviewer found no critical/important issue. Its minor finding was that chunks only asserted is_err for terminal failure; the test now requires WorkspaceError::Io with the expected operation, path and injected error text. Equal already asserted its error kind. Reader schedules, invalid encoding/cookie recognition, nonnormalized Git intervals and filesystem races were explicitly outside this bounded addition and remain so in the development guide.

## Knowledge references

Consulted docs/development/knowledge/index.md, audits/rust-2026-07.md, and docs/development/guides/okf-workflow.md. The historical July audit is not evidence for this change. Update the design/report source indexes for these new artifacts; the canonical repeatable procedure lives in docs/development/guides/contributing.md. Existing runtime/design contract concepts are unchanged, because no runtime behavior or Lean claim changes.

## Initial execution results (b1641ee, before CI follow-up)

- Baseline affected core encoding and CLI line/stream suites passed before test changes. The previous full main result was historical context, not substituted for this run.
- New properties passed at the default configuration, then all eight passed at 1024 cases each with seed 20260926 (8192 successful generated cases). The core pair took 0.16s and the six CLI properties 1.23s in this local debug run; these observations are not CI timing limits.
- Full cargo test --workspace passed: 2492 top-level tests, zero failures, 22 ignored across 123 result groups. Two additional successful subprocess test summaries are excluded from this count.
- Exact CI workspace Clippy and vendored-parser Clippy passed. Both formatting checks and git diff --check passed. All 40 workflow-definition Python tests passed.
- Final source diff confirms existing production functions are unchanged; only two cfg(test) module declarations were added. No dependency changes or artificial regression files remain.

## Randomized CI follow-up

Job 108135063356 on PR 663 failed the existing managed-child ownership test: after dropping the final local child, reclaimed_roots was 0 instead of 1. The new proptest cases passed; all other CI jobs passed. The log did not include ReclaimReport details. The original shuffle seed 1790350533343656379 passed locally on macOS before the fix, so the historical Linux scheduling interleaving is not asserted to have been reproduced.

A controlled temporary fork probe retained an inherited lease descriptor while the local owner/child were dropped. It reproduced the same 0-versus-1 failure with preserved_roots=1 and empty details. Releasing and reaping that process before reclamation passed. Existing Linux process setup uses pre_exec, which gives unrelated parallel tests a plausible fork-to-exec descriptor inheritance window. These probes establish a concrete failure mechanism, not definitive attribution of the original CI occurrence. Both probes were restored; no fork/unsafe test instrumentation ships.

The ownership assertions now run in a bounded subprocess, with the fixture created only after that process starts. The parent harness requires successful exit and a completion marker emitted after all child assertions; a deliberately nonexistent exact test name was rejected despite its zero-test successful exit. The timeout uses kill_on_drop and does not claim synchronous reaping before runtime shutdown. Runtime ownership and reclamation code are unchanged.

### Follow-up implementation reviews

1. Isolation: child-only Command environment, exact test name, single test thread, and fixture creation after re-exec exclude unrelated harness forks from the lease-owning process. No global environment mutation or production unlock is introduced.
2. Assertion preservation: live root remains present/unreclaimed and is now explicitly reported preserved; after dropping the child, exactly one root must be reclaimed and its path absent. Failure messages include complete ReclaimReport values.
3. Process outcome: the completion sentinel prevents zero-test false success. Captured child output explains failures, and a 30-second timeout with kill-on-drop bounds hangs. Independent review found the sentinel omission before it was corrected; no production or portability blocker was found.

### Follow-up test reviews and results

1. Controlled inheritance RED failed the original strict assertion; release/reap control passed. The original CI seed was tested both before and after the fix, and its pre-fix local success is retained as a reproduction limitation.
2. Zero-test negative control failed as intended. The final isolated test passed 100/100 executions. Final pinned-nightly library run with the original shuffle seed passed 738 tests, zero failures, 12 ignored on macOS.
3. Exact workspace Clippy, formatting and diff checks passed after the sentinel change. The prior full-workspace evidence predates this test-only follow-up; remote CI is rerun for the final branch. No temporary probe, assertion mutation or artificial seed remains in the diff.

## Lean model of source coordinate boundaries

The model represents each Unicode scalar by its width in the original source
and in decoded UTF-8. Conversion walks complete scalars and maps a boundary to
the corresponding sum of widths. This admits the start and end boundaries and
rejects offsets inside a scalar or beyond the source. In Latin-1 every raw byte
is one scalar. Bytes below 128 occupy one UTF-8 byte, and the other bytes occupy
two. UTF-8 and ASCII use the same widths on both sides. A leading UTF-8 BOM is
part of the decoded text and contributes three bytes to the coordinates.

Lean proves that reversing a successful conversion recovers the original
offset for any list of positive widths. It also proves that the boundary after
an arbitrary prefix maps to the sum of the output widths in that prefix. For
Latin-1, this gives the raw offset plus the number of non-ASCII bytes strictly
before it. The Rust implementation keeps those expansion positions in a sparse
index. The model computes positions directly from scalar widths.

The 154 fixtures declare an encoding and render original bytes, decoded text,
and expected offsets in both directions. The Latin-1 cases cover every sequence
of up to three bytes from four boundary values, all 256 byte values in one file,
and longer mixed or consecutive expansions. The UTF-8 cases cover one- through
four-byte scalars, with and without a BOM. ASCII cases include controls and
CRLF. The Rust adapter calls `decode_python_source`, compares every offset from
zero through one byte past the end in each representation, and checks that
encoding the decoded text recovers the original bytes.

Five sensitivity checks detect using a decoded coordinate as a raw one,
accepting interior UTF-8 bytes, counting an expansion at its start, rejecting
the end boundary, and dropping the BOM from the coordinate layout. The input
fixtures use recognized encoding declarations and valid payloads. The next
model covers declaration recognition and decoding errors.

On macOS, all 154 cases matched across 3,771 raw offsets and 4,019 decoded
offsets. The largest fixture contains 274 raw bytes and 402 decoded UTF-8
bytes. Rust passed 690 core tests and 31 CLI integration tests. The CI
workflow suite passed 222 tests. Guarded Lean checks peaked at 1,262,960 KiB
under a 2 GiB limit and 20-second timeout. Single-job Rust checks peaked at
985,888 KiB under a 3 GiB limit.

```sh
(cd formal/HoiminOracle && lake exe generate_candidate_span -- --check-source-coordinates corpus/source-coordinates.jsonl)
(cd formal/HoiminOracle && lake exe generate_candidate_span -- --sensitivity)
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-core --test source_encoding --test source_encoding_properties --test candidate_policy
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --test analyzer_handler
```

## Lean model of source encoding declarations

The strict decoder searches the first two physical lines for a case-sensitive
`coding:` or `coding=` marker in a comment. The model reads CRLF as one line
ending, skips blank lines and comments without a name, and stops at the first
line of source code. Lean proves that the search consumes only two lines, that
source code blocks declarations on later lines, and that the first recognized
name takes precedence even when the codec it names is unsupported.

Codec selection normalizes the declared name and retains its original spelling
in diagnostics. Lean proves that a missing declaration selects UTF-8, and that
an incompatible declaration with a UTF-8 BOM produces a conflict before the
decoder examines the payload. The model covers Hoimin's UTF-8, ASCII, and
Latin-1 codecs. For ASCII it reports the first byte above 127 by its original
offset. For invalid UTF-8 the oracle checks the diagnostic category and whether
the encoding was explicit, leaving Rust's UTF-8 error detail to the standard
library.

The 360 cases comprise 216 combinations of header placement, line endings, and
payload bytes, 132 combinations of codec names, BOM presence, and payload bytes,
and 12 focused boundary cases. The Rust test compares the selected codec and
decoded text or the diagnostic category, spelling, and applicable byte offset.
Seven sensitivity checks exercise searching a third line, searching after
code, splitting CRLF, accepting the `utf8` alias with a BOM, reporting an
unsupported codec before a BOM conflict, normalizing the diagnostic name,
and selecting the second declaration ahead of the first.

On macOS, Rust matched all 360 cases, including 228 successful decodes and 132
diagnostics. The targeted run passed 691 core tests and 31 CLI integration
tests. The CI workflow suite passed 222 tests. Guarded Lean checks peaked at
1,295,568 KiB under a 2 GiB limit, and single-job Rust checks peaked at
973,040 KiB under a 3 GiB limit.

```sh
(cd formal/HoiminOracle && lake exe generate_candidate_span -- --check-source-declaration corpus/source-declaration.jsonl)
(cd formal/HoiminOracle && lake exe generate_candidate_span -- --sensitivity)
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-core --test source_encoding --test source_encoding_properties --test candidate_policy
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --test analyzer_handler
```

## Lean model of delimiter depth admission

Hoimin checks whether a mutation can add one through eight delimiters while
keeping the original source at or below depth 200. Lean counts the enclosing
pairs at each byte of a proposed range and counts the pairs strictly enclosing
an insertion point. A delimiter contributes to a range starting at its opening
byte and ceases to contribute at the end of its closing token. An insertion
exactly at an opening or closing end lies outside that pair. The Rust adapter
parses the source with the production parser and uses its ordinary
`DelimiterDepth` constructor and query methods.

Lean proves that an addition with fewer delimiters also fits when a larger
addition fits, that depth 200 is allowed, and that an empty range has the
insertion result. For a nonempty range, it proves that checking every byte is
equivalent to checking disjointness from a set of intervals, given that those
intervals cover exactly the bytes above the depth budget. The Rust comparison
also checks the number of retained intervals at each depth and observes that
shallow source retains none.

The 15 cases include pure parentheses at depths 190 through 200, mixed
delimiter kinds, two separate deep expressions, UTF-8 and CRLF preceding a
deep expression, and delimiter characters in comments or literals. The
adapter evaluates both admission methods for 4,312 combinations of byte range
and addition size. These are `internal-fixture` cases because the index and its
queries are private to the analyzer. The fixtures use balanced source within
the depth limit. The audit covers index admission independently of individual
mutation operators and cancellation.

The sensitivity check catches rejecting depth 200, counting an opener before
it occurs, including an exclusive range end, checking only the range start,
ignoring a later depth peak, and counting only round delimiters.

On macOS, Rust matched all 4,312 paired observations and all 120 interval
counts. The library suite passed 5,212 tests with 5 ignored, and the CI
workflow suite passed 222 tests. The largest source is 817 bytes. The Lean
checks used a 20-second timeout and 2 GiB limit. The single-job Rust run took
81 seconds and peaked at 3,048,992 KiB under a 3 GiB limit.

```sh
(cd formal/HoiminOracle && lake exe generate_candidate_span -- --check-delimiter-depth corpus/delimiter-depth.jsonl)
(cd formal/HoiminOracle && lake exe generate_candidate_span -- --sensitivity)
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-python --lib
```

## Lean model of source escape decoding

The regex and format-string analyzers share a decoder that starts immediately
past a backslash. The model observes the outcome, the next byte offset, and
the name passed to a caller-supplied Unicode lookup. It recognizes standard
escapes, combines CRLF into one continuation, reads at most three octal
digits, and wraps octal byte values modulo 256. Fixed-width hexadecimal
spelling requires ASCII digits. Unknown escapes preserve the next character
and advance by its full UTF-8 width. The caller supplies the name lookup, so
these cases exercise its invocation and byte limit independently of Unicode
database contents.

Lean proves that byte-string mode preserves the `u`, `U`, and `N` markers,
that a CRLF continuation consumes two bytes after the backslash, and that
byte-mode octal wrapping produces a value below 256. A malformed numeric
spelling leaves the cursor just after its marker. A complete spelling of an
unsupported Unicode scalar consumes its digits before rejection. Names of
128 or more bytes fail before the lookup is called.

The 125 `internal-fixture` cases use the production decoder. They cover
UTF-8 start offsets, newline forms, ordinary escapes, octal widths and byte
wrapping, truncated or malformed hexadecimal fields, surrogate and scalar
bounds, and the closing-brace boundary at 127 and 128 name bytes. Eight
sensitivity checks detect signed hexadecimal input, consuming a fourth
octal digit, omitting byte wrapping, splitting CRLF, shifting the name
bound in either direction, rewinding after an invalid scalar, and counting
an unrecognized multibyte character as one byte.

Four cases record an internal divergence. The minimal input `\x+A` starts
with the cursor at byte 1. Lean rejects it with the cursor at byte 2. Rust's
radix parser accepts the leading `+`, decodes U+000A, and leaves the cursor at
byte 4. The same distinction occurs in byte literals and with `\u+041` and
`\U+0000041` in text. The vendored parser requires each digit to be hexadecimal,
so the test also runs the same spellings through that parser in ordinary,
byte, and interpolated format literals as applicable. The corpus preserves
the model result and the observed internal divergence for all four cases.

On macOS, 121 decoder observations matched and the four signed spellings
reproduced the documented divergence. The vendored parser rejected all seven
literal wrappers for those spellings. The library suite passed 5,214 tests
with 5 ignored, and the CI workflow suite passed 222 tests. The longest
fixture is 133 bytes. The guarded Lean build peaked at 1,265,488 KiB within
20 seconds and 2 GiB. The first Rust build reached the 3 GiB cap. With
debug information and incremental compilation disabled, the single-job
Rust run finished in 77 seconds and peaked at 2,444,320 KiB.

```sh
(cd formal/HoiminOracle && lake exe generate_candidate_span -- --check-source-escape corpus/source-escape.jsonl)
(cd formal/HoiminOracle && lake exe generate_candidate_span -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-python --lib
```

## Lean model of regex literal spans and edits

The model assembles Python literal tokens from source fragments. Each fragment
emits one decoded character or consumes bytes without emitting a character.
The source spelling determines its UTF-8 width, so a character from a Unicode
escape covers the whole escape and a physical CRLF can cover two bytes.
Line continuations advance the source coordinate without creating a decoded
unit. Adjacent tokens add quote and separator bytes between their units.

Lean proves that placement preserves decoded character order, that one
decoded character projects to its complete spelling,
that an insertion at the last decoded boundary uses the final token's body
end, and that surrounding a span preserves its selected characters. The model
also specifies the result of replacing a decoded span by concatenating the
unmodified prefix, the replacement, and the unmodified suffix.

The 12 `internal-fixture` inputs cover bytes, raw and ordinary strings,
multibyte text, named and numeric escapes, mixed quoting, physical newlines,
line continuations, adjacent literals, and empty trailing tokens. For each
input the Rust adapter uses the production parser and literal mapper to check
every ordered in-bounds range, including empty spans, and two nonempty
out-of-bounds ranges. For 16 selected edits it applies the returned byte edit
and reparses the file expression and the materialized reference separately.
It checks the decoded values and counts retained comment and continuation
spellings in the edited expression. The cases exercise both sides of token
boundaries, including deletion next to a matching quote and surrounding a
span that begins partway through a raw backslash run. The fixtures choose the
literal edits directly, independent of regex grammar and operator selection.

Six sensitivity checks detect using decoded offsets as source offsets,
anchoring an empty span on the preceding unit, stopping at the start of the
last selected unit, clipping a range to its first unit, consuming the
exclusive endpoint, and dropping the selected text during a surround edit.

On macOS, all 261 byte ranges and all 16 edits matched. The library suite
passed 5,215 tests with 5 ignored, and the CI workflow suite passed 222
tests. Lean used a 20-second timeout and a 2 GiB cap per command. The Rust
run used one build job, took 39 seconds, and peaked at 2,513,584 KiB under a
3 GiB cap.

```sh
(cd formal/HoiminOracle && lake exe generate_candidate_span -- --check-regex-literal-span corpus/regex-literal-span.jsonl)
(cd formal/HoiminOracle && lake exe generate_candidate_span -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-python --lib
```

## Lean model of f-string format mutations

The model constructs a format specification from typed slots for fill,
alignment, sign, width, grouping, precision, presentation, and numeric flags.
A slot can contain literal source spelling, an escaped glyph, or a dynamic
field. The expected source offsets come from the UTF-8 lengths of the
rendered slots. The adapter parses the f-string and runs the two production
operators for width/precision neighbors and format option swaps.

Option swaps require the original format to pass Hoimin's subject and option
checks. Lean proves that each emitted one-character replacement also passes
those checks, including the restrictions for strings and for integer and
complex presentations. The locale code `n` can be removed to restore the
default. Width and precision use their nonnegative adjacent integers and
skip ineligible or non-static slots. Lean proves the numeric behavior at
zero, at the signed integer limit, and above the limit. The model uses a
parameter for the limit because Rust parses those numbers as `isize`.

The corpus has 448 combinations of seven subject expressions, four option
layouts, and 16 presentation states, plus 23 focused cases. Focused cases
cover dynamic fields, Unicode and escaped spellings, unsupported layouts,
and 32-bit and 64-bit numeric boundaries. The expected candidate list records
each operator, its byte range, and its replacement in emission order. The
Rust test also reparses each mutant. Both the production emitter and its
observations are exercised through an `internal-fixture` test. This audit
covers Hoimin's static admission rules and Python syntax acceptance.

Five sensitivity checks detect ignoring the subject type, losing the locale
deletion, editing ineligible numeric slots, incrementing beyond the integer
limit, and using a one-byte source range for an escaped option.

On 64-bit macOS, all 471 cases matched. The 1,136 numeric edits and 1,652
option edits all reparsed successfully. The library suite passed 5,216
tests with 5 ignored, and the CI workflow suite passed 222 tests. Three
fixtures have a distinct 32-bit expectation generated by Lean. The Lean
checks used a 20-second timeout and a 2 GiB cap per command. The Rust run
used one build job, took 43 seconds, and peaked at 2,443,744 KiB under a
3 GiB cap.

```sh
(cd formal/HoiminOracle && lake exe generate_candidate_span -- --check-format-spec corpus/format-spec.jsonl)
(cd formal/HoiminOracle && lake exe generate_candidate_span -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-python --lib
```
