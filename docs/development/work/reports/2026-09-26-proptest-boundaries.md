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

## Lean model of f-string format removal

The model renders trees of f-string fields and specifications. Each field
records its expression spelling, optional debug marker and conversion, and
whether its expression contains a named assignment or `yield`. The model
computes the source range of each specification and the peak delimiter depth
of its descendants. The public Rust analyzer receives the rendered program
with `fstring_format_spec_remove` selected.

A static specification is omitted, including the empty specification on a
debug field. A dynamic specification is replaced by an empty string selected
by a constant conditional. The other arm contains the original expressions
in source order, including descendants of nested specifications. An implicit
debug conversion becomes explicit `!r` for this rewrite. Lean proves that
the guarded form retains the original read list and executes none of it when
selecting the empty arm. It also proves that a disallowed child expression
blocks removal of its enclosing specification. Individual inner fields are
still visited. The dynamic rewrite requires room for two added delimiters.

The 145 `strict` fixtures include 112 combinations of quote style, debug
mode, conversions, and specification shape, 16 depth cases, and 17 focused
cases. The focused cases include named assignment, yield, triple quoting,
and lexical references for closures and zero-argument `super()`. Rust
compares candidate order, byte start, original spelling, and replacement,
then reparses each mutant. Five sensitivity variants detect removing an
ordinary empty specification, ignoring a disallowed expression, reversing
retained reads, forgetting the debug conversion, and exceeding the depth
budget by one.

On macOS, all 145 cases and their 151 edits matched. The 41 Rust integration
tests and 222 CI workflow tests passed. A local CPython 3.14 run compiled all
145 originals and 151 mutants. The execution probes `runtime-siblings`,
`runtime-debug`, and `runtime-nested` verify that only the removed fields'
calls disappear and that the rewritten debug field uses `repr`. Compiling
`compiler-free-variables` and `compiler-class-cell` before and after the edit
preserved the inner function's `width` and `precision` free variables and
the method's `__class__` reference, respectively. Lean used a 2 GiB cap and
20-second timeout per command. The single-job Rust integration run took
6.5 seconds and peaked at 486,032 KiB under its 3 GiB cap.

```sh
(cd formal/HoiminOracle && lake exe generate_candidate_span -- --check-format-removal corpus/format-removal.jsonl)
(cd formal/HoiminOracle && lake exe generate_candidate_span -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-python --test reference_mutations
```

## Lean model of AST depth admission

The model starts the module at depth 1 and increases depth by one for each
child reported by the source-order visitor. The Rust visitor pushes children
in source order onto a LIFO worklist. Lean proves that taking a frame and
pushing its children preserves the remaining visit schedule, and that a
worklist with sufficient fuel has the same result and poll count as scanning
that schedule. Without cancellation, acceptance is equivalent to every
scheduled node lying at or below the limit.

The checked parse rejects invalid syntax before it calls the traversal's
cancellation predicate. Each visited node polls once before the depth test.
Lean proves that cancellation wins at the same node as a depth rejection and
that a node at the depth limit can proceed. The model applies to the checked
parser helper, after any cancellation checks owned by its callers.

The 40 `internal-fixture` inputs pair Python source with ordered trees for
assignment expressions, annotations, parameters, type parameters, nested
interpolations in f-strings and t-strings, class patterns, and nested
statements. They include an empty module, a wide shallow module, and syntax
errors after both shallow and deep statements. The 266 queries vary the
cancellation checkpoint around the first and last polls and the first
depth failure. The Rust test calls the ordinary parser helper and compares
the outcome and exact count, including zero polls for invalid syntax.

Six sensitivity checks detect visiting siblings in the opposite order,
starting the module at zero, rejecting at the exact limit, confusing depth
with node count, checking depth before cancellation, and traversing invalid
syntax. The observed scope is parser admission and polling. The existing
bounded-stack Rust tests supply evidence for disposal of large rejected trees.

On macOS, all 266 queries matched. This included 181 cancellations,
57 acceptances, 20 depth failures, and 8 syntax failures. The Rust library
suite passed 5,217 tests with 5 ignored, and the CI workflow suite passed
222 tests. Guarded Lean commands used a 2 GiB cap and a 20-second timeout
and peaked at 1,233,360 KiB. The single-job Rust run finished in 42 seconds
and peaked at 2,533,312 KiB under a 3 GiB cap.

```sh
(cd formal/HoiminOracle && lake exe generate_candidate_span -- --check-ast-depth corpus/ast-depth.jsonl)
(cd formal/HoiminOracle && lake exe generate_candidate_span -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-python --lib
```

## Lean model of symbol catalogs and candidate selection

The model assigns each function, async function, and class the names of its
lexically enclosing definitions followed by its own name. Ordinary suites
keep the existing qualifier. Lean proves that a definition qualifies its body
while preserving the surrounding qualifier for subsequent statements. The
catalog deduplicates names. Separate occurrences of a repeated definition
can still carry candidates with the same qualifier at different offsets.

Candidate selection uses whole name components. An unqualified selector
admits its named scope and descendant scopes. A module-qualified selector
also requires a whole-component suffix of the analyzer's current module
path. The `__init__.py` stem is omitted from that path. Lean proves that a
selected ancestor admits its descendants, that a distinct next component
prevents ancestry, and that the unrestricted selection admits module-level
sites. A nonempty selection excludes module-level sites.

The 21 strict fixtures render functions and classes in lexical and control
flow suites, including branches, handlers, match cases, async suites, empty
and unreachable definitions, and redefinitions. They also exercise
decorators and defaults, class headers, CRLF and Unicode, and package
initializers. The adapter invokes `definition_names` and the public mutation
analyzer on the same source. It selects the Boolean literal operator to
observe enclosing scope and compares the byte start, original, replacement,
and attached symbol of each candidate in emission order. CLI module discovery
and reporting have their own oracle.

The corpus compares 81 catalog entries and 440 candidate records across 248
selector queries. Six sensitivity checks detect exact-only selection,
character-prefix selection, ignoring the module qualifier, character-suffix
module matching, treating a filtered-out selector list as unrestricted,
and dropping enclosing names from definitions. On macOS, all cases matched,
42 Rust integration tests passed, and 222 CI workflow tests passed.
Lean checks used a 2 GiB cap and 20-second timeout per command. The single-job
Rust run took 6.8 seconds and peaked at 482,592 KiB under its 3 GiB cap.

```sh
(cd formal/HoiminOracle && lake exe generate_candidate_span -- --check-symbol-catalog corpus/symbol-catalog.jsonl)
(cd formal/HoiminOracle && lake exe generate_candidate_span -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-python --test reference_mutations
```

## Lean model of combined line and symbol admission

Line selection asks whether the query shares an inclusive line with at least
one valid requested range. An empty input allows all lines. A nonempty input
whose ranges all run backwards allows none. The symbol predicate from the
catalog model applies in conjunction with the line predicate.

Lean proves that the start cutoff selects a prefix of the sorted ranges.
It then proves that the largest end among that prefix answers the same query
as scanning every requested range. The proof covers empty inputs, filtered
inputs, overlaps, and nested ranges. The test uses Hoimin's prepared selection
index and compares 6,732 results across 11 range sets, six selector sets,
17 spans, and six enclosing scopes. The endpoint fixtures include zero and
the largest supported line number.

The regex fixtures observe two decisions in the actual analyzer. Pattern
inspection can begin when the requested lines overlap a literal or its flags.
The final mode-switch edit uses its start line as the selection anchor, even
when the edit covers bytes on later lines. The test observes whether the
literal was decoded and whether a candidate was emitted. For candidates it
compares the byte span, text, anchor line, and symbol. The 324 queries use
six combinations of LF, CRLF, or CR and pattern/flags order in a nested
function. Test-only inspection counters make these internal fixtures.

Six sensitivity checks detect treating an empty filtered list as
unrestricted, using only the last eligible end, excluding equal endpoints,
using a union with symbol selection, admitting inspection only on the first
line, and emitting an edit on any intersecting line.

On macOS, all queries matched. Of the regex queries, 90 decoded the pattern
and emitted a candidate, 90 decoded it without emitting a candidate, and
144 skipped decoding. The Rust library suite passed 5,218 tests with 5 ignored.
The CI workflow suite passed 222 tests. Guarded Lean checks peaked at
1,331,360 KiB under a 2 GiB cap and 20-second timeout. The Rust run took
43 seconds and peaked at 2,636,672 KiB under a 3 GiB cap with one build job.

```sh
(cd formal/HoiminOracle && lake exe generate_candidate_span -- --check-candidate-selection corpus/candidate-selection.jsonl)
(cd formal/HoiminOracle && lake exe generate_candidate_span -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-python --lib
```

## Lean model of source lines and candidate columns

The model walks Unicode scalars and records the byte offset, one-based
physical line, and zero-based column at each boundary. LF and lone CR
start a new line. CRLF starts just one new line, after LF. The intermediate
boundary after CR remains on the earlier line. A leading file BOM consumes
three UTF-8 bytes and contributes zero columns. Subsequent BOM scalars
contribute one column. Other Unicode separators stay on the same physical
line. A query inside a scalar or past the end has no location.

Lean proves that a row's byte length minus its accumulated UTF-8 excess
gives the number of scalars. Subtracting the global excess at the line
start from the global excess at the query gives exactly the excess for
that row prefix. The proof also accounts for subtracting the initial BOM.
The core Rust test compares physical line starts and all byte offsets for
230 strings, including combinations of CR and LF, interior BOMs, combining
marks, and multibyte scalars. Queries include EOF and out-of-range offsets
and revisit earlier offsets after later ones.

The CLI fixtures render 18 files across UTF-8, UTF-8 with BOM, and declared
Latin-1, with LF, CRLF, or CR and with or without a final newline. The
ordinary analyzer handler reads each file and records two Boolean mutations
in its candidate spool. The test replays those records and compares the raw
byte span, decoded start, line, column, symbol, and replacement. The real
candidate validator accepts the correct location and rejects an adjacent
incorrect column for each candidate.

Seven sensitivity checks detect byte columns, splitting CRLF, counting the
initial BOM, ignoring interior BOMs, forgetting to reset columns, snapping
offsets inside a scalar to a boundary, and storing decoded offsets as raw
offsets for Latin-1.

On macOS, all 2,958 queries and 36 recorded candidates matched. The core
candidate-policy suite passed 93 tests, the CLI analyzer-handler suite
passed 32, and the CI workflow suite passed 222. Lean checks used a 2 GiB
cap and 20-second timeout per command. The single-job Rust runs peaked at
483,152 KiB for core and 794,512 KiB for the CLI under a 3 GiB cap.

```sh
(cd formal/HoiminOracle && lake exe generate_candidate_span -- --check-source-locations corpus/source-locations.jsonl)
(cd formal/HoiminOracle && lake exe generate_candidate_span -- --check-candidate-locations corpus/candidate-locations.jsonl)
(cd formal/HoiminOracle && lake exe generate_candidate_span -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-core --test candidate_policy
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --test analyzer_handler
```

## Lean model of analyzer source admission

The model applies the strict source decoder before using the parser
outcome. A missing source or a decoding failure stops target traversal.
A syntax error in a decoded source contributes a diagnostic and leaves
later targets eligible. Lean proves that a decoding failure takes
precedence over any parser outcome and that appending files after the
first fatal error leaves the examined prefix and the failure unchanged.

The 214 cases use real files with LF, CRLF, and CR endings, including
UTF-8, ASCII, Latin-1, BOMs, and conflicting or unsupported declarations.
All ordered pairs of file outcomes and six selected triples check mixed
failures and successful continuation. The cases use the Boolean literal
operator with capacity for every candidate so they isolate source
admission from exception-project loading and candidate limits.

Rust runs both candidate discovery and the streaming analyzer handler
against the generated expectations. The test compares diagnostics and
failure codes, then replays the final candidate spool for successful
batches and compares it with discovery. A missing file has a fixed
message that omits the path. The streaming test checks its position
against the original request. The UTF-8 failure expectation includes
the path and declared codec and compares the stable part of the message
before the Rust standard library's error detail.

The four sensitivity checks detect recovery from a decoding error,
checking syntax before decoding, aborting on syntax errors, and
choosing the last failure. On macOS, all 214 cases matched through both
entry points. The CLI analyzer-handler suite passed 33 tests and the CI
workflow suite passed 222.

```sh
(cd formal/HoiminOracle && lake exe generate_candidate_span -- --check-analyzer-source-admission corpus/analyzer-source-admission.jsonl)
(cd formal/HoiminOracle && lake exe generate_candidate_span -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --test analyzer_handler
```

## Lean model of descriptor-length reads

`SnapshotReader` starts with the length observed on its open descriptor.
A nonempty request reads at most the smaller of the remaining budget and
the caller's buffer capacity. Only bytes delivered to the caller reduce
the budget. EOF with bytes still due is a shrink error. At zero budget
the reader probes one byte and distinguishes exact EOF from growth.
Empty requests leave the underlying reader untouched, and failed reads
preserve both the remaining budget and the underlying error.

Lean proves that bytes delivered plus the remaining budget equal the
initial budget of each call. A drain that stops at EOF, a length change,
or an ordinary I/O error reads at most the initial length plus one byte,
including any retries after interruption. The model assumes that
underlying requests return at most the number of bytes requested and
that failed reads consume no bytes. Same-length rewrites and filesystem
I/O duration are outside the modeled contract.

The corpus covers the eight-call alphabet at depths one and two for
initial lengths 0, 1, 4, 5, and `u64::MAX`, plus four selected traces of
length three or four. The 364 traces contain 693 calls. The Rust unit
adapter exercises the actual reader using a scripted underlying read and
compares its requested width, consumed bytes, remaining budget, outcome,
and buffer writes. It also replays 122 traces using real files with
controlled length changes and compares physical descriptor offsets.
Both adapters use the internal-fixture mode because the reader is a
private component. The source read and hash, report decoding, report
comparison, and workspace copy tests exercise its consumers.

Seven broken variants cover an empty request touching the file, reading
past the allowance, widening or skipping the EOF probe, treating early
EOF as success, spending the whole request on a short read, and charging
the budget for an error.

On macOS, all 364 scripted traces and all 122 real-file replays matched.
The targeted CLI run passed 266 tests with 10 ignored and the CI workflow
suite passed 222. Lean commands stayed within a 2 GiB cap and a 20-second
timeout per command. The targeted Rust build and tests used one build
job and peaked at 2,073,776 KiB under a 3 GiB cap.

```sh
(cd formal/HoiminOracle && lake exe generate_timeout_limit -- --check-snapshot-reads corpus/snapshot-reads.jsonl)
(cd formal/HoiminOracle && lake exe generate_timeout_limit -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --lib -- --test-threads=2 finite_file plan::source artifacts::run_report progress::duplicate workspace::copy
```

## Lean model of progress duplicate warnings

`progress` reports the first occurrence of a repeated path even when an
earlier report at a different path has identical bytes. With no prior
occurrence of the path, it chooses the first earlier input in the same
fingerprint bucket that can still be confirmed byte for byte. Missing or
changed files can fail confirmation, so the search moves to the next
candidate. Each input stays in the comparison history.

Lean proves the earliest-index conditions for both warning reasons,
including the path rule's precedence. It proves that an unavailable file
cannot confirm a byte match and that projecting the inputs from an
annotated history preserves their order for any annotation rule. The
existing progress decision model computes the adjacent comparison states,
the final decision, and the consecutive stall count for the strict cases.

The 84 strict cases exercise the public CLI entry point with real reports.
They enumerate ordered pairs and triples across four stable paths, plus
four longer or mixed histories. Different whitespace can preserve the
meaning of a report while changing its bytes. The mixed histories include
repeated incomplete reports that warn and still break the stall chain.
The adapter checks warning text and order, the retained input paths, and
the comparison outcomes.

The 71 internal-fixture cases replay a two-input matrix over eight
actions plus seven selected histories. They supply fingerprints to the
private tracker so different contents can deliberately collide, and edit
real files between observations to exercise optional confirmation.
Confirmation uses the production file reader. The matrix treats files
as stable inside each observation. Existing tests separately cover
growth and shrinkage during a read. Seven broken variants cover path
priority, the last occurrence in either branch, trusting the fingerprint
alone, ignoring the fingerprint, treating two missing files as equal,
and stopping at the first failed confirmation.

On macOS, all 84 CLI cases and 71 tracker cases matched. The targeted Rust
runs passed 18 unit tests and 34 CLI integration tests, and the CI workflow
suite passed 222. Lean commands ran under a 2 GiB cap with a 20-second
timeout per command. The Rust runs used one build job and a 3 GiB cap.

```sh
(cd formal/HoiminOracle && lake exe generate_progress_input -- --check-duplicates corpus/progress-duplicates.jsonl)
(cd formal/HoiminOracle && lake exe generate_progress_input -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --lib -- progress::duplicate
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --test progress -- duplicate output_defers_mixed_history_rendering
```

## Lean model of buffered byte comparison

The exact-byte confirmation used by `progress` compares the common-length
prefix of the two available buffers. On a match it consumes that many
bytes on both sides, retaining any remainder in the longer buffer. It
stops at a byte mismatch or at EOF and returns equality only if both
streams end together. Each iteration fills the left buffer first and
propagates the first read error. A successful left EOF still requires a
right fill because the right stream might have more bytes or a read error.

Lean proves for arbitrary finite, error-free streams that the comparison
returns equality exactly when all their bytes are equal, regardless of
how either stream was split into nonempty buffers. The inductive step
relates the matched prefix to the original byte sequences, then proves
that the number of remaining bytes decreases. Separate theorems show
that a left error stops before reading the right side and that a right
read error takes precedence over a successful left EOF.

The 324 internal-fixture cases pair 18 streams in both orders. They cover
empty streams, strict prefixes, mismatches before and after a refill,
different partitions of the same bytes, and interruption and other I/O
errors before or after a matching prefix. The adapter supplies valid
`BufRead` buffers to the same private loop used by the real file wrapper.
It verifies each fill and consume against the generated trace and checks
the returned result or error. Existing real-file tests cover the wrapper
and its descriptor-length checks for growth and shrinkage.

The sensitivity check detects six broken variants. They compare full
buffers, discard a longer buffer's remainder, treat either EOF as equal,
skip the right fill at left EOF, prefer a right error over a left error,
or continue beyond a byte mismatch.

On macOS, all 324 traces matched. The targeted Rust runs passed 162 unit
tests and 34 CLI integration tests, and the CI workflow suite passed
222. Lean used a 2 GiB cap and a 20-second timeout per command. The Rust
runs used one build job and a 3 GiB cap.

```sh
(cd formal/HoiminOracle && lake exe generate_progress_input -- --check-buffered-comparison corpus/progress-buffered-comparison.jsonl)
(cd formal/HoiminOracle && lake exe generate_progress_input -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --lib -- --test-threads=2 progress::duplicate finite_file artifacts::run_report
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --test progress -- --test-threads=2 duplicate output_defers_mixed_history_rendering
```

## Lean model of fixed-cap file reads

Plan and review artifact loading and portable source acquisition share a
two-stage size policy. The length observed on the open regular file must
fit the cap before reading begins. The admitted read consumes at most
the cap plus one byte. It returns the complete payload within the cap
and uses the extra byte as evidence to reject an oversized payload.
An admitted file that shrinks or grows within the cap can still succeed.

Lean proves the metadata rejection, preservation of every byte in an
admitted payload, and exact consumption of the first excess byte when
the payload grows past the cap. The total consumed byte count is bounded
by the cap plus one in all modeled cases. The model assumes regular files
and successful underlying reads, with the size change between the
metadata check and reading. Opening policy and I/O error classification
are exercised separately by the existing Rust suites.

The corpus crosses four caps, eight metadata lengths, and eight payload
lengths. A 257th case uses a file extended to one byte above the deployed
64 MiB artifact cap and checks rejection before the file contents are
read. The internal-fixture test runs the two existing Rust read paths
on real files. A test-only hook runs after metadata admission so the
adapter can replace bytes on the same open file and observe the
descriptor cursor. It then compares the returned bytes or the size
diagnostic at each path's boundary.

The six broken variants skip metadata, reject an exact bound in either
phase, omit the extra byte, use the initially observed length as the
cap, or read one byte too far.

On macOS, all 257 cases matched for both readers. The targeted Rust
suite passed 60 tests and the CI workflow suite passed 222. Lean ran
under a 2 GiB cap and a 20-second timeout per command. The single-job
Rust run peaked at 2,073,952 KiB under a 3 GiB cap.

```sh
(cd formal/HoiminOracle && lake exe generate_timeout_limit -- --check-bounded-file-reads corpus/bounded-file-reads.jsonl)
(cd formal/HoiminOracle && lake exe generate_timeout_limit -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --lib -- --test-threads=2 artifacts::tests finite_file source_file review::input::input_boundary_tests plan::review_tests analyzer::exception_project reduce::input
```

## Lean model of review source grouping

Review visits distinct source paths in the order of their first
occurrence and uses one source observation per path. Each candidate
still receives its own result in the original candidate order. This
means an error for an earlier candidate on the second file reaches the
caller before a later error on the first file.

The model distinguishes an invalid path, an unavailable file, and an
opened file. For an opened file a mismatch against the current raw
hash gives `stale`. With a matching hash, the candidate's mutation,
span, and original-text checks precede the retained decoding error.
Location and stable-ID checks follow source validation. Lean proves
that each path occurs once in the visit list, that the first candidate
path is visited first, and that using the captured observations yields
the pointwise results in candidate order. Separate theorems cover the
stale and original-text precedence.

The corpus uses seven file fixtures, including valid UTF-8 and Latin-1,
invalid UTF-8, an unsupported codec, a missing file, and an invalid
path. It crosses both hash states with six candidate shapes for each
file, then exercises interleaved batches and competing errors. The
internal-fixture adapter invokes the existing Rust batch entry point
with real files. It checks the read-attempt order, the count of created
contexts, all result values and diagnostics, and the first error index.
The corpus uses distinct path spellings and selects one descriptor
defect per candidate. Files remain stable within each batch.

Seven broken variants exercise hashing after validation, checking
decoding too early, misclassifying missing files, ignoring candidate
identity, returning grouped results, sorting paths before reads, and
reusing the first candidate's result for all candidates on a path.

On macOS all 207 cases matched. The targeted Rust runs passed 39 unit
tests and 43 CLI integration tests, and the CI workflow suite passed 222.
Lean ran under a 2 GiB cap and a 20-second timeout per command. The
single-job Rust runs stayed under a 3 GiB cap, with a peak of 2,087,744 KiB.

```sh
(cd formal/HoiminOracle && lake exe generate_survivor_review -- --check-review-sources corpus/review-sources.jsonl)
(cd formal/HoiminOracle && lake exe generate_survivor_review -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --lib -- --test-threads=2 review::input review::tests::source review::tests::review_list_reads_and_indexes_each_source_once_per_command plan::review_tests
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --test review_cli -- --test-threads=2
```

## Lean model of review report admission

Review first reads the bounded artifact and delegates JSONL decoding and
structural validation to the shared report reader. Once those checks pass,
it resolves the report project, checks for a passing baseline and a
complete run, and compares the canonical report project with the plan.
It then looks up each reported candidate ID in the plan and compares
descriptors for overlapping IDs. A report may contain extra IDs or leave
out plan IDs. Accepted reports retain the reported candidate order.

Lean proves that artifact failures and bad summary counts preempt later
checks, and that project-resolution failure preempts usability and plan
lookups. For an arbitrary ordered candidate list, the model retains that
order on a compatible join and stops at precisely the first incompatible
descriptor. Descriptor tokens represent a difference in original text
that leaves the stable candidate ID intact.

The 97 internal-fixture cases drive the existing Rust report-admission
entry point with real files. They cover missing and oversized inputs,
JSON and typed configuration failures, old schema input, conflicting
structural and identity errors, a canonical project alias, missing and
different project roots, baseline and completion combinations, and
overlaps at different event positions. The adapter checks the diagnostic
category, the number of plan lookups, accepted candidate order, and the
reference's run ID, canonical path, and raw-byte fingerprint. The corpus
uses one plan candidate and two possible report-only candidates. Files
and project paths remain stable within each call.

Eight broken variants expose reordered summary, identity, project, or
usability checks, an ID-only comparison, requiring either side's IDs to
be a subset of the other, scanning past a descriptor conflict, and
sorting the admitted candidates.

On macOS all 97 cases matched. The targeted Rust runs passed 249 unit
tests and 43 CLI integration tests, and the CI workflow suite passed 222.
Lean ran under a 2 GiB cap and a 20-second timeout per command. Rust ran
with one build job under a 3 GiB cap.

```sh
(cd formal/HoiminOracle && lake exe generate_survivor_review -- --check-review-report-admission corpus/review-report-admission.jsonl)
(cd formal/HoiminOracle && lake exe generate_survivor_review -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --lib -- --test-threads=2 review::tests artifacts::run_report
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --test review_cli -- --test-threads=2
```
