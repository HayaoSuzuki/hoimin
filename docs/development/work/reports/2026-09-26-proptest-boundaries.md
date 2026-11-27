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

## Lean model of review store destination checks

Review checks the database, rollback journal, WAL, and SHM in that order
before admitting the optional report or opening the database. A missing
destination entry requires no protected-input lookups. An existing
destination must be a regular file and must have a different descriptor
identity from each readable protected file. The input order is plan,
optional report, manifest sources, and environment fingerprint inputs.
A missing protected path or an opened nonregular input can be skipped.
Other protected-input I/O failures propagate.

Lean proves that an accepted destination set has no protected-file alias.
It also proves that appending checks after the first failed destination
or protected input preserves the failure. The proofs compare observed
descriptor identities. The filesystem fixture keeps paths stable during
each call.

The corpus has 46 portable internal-fixture cases and 13 Unix-only cases.
The portable cases put hard links to each protected input in each
destination slot, accept separate copies with identical bytes, and
compare the diagnostics from competing failures. Unix cases exercise
symlinks, nonregular protected inputs, and a report path with a regular
file as its parent. The adapter invokes the real validator, compares
diagnostics, and verifies that regular-file contents are preserved.
The public CLI test confirms that an alias in the WAL is rejected before
parsing a malformed report, even with the main database absent.

Eight broken variants expose skipping each companion, reversing the
destination order, ignoring file identity, omitting fingerprint input
protection, ignoring a protected-input I/O error, and checking inputs for
a missing destination.

On macOS all 59 cases matched. The targeted Rust runs passed 124 unit
tests and 44 CLI integration tests, and the CI workflow suite passed 222.
Lean used a 2 GiB cap and a 20-second timeout per command. Single-job
Rust peaked at 2,129,040 KiB under a 3 GiB cap.

```sh
(cd formal/HoiminOracle && lake exe generate_survivor_review -- --check-review-store-destination corpus/review-store-destination.jsonl)
(cd formal/HoiminOracle && lake exe generate_survivor_review -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --lib -- --test-threads=2 review::tests artifacts::tests
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --test review_cli -- --test-threads=2
```

## Lean model of review listing and operator counts

The listing joins planned candidates to saved reviews and optional
execution results by stable ID. It keeps the plan order and preserves
history for candidates whose source is stale or unavailable. The
latest review is applicable only when the source is current. A report
that was never loaded and a loaded report missing the candidate have
different execution states.

With `--stale`, saved candidates absent from the plan appear after the
main list in snapshot order. They retain history and compatible
execution results, use display sequence zero, and leave the latest
review empty. Operator totals use only plan rows and count each as
reviewed, unreviewed, stale, or source unavailable. The assessment
breakdown uses the latest review of each reviewed row.

Lean proves the stale membership rule, preservation of retired history
and the empty review field, and the two accounting identities for row
counts and assessed reviews. The corpus crosses four source layouts,
three stored histories, four report layouts, and both `--stale` values.
Six additional cases exercise stored-identity and retired-report
descriptor conflicts, including conflicts absent from the displayed
rows. Eight broken variants cover these joins and the accounting rules.

The Rust adapter generates a plan with three current candidates from
two operators and two candidates whose reviews outlive the plan. It
seeds real SQLite stores through the normal append method, supplies
current-schema JSONL reports, and invokes the public list entry point.
It compares both ordered lists, descriptor preservation, history and
applicable review, execution status, operator counts, and error
diagnostics. The internal-fixture mode allows seeding a key with
different original text and the same stable identity.

On macOS all 102 cases matched. The targeted Rust runs passed 118 unit
tests and 44 CLI integration tests, and the CI workflow suite passed 222.
Lean used 2 GiB and 20-second limits per command. Single-job Rust
peaked at 2,096,608 KiB under a 3 GiB cap.

```sh
(cd formal/HoiminOracle && lake exe generate_survivor_review -- --check-review-listing corpus/review-listing.jsonl)
(cd formal/HoiminOracle && lake exe generate_survivor_review -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --lib -- --test-threads=2 review::tests
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --test review_cli -- --test-threads=2
```

## Lean model of review record retries

A fresh source reaches the store. A matching expected revision appends
one event for the selected candidate. A rollback or write-lock conflict
leaves existing histories intact. Creating the database can precede
rejection of the first write. A source that has become stale or
unavailable is rejected before creating a database.

The record command commits before rendering its response. If writing the
response fails, it reports the saved revision. Failure on the first
write leaves empty output. Failure on the final newline leaves a
complete JSON document. In both cases the event remains visible after
reopening, and a retry with the old expected revision conflicts.

Lean proves that rollback preserves both candidate histories, that
output failure preserves exactly the same committed state as successful
output, and that repeating the same expected revision after a lost
response is rejected. A commit on one candidate preserves the other
candidate's history. The model treats storage operations at the observed
commit and rollback boundaries and uses revisions below the store cap.

The 108 internal-fixture traces cover all ordered pairs of ten record
actions and eight longer or boundary traces, for 232 operations. The
adapter uses the public record command and invokes show after a saved
output error. The store fixture uses the existing pre-commit failure
hook and a real write lock from a separate SQLite connection. A fresh
read-only connection checks both histories and database presence after
each operation, including the revision and draft that survived. Eight
broken variants exercise rollback, output loss, the saved revision,
revision scope and conflicts, busy writes, the source check, and the
newline boundary.

On macOS all 232 operations matched. The targeted Rust runs passed 121
unit tests and 44 CLI integration tests, and the CI workflow suite passed
222. Lean used a 2 GiB cap with a 20-second timeout per command. Rust ran
with one build job under a 3 GiB cap.

```sh
(cd formal/HoiminOracle && lake exe generate_survivor_review -- --check-review-record corpus/review-record.jsonl)
(cd formal/HoiminOracle && lake exe generate_survivor_review -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --lib -- --test-threads=2 review::tests review::store_tests::store_rolls_back_failed_append review::store_tests::store_busy_rejection_leaves_no_history
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --test review_cli -- --test-threads=2
```

## Lean model of first review store publication

An initializer builds the database and SQLite journal inside a private
directory. The five observed private phases are creation, the three
schema statements, and readiness to publish. Publication can install a
complete database at an absent path. If the path is occupied, the
initializer preserves it and validates the public file when reopening.
A compatible initializer sees the published history. A different
project or an unrelated file is rejected.

Lean proves that observing a private phase leaves the public file
unchanged, that publication preserves every already present file, and
that a returned initialization error or unwind leaves the publication
unchanged. A compatible reopen appends to the existing history in the
model.

The 52 internal-fixture schedules contain 555 boundary observations.
They park a competing initializer at each private phase, alternate
initializers, sweep returned errors and unwinds at all five phases,
and introduce an unrelated file before or during initialization.
They cover compatible and different projects and retry after an
interruption. The Rust adapter uses the existing initialization hooks
to coordinate scoped threads. On every successful reopen it appends a
worker marker through the real store API. At each boundary it compares
the public database, ordered history, and count of private directories.
This observes the existing SQLite and filesystem operations at their
test hooks, including the absence of public journal files at those
boundaries. The schedules exercise ordinary errors and unwinds.

Seven broken variants expose early publication, overwriting or bypassing
an existing winner, leaked private state, losing the winner on abort,
accepting a different project, and accepting an unrelated file.

On macOS all 555 observations matched. The targeted Rust runs passed
43 unit tests and 44 CLI integration tests, and the CI workflow suite
passed 222. Lean used a 2 GiB cap with a 20-second timeout per command.
Single-job Rust peaked at 2,147,840 KiB under a 3 GiB cap.

```sh
(cd formal/HoiminOracle && lake exe generate_survivor_review -- --check-review-store-publication corpus/review-store-publication.jsonl)
(cd formal/HoiminOracle && lake exe generate_survivor_review -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --lib -- --test-threads=2 review::store_tests
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --test review_cli -- --test-threads=2
```

## Lean model of existing review store admission

The file preflight checks file type and the 16 MiB limit before reading
the SQLite journal format bytes. A SQLite header must have format
bytes `[1, 1]` before either open proceeds to SQLite. Admission then
checks application and version identifiers, schema objects, project
metadata, journal mode, integrity, and page geometry in that order.
Lean proves the priority of the file limit, header rejection, unexpected
objects, and unsupported page size. It also proves that a database with
4096-byte pages meets the geometry limit exactly when it has at most
4096 pages.

The 106 cases include combinations of identity, schema and project
checks, and conflicting failures in the same file. The 104 actual file
fixtures on macOS exercise both read and write admission. One of these
uses a Unix symbolic link. Tests compare directory entries and stream
file contents into digests after both operations. A write open on an
absent path creates a database that can then be reopened. The two
model-only cases exercise the inner journal mode and page count guards
independently of the earlier on-disk preflight.

An unreferenced page fails SQLite's integrity check for both access
modes. In the bundled SQLite 3.54.1, a one-byte revision is reported by
`quick_check` when the database is reopened for writing. A read-only
reopen omits the CHECK constraint during schema loading. The Rust
adapter checks that reading the affected candidate after read-only
admission still fails with an invalid stored revision. Eleven defective
variants exercise the admission guards and the access-dependent
integrity result.

On macOS all 208 admission observations matched. The targeted Rust runs
passed 44 unit tests and 44 CLI integration tests. The CI workflow suite
passed 222 tests. The Lean build peaked at 1,299,280 KiB under a 2 GiB
cap. Single-job Rust peaked at 2,133,872 KiB under a 3 GiB cap.

```sh
(cd formal/HoiminOracle && lake exe generate_survivor_review -- --check-review-store-admission corpus/review-store-admission.jsonl)
(cd formal/HoiminOracle && lake exe generate_survivor_review -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --lib -- --test-threads=2 review::store_tests
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --test review_cli -- --test-threads=2
```

## Lean model of review store snapshot validation

After the DB is admitted, the reader rejects an event count above 10,000
before scanning candidates. In storage order, it validates each candidate
key and checks that the history exists, that revisions start at one and
are consecutive, and that every stored draft decodes and passes validation.
It reconciles the total count against the events visited to catch orphan
events. Lean proves that selecting a candidate preserves every validation
failure and, on success, returns its projection of the complete snapshot.
It also proves the event cap and revision error priorities and the
consecutive revision property by induction. The model takes candidate
and revision order and the individual key and draft decoding and validation
outcomes as inputs.

The matrix combines fifteen row shapes across two candidate slots and
adds eight cases for orphan events, conflicting failures and the count
boundary. The 233 DB fixtures exercise four queries each, one full
snapshot and one lookup for each of two candidate slots and an absent ID.
The adapter uses a newly created review schema, inserts rows in reverse
order, and reopens with the real read-only store. Successful results are
checked against the complete expected keys and tagged revision drafts.
Every query also checks the DB content digest and directory entry count.
Nine defective model variants exercise filtering, ordering, the count
boundary, orphan detection and validation of keys, revisions and drafts.

All 932 reads matched on macOS. The targeted Rust runs passed 45
unit tests and 44 CLI integration tests. The CI workflow suite passed
222 tests. Lean peaked at 1,280,160 KiB under a 2 GiB cap. Single-job
Rust peaked at 2,172,832 KiB under a 3 GiB cap.

```sh
(cd formal/HoiminOracle && lake exe generate_survivor_review -- --check-review-store-snapshot corpus/review-store-snapshot.jsonl)
(cd formal/HoiminOracle && lake exe generate_survivor_review -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --lib -- --test-threads=2 review::store_tests
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --test review_cli -- --test-threads=2
```

## Lean model of run fingerprint encoding

The schema-13 encoding starts with its version byte and frames thirteen
tagged fields with unsigned 64-bit little-endian lengths. Lean proves
that a stream of these frames uniquely identifies the field sequence
when each payload length fits in the encoded width. It proves that
permuting sources, targets and operators preserves the encoded bytes,
that the two command argument kinds have distinct encodings, and that
jobs, max-mutants and max-output are excluded. The framing proof
concerns the input bytes passed to BLAKE3.

The 38 cases exercise every field, including sorting and duplicate
removal, line range multiplicity, ordered roots and glob rules, arbitrary
Unix bytes and Windows code units, fractional durations, and captured
environment metadata. The file record fixtures use normalized
root-relative paths and include a pair whose component order differs
from raw string order. Each case checks its expected changed fields
against the reference. Eleven deliberately incorrect encoders cover
framing, ordering, argument kind, the environment digest and included
limits.

The Rust test compares the complete preimage byte for byte and checks
that the public fingerprint equals BLAKE3 of the Lean preimage. An
internal function gives the test access to the same preimage that
production hashes.

In a separate macOS probe, the publicly constructible
`FingerprintInput` received two `FingerprintInputFile` records,
`p/a.py` and `p/./a.py`, each with the digest `"a".repeat(64)`.
The two records compared equal. Reversing them in
`fingerprint_inputs` changed the result of `fingerprint(&input)`
with every other field held fixed. Sorting and deduplication keep the
first spelling of equal records, and encoding uses that spelling's
raw bytes. CLI preparation supplies normalized paths. This observation
applies to manually supplied records with distinct spellings and equal
path components.

All 38 preimages matched on macOS, and the 47 existing resume policy
tests and 222 CI workflow tests passed. Lean peaked at 1,265,408 KiB
under a 2 GiB cap. The targeted Rust runs used one build job and a
3 GiB cap.

```sh
(cd formal/HoiminOracle && lake exe generate_run_fingerprint -- --check corpus/run-fingerprint.jsonl)
(cd formal/HoiminOracle && lake exe generate_run_fingerprint -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-core --lib resume::run_fingerprint_oracle_tests
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-core --test resume_policy
```

## Lean model of saved plan record comparison

Plan verification first validates the saved source and fingerprint input
records. It checks each record's portable path, lowercase 64-digit digest
and path uniqueness. Source inventory validation precedes fingerprint
input validation, and record validation precedes workspace comparison.
The comparison uses path and digest, reports added, removed and modified
files, and gives source changes priority. Paths in diagnostics use
component order.

Lean proves that an empty change list is equivalent to agreement on the
digest or absence of every path. It also proves validation priority,
duplicate rejection with a valid path and digest, and that displayed and
omitted counts sum to the complete change count. The display is bounded
at ten paths. The current inventory is assumed to come from the
workspace resolver with unique, normalized paths.

The 201 cases include an exhaustive comparison of two path slots with
absence and two digests per slot, invalid records, mixed changes at the
display boundary, and escaped stale paths. Nine cases use a saved plan
and a real temporary workspace through `prepare_verify`. The adapter
creates the original plan with the regular planner, substitutes actual
workspace digests for the two fixture digests, and checks full diagnostic
text and preservation of the saved plan. The other cases exercise
`validate_header` and `ensure_exact_records` directly. Nine deliberately
incorrect variants check sensitivity to changes in validation, comparison
order, path sort and truncation.

A saved fingerprint input named `one\ntwo.txt` with a malformed digest
produced a multiline manifest diagnostic. The path is interpolated
directly in that diagnostic. The stale-record diagnostic for an
otherwise valid saved path escapes the newline and stays on one line.

All 201 cases matched on macOS. The plan unit and integration suites
passed 527 and 292 tests respectively, with four existing ignored
tests. The CI workflow suite passed 222 tests. Lean ran under a 2 GiB
cap with a 20-second timeout per command. Single-job Rust peaked at
2,254,384 KiB under a 3 GiB cap.

```sh
(cd formal/HoiminOracle && lake exe generate_plan_records -- --check corpus/plan-records.jsonl)
(cd formal/HoiminOracle && lake exe generate_plan_records -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --lib -- --test-threads=2 plan::
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --test plan -- --test-threads=2
```

## Lean model of saved plan candidate verification

The verification stage visits distinct requested IDs in ascending ID
order. It requires a saved descriptor and a selected file, builds a
source validation context once per requested file, and reports descriptor
or ID failures in request-ID order even when file results are cached.
Rediscovery preserves the target slice order and keeps the slices for the
requested files. It then checks that each requested ID has a rediscovered
descriptor equal to the saved one. The successful result retains the
requested IDs in discovery order. The model takes digest and descriptor
validation outcomes and per-slice discovery results as inputs.

Lean proves that a stored-descriptor failure prevents confirmation, that
the projection of target slices preserves order, and that a successful
confirmation is exactly the discovery projection and has the same
membership as the requested IDs. The fixtures use nonempty requests.

The 55 cases include valid and corrupt stored descriptors, missing and
unselected files, conflicting failures, symbol-only differences, and
differing ID and discovery orders. The adapter uses actual Python files
and obtains stable IDs from the production planner. The three selected
IDs alternate between two files. Fifty cases exercise the production
verification stage and compare the number of file validation contexts.
Five serialize a plan and call the public verification preview. The
cases distinguish the accepted candidates and the full rejection
diagnostic. Eight deliberately incorrect models vary failure priority,
cache reuse, rediscovery checks, or output projection.

On macOS, all 55 cases matched. The Rust plan suites passed 528 unit
tests and 292 integration tests, with four existing ignored tests.
The workflow suite passed 222 tests. The Lean commands ran under a
2 GiB cap with a 20-second timeout per command. Single-job Rust peaked
at 2,059,120 KiB under its 3 GiB cap.

```sh
(cd formal/HoiminOracle && lake exe generate_plan_verify -- --check corpus/plan-verify.jsonl)
(cd formal/HoiminOracle && lake exe generate_plan_verify -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --lib -- --test-threads=2 plan::
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --test plan -- --test-threads=2
```

## Lean model of bounded plan rediscovery

The bounded candidate discovery model retains a prefix of the complete
ordered candidate stream. Verification projects the original target
slices onto requested files, preserving their order, and gives those
slices the saved candidate limit. Lean proves that the kept part of
the original prefix remains a prefix of rediscovery when each
retained file supplies the same ordered candidates. Every saved
candidate in a retained file therefore remains visible. Removing
files cannot introduce truncation, and an untruncated original has
exactly its kept projection after rediscovery.

The 143 cases use six file layouts and candidate limits from one through
one above the complete candidate count. They cover empty files at the
limit, overflow first observed in a later file, and requests for
individual candidates and groups of files. The Python fixtures put a
binary addition mutation outside the saved line range in each file.
The adapter runs the public planner, then uses the production target
projection and discovery functions. It compares candidate positions,
operators, sequence numbers, truncation, the diagnostic file and plan
exit status. The 142 nonempty requests also exercise the public verify
preview and compare returned stable IDs in discovery order.

The corpus has 68 truncated plans. In ten cases, the restricted discovery
admits more candidates than the kept portion of the original prefix
because earlier files release capacity. The preview retains the requested
IDs. Five deliberate variants exercise target projection, capacity reuse,
rediscovery after projection, target order, and the exact limit.

All 143 cases matched on macOS. The Rust plan suites passed 529 unit
tests and 292 integration tests, with four existing ignored tests.
The workflow suite passed 222 tests. Lean peaked at 1,261,120 KiB
under a 2 GiB cap. Single-job Rust peaked at 2,203,536 KiB under a
3 GiB cap.

```sh
(cd formal/HoiminOracle && lake exe generate_plan_discovery_budget -- --check corpus/plan-discovery-budget.jsonl)
(cd formal/HoiminOracle && lake exe generate_plan_discovery_budget -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --lib -- --test-threads=2 plan::
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --test plan -- --test-threads=2
```

## Lean model of verify selection from preview to execution

Explicit IDs select membership and retain discovery order. Ranked pages
and seeded samples carry an order into both the preview and execution.
The model reuses the line and file round-robin order and the seeded
sampling algorithm. It treats the ranked plan and source validation
as prior inputs and assumes a discovery stream with unique IDs.

Lean proves that explicit orders agree when preview and execution see
the same selected discovery projection. For ranked pages and samples,
it proves agreement once discovery accounts for each selected ID and
that an incomplete ordered discovery cannot release the dispatch
order. It also proves priority for the malformed explicit ID, the
out-of-range offset and the truncated-plan sample rejection.

The 32 cases use two small Python files, one worker and a successful
baseline for accepted requests. They vary ordering policy, offset, seed,
duplicate and unknown IDs, and budgets, with complete, truncated and
empty plan fixtures. One empty manifest also carries a truncated flag
to exercise competing sample rejections. The adapter compares full
preview rows, the IDs in `mutant_started`, selection provenance in
`run_started` and `run_finished`, and exit status. The marker records
whether the test command ran, and the saved plan stays byte-identical.
Six altered models exercise budget checks, sample admission and seed,
paging, and the two sources of ordering.

The legacy truncated-preview test wrote its baseline marker inside the
original project and expected exit 2. A local reproduction moving only
the marker confirmed that the in-project write triggers
`workspace.original.changed` and exit 2. A marker outside the project
leaves the candidate limit as the sole warning and gives exit 4,
as documented for a truncated run. The test now uses the outside marker.

On macOS, the 21 accepted cases and 11 rejections matched for both
public commands. The plan and sampling suites passed 293 and 48 tests
respectively, with two existing ignored tests. The workflow suite
passed 222 tests. Lean ran under a 2 GiB cap, and single-job Rust ran
under a 3 GiB cap.

```sh
(cd formal/HoiminOracle && lake exe generate_verify_selection_flow -- --check corpus/verify-selection-flow.jsonl)
(cd formal/HoiminOracle && lake exe generate_verify_selection_flow -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --test plan --test sampling -- --test-threads=2
```

## Lean model of original workspace integrity

The workspace checker compares the selected directory set and selected
file contents against the preflight snapshot. A directory difference
takes priority, followed by the first changed or removed file from the
saved manifest, then the first newly added file. Each traversal follows
path-component order. The model uses exact content equality for the
small fixture files, and the implementation compares size and digest.

Lean proves that writes to the excluded fixture path leave the selected
snapshot unchanged. It proves that a change detected before the baseline
prevents execution, and that a change detected before the final report
gives exit 2 while preserving the observed baseline exit or finished
mutant status. The two comparisons observe snapshots at their
checkpoints. A change restored before the final comparison may leave
the original snapshot equal.

The 58 cases comprise 45 real-file comparisons through workspace
preflight and original verification and 13 public verify runs with one
worker and one candidate. The test command in the latter group applies
the requested edits to the original tree during the baseline or mutant.
The oracle checks the reported path, baseline exit, mutant status,
summary exit and touched file contents. The case matrix includes
ignored files and directory trees, same-size edits, replacements with
the same bytes, empty directories, a failing baseline and a transient
edit restored before final verification. Six deliberately incorrect
variants check the diagnostic priority, comparison checkpoints,
outcome preservation and exit precedence.

All 58 cases matched on macOS. The plan, workspace handler and empty
directory suites passed 294, 60 and 16 tests respectively, with one
existing ignored test. The workflow suite passed 222 tests. Lean
peaked at 1,282,224 KiB under a 2 GiB cap. The Rust runs used one build
job and a 3 GiB cap.

```sh
(cd formal/HoiminOracle && lake exe generate_workspace_integrity -- --check corpus/workspace-integrity.jsonl)
(cd formal/HoiminOracle && lake exe generate_workspace_integrity -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --test plan --test workspace_handler --test workspace_empty_directories -- --test-threads=2
```

## Lean model of worker workspace restoration

Reset removes added entries and entries of the wrong kind, recreates
missing snapshot directories, and then restores snapshot files in
path order. The model takes the directory phase as completed before
any injected restoration fault. Lean proves that a complete file pass
produces the saved state for every slot and replaces exactly the files
that differ. A matching file pass performs no replacements, even when
a write fault is armed.

The 60 cases exercise the real `WorkerWorkspace::reset` over files and
directories. They check the inventory, file bytes and read-only state,
and on Unix the identity of existing files and removal of symlinks
without changes to the external target. The tests change the original
after taking the snapshot and check that reset uses the snapshot and
leaves the changed original intact.

The injected faults cover rewind before replacement, the first data
write after replacement, and permission setting after all bytes are
written. They leave the old file, an empty replacement, and a
replacement with saved bytes and writable permissions, respectively.
Earlier completed work stays in place during retry. Seven deliberately
incorrect variants exercise the comparison and interruption rules.

On macOS, all 60 cases matched. The reset, workspace handler and
workspace recovery suites passed 28, 60 and 39 tests respectively,
with one existing ignored reset test. The reset suite with contracts
enabled passed 30 tests, with the same ignored test. The workflow suite
passed 222 tests. Lean ran under a 2 GiB cap and single-job Rust ran
under a 3 GiB cap.

```sh
(cd formal/HoiminOracle && lake exe generate_workspace_restore -- --check corpus/workspace-restore.jsonl)
(cd formal/HoiminOracle && lake exe generate_workspace_restore -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --lib workspace::reset -- --test-threads=2
```

## Lean model of worker recovery after failure

The model starts with one active worker and a bound copy reservation.
A failed reset releases the worker if discard succeeds. A failed
discard keeps the worker pending and keeps its copy charge. A foreign
grant is rejected before cleanup of any pending worker. An authorized
retry cleans up the old worker before copying a replacement. A failed
copy releases the new charge, while rejection of a copied import root
leaves the new worker pending for cleanup. The model covers one slot
with sequential requests and observes the owned task before execution,
after execution, and after acceptance.

| Premise or observation | Lean | Rust fixture and evidence |
| --- | --- | --- |
| Reset and discard failures | Missing snapshot, blocked discard, or excessive depth | Remove the saved file, arm the directory enumeration fault, or extend the tree one level past the limit |
| Retry admission or materialization failure | Foreign grant, blocked cleanup, failed copy write, or rejected import root | Use a grant from a second preflight, arm cleanup or copy faults, or configure an absent import root |
| Ownership and copy charge | Registered owner, live generation, slot count and charged bytes | Observe both handler entry points, including the registry and live roots at each owned task boundary |
| Diagnostics | Effect code and ordered failure stages | Compare the returned effect and the positions of its diagnostic markers |

All schedules use `internal-fixture` correspondence. The fault hooks
run at the production I/O boundaries. Fixtures repair the snapshot or
remove the over-depth leaf after observing the outcome, so subsequent
operations exercise the retry protocol. The larger workspace lifecycle
model covers scheduling across slots and task completions.

Lean proves that any finite trace preserves the generation order, a
successful retry has a fresh owner, and one owned slot carries at most
one logical copy charge. It also proves the retained and released
charges for a failed discard and a failed replacement respectively.
The 48 schedules contain 100 transitions from ten operations. They
cover all applicable schedules of lengths one and two from the initial
state and five recovery schedules of length four. Seven deliberately
incorrect variants have witnesses, including `discard_fault/copy_fault`
for retaining the old charge and `discard_fault/foreign_with_cleanup_fault`
for cleanup before grant validation.

On macOS all 48 schedules matched on both the synchronous and the
owned task paths. The task suite passed 26 tests with and without
contracts enabled. The workspace handler and workspace recovery suites
passed 60 and 39 tests respectively. The workflow suite passed 222
tests. Lean ran under a 2 GiB limit and single-job Rust ran under a
3 GiB limit.

```sh
(cd formal/HoiminOracle && lake exe generate_worker_recovery -- --check corpus/worker-recovery.jsonl)
(cd formal/HoiminOracle && lake exe generate_worker_recovery -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --lib workspace::task_tests -- --test-threads=2
```

## Lean model of workspace capacity admission

Preflight admits equality at the worker-copy allowance. The owned
workspace limit also includes the shared snapshot and rejects equality.
Worker multiplication and the copy allowance are checked before the
owned calculation. The owned calculation must fit in a `u64` even when
the configured owned limit is absent. The arithmetic model proves the
accepted bounds, copy-limit precedence and both equality rules.

The snapshot checks the metadata inventory before allocation when a
capacity limit is configured. Before opening a snapshot output for a
file, it checks the new cumulative size using that file's metadata.
Each subsequent chunk passes the guard before reaching the output.
Lean proves the written bytes are a prefix of the input chunks and that
any nonempty written prefix passed admission. A finite reader observes
its opened descriptor's length and reports growth or shrinkage after
that observation. Validation is followed by comparison with the original
before publication. Repeated visits to a stable included path count once.

| Premise or observation | Lean | Rust fixture and evidence |
| --- | --- | --- |
| Integer bounds and limits | 72 arithmetic cases with zero, equality and checked multiplication boundaries | Call `PreflightLimits::check` and compare typed errors, amounts and accepted aggregates |
| Source size changes | 30 filesystem cases with an edit at one of four checkpoints | Use existing hooks around snapshot allocation and descriptor length, plus the validation callback |
| Progress and cleanup | Snapshot start, write count, bytes written, validation and final admission | Observe the existing snapshot write counters, snapshot bytes, returned plan and managed child inventory |

The corpus cases use `internal-fixture` correspondence. Live files
range from zero bytes to 65,538 bytes. The two-file cases use equal
file lengths so directory enumeration order cannot change the expected
prefix. The large integer cases run directly against the arithmetic
check without allocating large files. Nine deliberately incorrect
variants cover admission boundaries, skipped checks, duplicate charging,
unbounded reads and publication after original changes.

All 102 cases matched on macOS. The successful managed snapshot remains
owned by its run root after the plan is dropped. The fixture checks
failed preflight cleanup before run cleanup and verifies that run
cleanup also removes the successful snapshot. The copy suite passed
96 tests in the default build and 97 with contracts enabled, with ten
existing ignored cases in each configuration. The workspace handler,
preflight heap and recovery suites passed 100 tests, and the workflow
suite passed 222. Lean used a 2 GiB limit and single-job Rust used 3 GiB.

```sh
(cd formal/HoiminOracle && lake exe generate_workspace_capacity -- --check corpus/workspace-capacity.jsonl)
(cd formal/HoiminOracle && lake exe generate_workspace_capacity -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --lib workspace::copy -- --test-threads=2
```

## Lean model of worker reservation publication

The first valid create request binds the plan to one reservation and
publishes the copy allowance while the plan mutex is held. A concurrent
request can proceed only after publication. After release of the lock,
the second request may use a different slot on the same reservation.
Invalid preflight IDs, worker indexes and allowances take precedence
over reservation mismatch and slot duplication. A failed copy rolls
back its own slot and charge. Explicit cleanup releases a ready worker's
slot when its workspace is dropped. Dropping a managed workspace with
its root still present retains the slot and charge until run cleanup.

Lean proves that any finite history preserves the absence of visible
workers before publication, and that charged bytes never exceed the
published allowance under that invariant. It also proves that a bound
reservation stays bound, either copy failure preserves the sibling slot,
and retaining a managed worker preserves its charge. The model observes
two slots and one contender during the first binding. Its stable copy
checkpoints cover a held lock, a copy paused after charging, and a
completed copy or cleanup.

| Premise or observation | Lean | Rust fixture and evidence |
| --- | --- | --- |
| Contender during publication | One waiting request while the binding is held | Start a second thread, observe `try_lock` report contention, and execute `WorkspacePlan::create_worker` |
| Reservation, slot and allowance | Published reservation, registered slots, charged and granted bytes | Read the real plan state and allowance at the synchronized checkpoints |
| Failure, retry and cleanup | Copy outcome, live roots and bytes on disk | Pause at the real write or permission boundary, inject I/O failure, inspect managed roots, and run explicit cleanup |

The 80 `internal-fixture` histories contain 724 transitions and run up
to 11 operations. They cover empty copies, a partial 64 KiB copy with a
ready sibling, a dropped managed worker, conflicts before and after all
workers are gone, and both worker orders. Seven deliberately incorrect
transitions have witnesses. The two threads use channel handshakes for
the checkpoints, with timeouts for missing responses.

All 80 histories matched on macOS. The copy suite passed 97 tests in the
default build and 98 with contracts enabled, with ten existing ignored
tests in each configuration. The workspace handler and recovery suites
passed 99 tests, and the workflow suite passed 222. Lean used a 2 GiB
limit and single-job Rust used a 3 GiB limit.

```sh
(cd formal/HoiminOracle && lake exe generate_worker_grant_publication -- --check corpus/worker-grant-publication.jsonl)
(cd formal/HoiminOracle && lake exe generate_worker_grant_publication -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --lib worker_grant_publication_matches_lean_oracle -- --test-threads=1
```

## Lean model of mutation application in a worker

The worker first checks that the path belongs to the snapshot and opens
the worker file. It checks the expected length, reads with that length
as a budget, and compares both the candidate hash and the worker file
against the snapshot. Candidate validation then checks the original
bytes, source encoding, location and stable ID before the worker asks
for write access. A hard link to another file name is rejected before
permission repair. A failure at the final link check after permission
repair can leave the file writable with its bytes intact.

Lean proves prefix and suffix preservation for the raw byte splice,
the descriptor length plus one byte bound on reads, file preservation
after validation rejection or hard-link rejection, and byte preservation
at the final link check. The small fixtures model digest equality with
exact bytes and use Python source encodings fixed by each fixture. The
I/O fault cases cover allocation and the two link checks before writes.

| Premise or observation | Lean | Rust fixture and evidence |
| --- | --- | --- |
| Live file and snapshot | Selected path, missing file, bytes and read-only state | Create a real preflight plan and worker, then seed the target or a hard link to an external fixture |
| Candidate and encoding | Raw span, original, replacement, location, hash and ID match | Apply the candidate to UTF-8, BOM, ASCII, Latin-1 and invalid UTF-8 fixtures |
| Read and write preparation | Resize after length capture and faults before or after permission repair | Reuse the existing read observation and I/O fault hooks and compare bytes read, permissions and typed errors |
| Retry and reset | Retry from the first outcome with transient faults removed | Apply the same candidate again, reset, and check the original and external referent |

The 111 cases use `internal-fixture` correspondence. One read-only
hard-link case runs on Unix. Each applicable fixture runs two
applications and a reset. Eight deliberately incorrect variants check
authority and validation order, encoding, permissions and the finite
read budget.

All 111 cases matched on macOS. The mutation suite passed 28 tests
with and without contracts, with one existing ignored test. The
candidate-span, workspace handler and recovery suites passed 132 tests,
and the workflow suite passed 222. Lean used a 2 GiB limit and single-job
Rust used a 3 GiB limit.

```sh
(cd formal/HoiminOracle && lake exe generate_workspace_mutation -- --check corpus/workspace-mutation.jsonl)
(cd formal/HoiminOracle && lake exe generate_workspace_mutation -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --lib workspace::mutation -- --test-threads=2
```

## Lean model of interrupted worker mutation writes

The mutation writer truncates the worker file before writing the new
bytes. A file-size-limit error can therefore leave an empty file or a
prefix of the mutation. The worker handler keeps the same active worker
and its original copy charge after an apply error. An unlimited retry
requires the worker file to match the snapshot. An interrupted append
can leave that exact snapshot, while a same-length prefix with changed
bytes fails the hash check. Reset skips a file that already matches in
both bytes and permissions. If the reset itself hits the file-size limit
and discard succeeds, the handler releases the old worker and its copy
charge. A new worker can use the same reservation.

Lean proves the write bound, the exact prefix on a short write, worker
generation and charge preservation during apply, the byte-equality
condition for retry, and charge release on a failed restore. The model
covers the file-size-limit outcome with valid candidates in one worker
slot. It reuses the raw splice from the mutation application model.

| Premise or observation | Lean | Rust fixture and evidence |
| --- | --- | --- |
| Limit during apply or reset | Optional write cap and a bounded byte prefix | Run the real request under `RLIMIT_FSIZE` in an isolated Unix test process and match `EFBIG` diagnostics |
| Existing or recreated worker | Active generation, saved copy charge and live root | Observe the registry, physical roots and accounting, including before execution and before accepting owned tasks |
| Retry and restoration | Full byte equality, exact snapshot permissions and fresh worker after discard | Replay the same candidate, reset, or create again through the handler and check the files on disk |

The 144 `internal-fixture` histories cover limits from zero through one
beyond the greater of the source and mutated file lengths, plus unlimited
writes, for five mutations and both initial permission states. Six targeted
histories add capped resets, discard and recreation. The 580 transitions
are replayed through synchronous and owned task paths. Six deliberately
incorrect transitions exercise the partial prefix, retry, ownership, and
reset rules. The same child-process limit helper is used by snapshot and
worker copy coverage tests.

All 288 replays and 1,160 transitions matched on macOS. The task suite
passed 27 tests in both configurations, with one ignored child fixture.
The copy suite passed 97 tests with ten existing ignored tests, the
workspace handler and recovery suites passed 99, and the workflow suite
passed 222. Lean used a 2 GiB limit and single-job Rust used a 3 GiB limit.

```sh
(cd formal/HoiminOracle && lake exe generate_workspace_mutation_write -- --check corpus/workspace-mutation-write.jsonl)
(cd formal/HoiminOracle && lake exe generate_workspace_mutation_write -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --lib mutation_write_limit_matches_lean -- --test-threads=1
```

## Lean model of worker command environment

The worker root becomes the command's working directory and the first
Python search path. Explicit import roots follow in request order, then
source roots, then inherited `PYTHONPATH` entries. Source and inherited
paths are resolved against the original project, using canonical filesystem
paths where possible and lexical normalization otherwise. Paths inside the
resolved project move under the worker root. External paths keep their
resolved locations, including directories whose names share a prefix with
the project name. Duplicate paths keep the first spelling and position.

Lean proves that the emitted paths form a subsequence of the composed
input order with distinct normalized keys, that the worker root comes
first, and that external paths preserve their resolved locations. The
bounded fixtures use an explicit filesystem tree with Unix symbolic links.
The Windows cases model ASCII case folding and ordered merging of
case variants of the `PYTHONPATH` variable.

| Premise or observation | Lean | Rust fixture and evidence |
| --- | --- | --- |
| Search order and duplicate paths | First occurrence of each platform-normalized key | Call both public environment builders and compare the complete split search path and working directory |
| Canonicalization and containment | Existing directories, missing paths and symbolic links in a finite tree | Create the same layout in a real temporary directory |
| Native encoding and variables | Platform join error and retained environment names | Check native split/join results and unchanged values of all other environment entries |
| Module resolution | First search directory containing the test module | Start Python with the returned environment and check the imported module's file |

The generated corpus contains 146 `strict` cases, including 76 for Unix
and 70 for Windows. Each platform has four import probes. Six deliberately
incorrect variants exercise priority, canonicalization, containment,
deduplication, Windows case handling and native join errors.

All 76 Unix cases matched on macOS in both build configurations. The four
Python probes loaded the expected files. The workspace handler, import
root and environment fingerprint suites passed 124 tests, and the
workflow suite passed 222. The Windows cases were generated and checked
for freshness. Rust correspondence for those cases requires a Windows
host. Lean used a 2 GiB limit and single-job Rust used a 3 GiB limit.

```sh
(cd formal/HoiminOracle && lake exe generate_worker_environment -- --check corpus/worker-environment.jsonl)
(cd formal/HoiminOracle && lake exe generate_worker_environment -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --test lean_worker_environment_oracle -- --test-threads=1
```

## Lean model of workspace copy selection

The ordinary walk honors the source's ignore rules. The explicit include
walk can restore ignored files and empty directories. Both walks apply
the built-in names, literal exclusions and explicit glob exclusions. A
literal file exclusion prunes the exact file or link. A literal tree
exclusion prunes a directory and its descendants and preserves a regular
file with the same name. A chosen file brings its parent directories
into the manifest. A symlink contributes a skip diagnostic.

Lean proves the exclusion precedence, the file and tree distinction, the
presence of selected file ancestors, and that each output file comes
from a selected regular source file. The bounded pattern grammar covers
rooted paths, rooted descendants, basename suffixes and the broad
include. The cases also check typed rejection of malformed patterns and
literal paths. The model uses platform case rules for explicit globs,
literal exclusions and built-in names.

| Premise or observation | Lean | Rust fixture and evidence |
| --- | --- | --- |
| Selection and exclusions | Files, directories, link diagnostics and logical byte totals | Build a real preflight manifest from the same files, `.gitignore` and options |
| Copy membership and inventory | Requested intersection and unique byte total | Check all paths and a subset through the internal membership API and confirm zero content reads |
| Worker materialization | Exactly the manifest's files and directories | Materialize one worker under a two-worker grant, enumerate its tree and check content and accounting |

The 237 `internal-fixture` cases include 121 for Unix and 116 for
Windows. The five Unix link cases include file, directory and dangling
links, as well as links restored through explicit inclusion. Seven
deliberately incorrect rules check ignore restoration, hard and glob
exclusions, literal kinds and Windows case handling.

All 121 Unix cases matched on macOS in both build configurations. The
manifest suite passed 14 tests in each configuration. Workspace, empty
directory and fingerprint input suites passed 132 tests, and the
workflow suite passed 222. The Windows corpus passed generation and
freshness checks. Implementation correspondence requires a Windows
host. Lean used a 2 GiB limit and single-job Rust used a 3 GiB limit.

```sh
(cd formal/HoiminOracle && lake exe generate_workspace_selection -- --check corpus/workspace-selection.jsonl)
(cd formal/HoiminOracle && lake exe generate_workspace_selection -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --lib workspace_selection_matches_lean_oracle -- --test-threads=1
```

## Lean model of saved-plan fingerprint preflight

Verification records which fingerprint inputs were selected for workspace
copying. During preflight, the live input records must equal the verified
records. Each matching input in the pending snapshot must have verified
content, and every input selected for copying at preparation must still
be present there. An input excluded from copying can be absent from the
snapshot. The pending tree must contain every target source and agree
with a fresh manifest of the copy scope before publication.

Lean proves these publication conditions and the precedence of live
resolution failures and live record drift over snapshot failures. The
finite fixture uses one glob, one exact config path, one optionally
protected source, and one unprotected note. It models regular file
contents as strings and compares exact bytes. Rust compares BLAKE3
records and inspects the published worker files.

The 79 cases create and verify a saved plan through the ordinary plan
APIs. They apply file edits before copying, before validation, and after
validation. The adapter calls the preflight handler and the shell's
fingerprint helpers in execution order. It compares the prepared input
records, selected copy membership, pending manifest, and typed outcome.
It also checks worker contents on acceptance and verifies that rejected
preflight leaves no plan. Six sensitivity checks cover the live and
snapshot comparisons, membership in the pending copy, the permissive
handling of excluded inputs, source presence, and final manifest
comparison. The fixture covers explicit selectors and copy exclusions.
This includes changes that are reverted before the live check. A
previously copied input that is changed or missing in the pending
snapshot still causes rejection after restoration.

All 79 cases matched on macOS in both Rust build configurations. The
Lean library, corpus freshness, six sensitivity checks, and all 222
workflow tests passed. Lean used a 2 GiB limit and single-job Rust used
a 3 GiB limit.

```sh
(cd formal/HoiminOracle && lake exe generate_fingerprint_preflight -- --check corpus/fingerprint-preflight.jsonl)
(cd formal/HoiminOracle && lake exe generate_fingerprint_preflight -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --lib fingerprint_preflight_matches_lean_oracle -- --test-threads=1
```

## Lean model of session artifact exclusions

Preflight reserves the resolved session database and its SQLite
companions as exact file exclusions. It reserves the lexical lock
directory and any distinct canonical lock directory as tree exclusions.
Each path is projected into the project independently. A symlink from
an external session into the project can therefore exclude an in-project
lock tree. Component comparison keeps a sibling project with a similar
name outside the copy scope.

Lean proves that an in-project database file or companion is reserved
without an existence check, that an absent lock tree still reserves its
lexical name, and that a canonical lock tree inside the project is
reserved even when the lexical lock is outside. A proper descendant of
a reserved tree remains excluded when explicit includes are enabled.
The model takes filesystem resolution as an input and reuses the
workspace selection model for the effect of the resulting exclusions.

| Observation | Lean | Rust fixture |
| --- | --- | --- |
| Session preparation | Resolved database path, exact exclusions, tree exclusions or refusal | Create finite directory and symlink layouts and call the shell preparation helper |
| Files created after preparation | Exclusions apply to newly present artifacts | Create the files and lock directory before constructing the snapshot |
| Copy contents | Files, directories and skipped links | Run actual workspace preflight and materialize a worker |
| Read-only preparation | Test premise | Compare both project and sibling trees immediately before and after the helper |

Cases cover existing and missing artifacts, literal metacharacters,
similar names, file and directory type boundaries, and broad includes.
Unix cases additionally exercise symlink resolution, aliases across the
project boundary, dangling paths, and loops. An alias to the project root
produces an empty relative exclusion and fails workspace validation. The
adapter stops at worker materialization. Session opening and ownership
acquisition have separate tests.

The corpus has 31 Unix cases and 22 Windows cases. Six broken variants
test missing companions, missing and canonical lock reservations,
resolution-error propagation, scope across siblings, and the effect of
removing protection from the copy itself. The Windows cases cover
portable layouts and case-insensitive path matching. The Unix cases
include the native symlink fixtures.

All 31 Unix cases matched on macOS in the default and `contracts`
configurations. The 23 active-session CLI tests, three session lock-tree
tests, and 222 CI workflow tests passed. The Windows corpus passed
generation and freshness checks. Windows implementation correspondence
requires a Windows host. Lean ran under a 2 GiB limit and single-job
Rust ran under a 3 GiB limit.

```sh
(cd formal/HoiminOracle && lake exe generate_session_artifacts -- --check corpus/session-artifacts.jsonl)
(cd formal/HoiminOracle && lake exe generate_session_artifacts -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --lib session_artifacts_match_lean_oracle -- --test-threads=1
```

## Lean model of SQLite session admission

SQLite session admission inspects the journal, WAL, and shared-memory
files in that order before checking the database. A missing companion
or a regular single-link companion can pass. A directory or symlink
is rejected as a nonregular companion. A shared database or companion
is rejected before inspecting its SQLite schema. For a valid SQLite
image, an unsupported version takes precedence over application ID and
schema mismatches. A supported image must have application ID zero and
either an empty version-zero schema or the canonical current schema.

Lean proves that the first rejected companion determines the outcome,
that a companion failure takes precedence over a database failure, and
that every accepted SQLite image satisfies the version and identity
conditions. The 98 cases include pairs of competing failures as well as
the schema and version combinations for closed rollback and WAL files.
Seven broken variants exercise the modeled precedence and admission
conditions.

The adapter seeds actual SQLite files and calls the Rust schema entry
point. It compares the rejection variant, including the companion name
or version when available. It checks names, contents, permissions,
links, and directory entries before and after rejection. For acceptance,
it compares the opened schema, version, journal mode, foreign-key flag,
and preexisting fingerprint row. This fixture covers stable file layouts
and closed SQLite images. Concurrent initialization is covered in the
schema migration audit.

All 98 cases matched on macOS in the default and `contracts`
configurations, with 9 accepted and 89 rejected. The schema suite
passed 84 tests including this adapter. The `session_hardlinks` target
passed 31 tests and the workflow suite passed 222. Twelve symlink
cases require Unix. The remaining cases describe portable layouts
and were executed on macOS. Lean used a 2 GiB limit and single-job
Rust used a 3 GiB limit.

```sh
(cd formal/HoiminOracle && lake exe generate_session_admission -- --check corpus/session-admission.jsonl)
(cd formal/HoiminOracle && lake exe generate_session_admission -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --lib session_admission_matches_lean_oracle -- --test-threads=1
```

## Lean audit of session admission with uncheckpointed WAL

The model tracks the checkpointed database image and the latest
committed image independently. A commit can advance the latter while
leaving the former unchanged. The opener's advisory check uses the
checkpointed image. If that check passes, admission on the writable
connection uses the committed image. Lean proves that successful open
requires an admissible committed image, and that checkpointing aligns
the opener's decision with admission on that same committed image.

The fixture runs 12 schema, version, and application-ID transitions
twice, with the writer kept open and automatic checkpoints disabled.
One replay leaves the commit in WAL. The other checkpoints it before
admission. It compares the images read by ordinary read-only and
immutable connections and calls the production opener. On rejection it
compares the main database and WAL bytes. SHM may reflect reader locks
and is outside the byte comparison. The fixture holds the committed
image stable during each open.

### Counterexample ledger

| Stable committed transition, left in WAL | Decision on the latest committed view | Production opener |
| --- | --- | --- |
| Canonical schema version 99 to current version 5 | Admit | Reject with unsupported version 99 |
| Canonical schema version 99 to unsupported version -1 | Reject with unsupported version -1 | Reject with unsupported version 99 |
| Canonical schema tagged as version 99 to version 0 | Reject with invalid identity | Reject with unsupported version 99 |
| Current schema application ID 173 to 0 | Admit | Reject with invalid identity |
| Empty version-zero schema application ID 173 to 0 | Admit and initialize | Reject with invalid identity |
| Drop the sole foreign table from a version-zero database | Admit and initialize | Reject with invalid identity |

Each discrepancy disappears after checkpointing without a further
commit. The shortest version witness changes only `user_version` on a
canonical schema. The WAL-aware reader sees 5, while the advisory
`immutable` reader sees 99 in the main file. The opener rejects before
its writable admission check. The reverse witness, from version 5 to
99, shows that writable admission rejects a committed unsupported
version even when the advisory check passes. These are observed
admission and diagnostic discrepancies relative to the latest
committed image. The audit preserves the production behavior and
records the expected committed-view decision for each witness.

All 24 cases matched the modeled opener behavior on macOS in both
the default and `contracts` configurations. Each discrepant case and
its checkpointed counterpart was also replayed individually. The
85-test schema suite, the three Lean sensitivity checks, and 222 CI
workflow tests passed. Lean used a 2 GiB limit and single-job Rust
used a 3 GiB limit.

```sh
(cd formal/HoiminOracle && lake exe generate_session_wal_admission -- --check corpus/session-wal-admission.jsonl)
(cd formal/HoiminOracle && lake exe generate_session_wal_admission -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --lib session_wal_admission_matches_lean_oracle -- --test-threads=1
HOIMIN_SESSION_WAL_ORACLE_CASE=upgrade-committed-version-in-wal CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --lib session_wal_admission_matches_lean_oracle -- --nocapture
```

## Lean model of WAL enablement retries

The first SQLite request runs even with a zero retry timeout. A reply
of `wal` or `memory` succeeds with ASCII case-insensitive matching.
Other journal modes are returned in the refusal. A busy or locked
SQLite error can lead to another request only if the first clock
sample following the error is strictly before the deadline. Other
errors terminate immediately.

Lean proves the eligibility condition and the two bounds on each
requested sleep: five milliseconds and the time remaining at the
second clock sample. If the second sample has crossed the deadline,
the requested sleep is zero. Once a retry is eligible, the opener
runs the next request after the sleep. That request can start at or
after the deadline. Successful mode replies still succeed then, and
an expired contention error is returned with its original extended
code. The deadline belongs to the retry loop. SQLite's busy timeout
and blocking duration apply independently inside each request.

The 85 `internal-fixture` cases exercise at most four requests per
trace. They compare exact query start times, sleep durations, and the
final mode or error. Fake time drives the production loop while
rusqlite's own error representation supplies primary and extended
result codes. The production entry point uses a monotonic clock, the
real SQLite query and thread sleep. A separate fixture starts with a
real exclusive SQLite transaction, commits the peer during the
first retry wait, and checks the committed data and WAL mode after
the retry. The existing real-database test covers expiration while
the peer keeps its transaction.

All 85 cases matched on macOS in both the default and `contracts`
configurations. The 87-test schema suite passed in each configuration.
The session handler and hard-link targets passed 133 tests with one
existing ignored test, and the workflow suite passed 222. Six
sensitivity checks distinguish deadline equality, sleep clamping,
clock resampling, permanent faults, extended contention errors, and
memory mode. Lean ran under a 2 GiB limit and single-job Rust under
a 3 GiB limit.

```sh
(cd formal/HoiminOracle && lake exe generate_session_wal_retry -- --check corpus/session-wal-retry.jsonl)
(cd formal/HoiminOracle && lake exe generate_session_wal_retry -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --lib session::schema:: -- --test-threads=1
```

## Lean model of immutable SQLite URI construction

The session opener constructs a `file:` URI by encoding each native
pathname byte as an uppercase percent triplet and appending the fixed
`?immutable=1` option. Lean proves that parsing the generated text
recovers the original bytes for arbitrary input length and that equal
URIs imply equal input bytes. The encoded pathname contains ASCII
characters and no raw question mark, fragment marker, or slash.
The modeled parser covers the generated URI form.

The 22 cases compare the Rust encoding with the generated text. The
codec cases include all 256 byte values, embedded NUL, malformed UTF-8,
and lexical Unix, drive, UNC, and verbatim path spellings. Valid
pathname fixtures additionally check the bundled SQLite reader. Each
fixture creates a database at the intended location and two neighbors
with different rows, opens the production URI read-only, and reads the
intended row. It also calls `SessionHandler::open` on the same path and
checks that rejecting a foreign schema preserves the directory's names
and file contents. The adapter probes the environment's ability to
create a non-UTF-8 pathname before attempting the URI read.

Seven sensitivity checks cover raw URI delimiters, percent handling,
slash handling, nibble order, replacement of high bytes, and loss of
the fixed option. The Windows-only native fixture checks that a
pathname with an unpaired UTF-16 surrogate returns `InvalidPath`.

On macOS, all 22 codec cases and 10 SQLite file cases matched in
both the default and `contracts` configurations. Direct SQLite
open could not create the non-UTF-8 fixture pathname in this
environment. The encoding of that pathname was checked. The 88-test
schema suite, 23 active-session CLI tests, and 222 workflow tests
passed. Native Windows runtime cases require a Windows host. Lean
used a 2 GiB limit and single-job Rust used a 3 GiB limit.

```sh
(cd formal/HoiminOracle && lake exe generate_session_uri -- --check corpus/session-uri.jsonl)
(cd formal/HoiminOracle && lake exe generate_session_uri -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --lib session_immutable_uri -- --test-threads=1 --nocapture
```

## Lean model of SQLite session header routing

The session header reader consumes at most 20 bytes. It chooses the
immutable advisory reader when the first 16 bytes match the SQLite
magic including its terminating NUL, and both format bytes at offsets
18 and 19 equal 2. Short files and incomplete reads ending in EOF use
the ordinary reader. Interrupted reads retain the partial header and
continue. Other I/O errors preserve their original kind and message.

Lean proves the magic and version requirements, the independence
from the page-size field and trailing bytes, and the interruption,
EOF, and failure behavior of the reading loop. This model describes
the routing decision. SQLite evaluates schema and database validity
after the decision.

The Rust adapter drives the production header helper with controlled
read events. It compares the route or error and every requested
buffer length. The regular-file cases also exercise the path-level
inspection and check the cursor position and unchanged contents.
The cases cover every truncation boundary through 20 bytes, each
magic byte, pairs of format values, page sizes, and trailing bytes.
Scripted cases cover fragmented input, repeated interruptions,
explicit EOF, and I/O errors before, during, or after the header.

The corpus has 67 regular-file cases and 12 additional read scripts.
Six broken variants distinguish the magic NUL, both format bytes,
page-size independence, interrupted reads, EOF, and unrelated I/O
errors.

All 79 cases matched on macOS in both the default and `contracts`
configurations. The 89-test schema suite passed in both configurations.
The session hard-link target passed 31 tests and the workflow suite
passed 222. Lean ran with a 2 GiB limit, and single-job Rust ran
with a 3 GiB limit.

```sh
(cd formal/HoiminOracle && lake exe generate_session_header -- --check corpus/session-header.jsonl)
(cd formal/HoiminOracle && lake exe generate_session_header -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --lib session_header_matches_lean_oracle -- --test-threads=1
```

## Lean model of session schema catalog identity

For the current schema version, admission compares SQLite's object
type, name, owning table name, and optional SQL text. The query orders
rows by type and name. The comparison normalizes ASCII whitespace
between existing SQL words and trims it at the ends. It preserves
SQL case and non-ASCII characters. The selected columns omit the
physical root page.

Lean proves that normalization preserves the object key and table name,
that equal word sequences normalize identically, and that NULL SQL
differs from an empty SQL string. Equality of the normalized catalogs
requires equal object counts, so adding an object changes admission.
The catalog model uses representative rows and the exact SQL
fragments used by the edits.

The Rust fixture obtains full definitions from the production
initializer. It applies each edit to those definitions and creates a
real version-five database. The fixture checks that SQLite stored
each changed fragment and that the creation-order and root-page
fixtures have the intended physical differences. It then calls the
normal session opener. On rejection it compares names and bytes in
the containing directory.

The 46 cases pair each edit with rollback and closed WAL journal modes.
They include spacing at an existing token boundary, quoted names,
non-ASCII spacing, missing constraints and foreign keys, altered
index definitions, and extra tables, indexes, views, and triggers.
Six sensitivity variants check SQL text, case, Unicode spacing,
indexes, creation order, and rejection of extra objects.

All 46 cases matched on macOS in both the default and `contracts`
configurations, with 12 admitted and 34 rejected. Each schema suite
passed 90 tests. The session handler target passed 102 tests with
one existing ignored test, and the workflow suite passed 222.
Lean ran under a 2 GiB limit and single-job Rust under a 3 GiB limit.

```sh
(cd formal/HoiminOracle && lake exe generate_session_catalog -- --check corpus/session-catalog.jsonl)
(cd formal/HoiminOracle && lake exe generate_session_catalog -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --lib session_catalog_matches_lean_oracle -- --test-threads=1
```

## Lean model of session configuration across journal failure

SQLite session configuration has a commit boundary before WAL
enablement. A fresh database passes admission, is rechecked after
acquiring the writer transaction, and is initialized and committed
before any journal-mode request. If the journal-mode request fails,
the opener returns that error and the committed schema remains.
A subsequent open admits the existing schema.

Lean proves that journal failure preserves the committed state,
and that journal success preserves schema, version, and rows while
enabling WAL. A disk-full error during initialization skips the
journal phase. The writer reread preserves the schema and rows when
another connection initialized the database after the first
observation.

The 13 cases execute the production configuration function on real
SQLite connections. They record the first version read, the writer
reread, the state before attempting WAL, the outcome, and the state
afterward. For named databases, another connection reads the state
at the journal boundary. The adapter closes the first connection
and reopens through the normal session handler, checking the schema,
journal mode, and any stored fingerprint.

The fixtures use SQLite's page-count cap for the initialization
failure and a second connection's exclusive transaction for journal
contention. The contention fixture sets both timeouts to zero for
one real journal request. The temporary-database fixtures observe a
real SQLite journal refusal and run configuration again on the same
connection. They exercise the commit boundary for a different kind
of failure. Six sensitivity checks cover rollback, the journal
boundary, the writer reread, success reporting, and row preservation.

All 13 cases matched on macOS in the default and `contracts`
configurations. The 91-test schema suite passed in each configuration.
The session handler and hard-link targets passed 133 tests with one
existing ignored test, and the workflow suite passed 222. Lean used a
2 GiB limit and single-job Rust used a 3 GiB limit.

```sh
(cd formal/HoiminOracle && lake exe generate_session_configuration -- --check corpus/session-configuration.jsonl)
(cd formal/HoiminOracle && lake exe generate_session_configuration -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --lib session_configuration_matches_lean_oracle -- --test-threads=1
```

## Lean model of stored result decoding and replacement

Session lookup checks ownership and the run state before reading a
stored result. The candidate and result rows must be both absent or
both present. With a present pair, the outcome JSON determines the
status. A failure to read the SQL value has its own diagnostic.
Lookup exposes a result for each valid status. Only killed and
survived outcomes are eligible for reuse, and only inconclusive
stored outcomes can be replaced. An output close timeout takes
precedence over the process termination when deriving status.

Lean proves the ownership gates, precedence of a missing candidate
over invalid outcome text, the close-timeout rule, and refusal to
replace a conclusive result. The matrix also exercises an owned run
whose database row has disappeared. Lookup reports the missing run.
An otherwise admissible write fails its deferred foreign key check
at commit and retains the prior rows.

The 79 cases exercise the production lookup, resume policy, and
persist calls on SQLite files. Each fixture begins with the
production schema and persisted rows, then sets the target row state
through a separate SQLite connection. The adapter checks stored
status, reuse, error code or corruption reason, unchanged target
rows on rejection, and the replacement candidate and diagnostic on
success. Rows sharing only the run ID or only the mutant ID serve
as controls. Payload cases cover execution termination variants,
output close timeout, reused and unstarted outcomes, malformed
JSON, and elapsed duration boundaries. Six sensitivity checks
exercise ownership, completion, row pairing, close timeout, reuse,
and replacement.

All 79 cases matched on macOS in default and `contracts` builds.
The session handler suite passed 103 tests in each build with one
existing ignored test. The existing Lean session target passed 59
tests and the workflow suite passed 222. Lean used a 2 GiB limit and
single-job Rust used a 3 GiB limit.

```sh
(cd formal/HoiminOracle && lake exe generate_stored_result -- --check corpus/stored-result.jsonl)
(cd formal/HoiminOracle && lake exe generate_stored_result -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --test session_handler stored_result_matches_lean_oracle -- --test-threads=1
```

## Lean audit of resume selection and fingerprint history

Resume uses insertion order to select the newest incomplete run
with the requested fingerprint and an eligible budget. A malformed
budget on a matching incomplete row remains eligible for selection
so validation can report the corruption. The selected row's budget
is validated before ownership is acquired. An invalid budget or
active owner ends the attempt at that row.

If the candidate query returns nothing, selection inspects the
fingerprint schema of the newest incomplete run with a matching
metadata row in the fingerprint table. A legacy schema or invalid
SQL type takes precedence over the ordinary fresh-run reason. A
run without a fingerprint metadata row is absent from this join.
An eligible exact fingerprint instead follows the candidate path,
even if its schema metadata is legacy, future, ill-typed, or missing.
Those metadata cases use deliberate database fixture edits.

Lean proves the selected-row precedence, budget validation before
ownership, active-owner refusal, legacy history precedence, and
preservation of other rows' budgets. The 49 cases use SQLite files,
real session ownership, and the production load call. They compare
the selected run or reason, every run's stored budget, the
fingerprint metadata, and which run the caller owns after the attempt.
Six sensitivity checks cover sorting, malformed budgets, active
ownership, the candidate and fallback order, legacy history, and
missing fingerprint metadata.

All 49 cases matched on macOS in default and `contracts` builds.
The session handler suite passed 104 tests in each build with one
existing ignored test. The related Lean session targets passed 93
tests and the workflow suite passed 222. Lean used a 2 GiB limit and
single-job Rust used a 3 GiB limit.

```sh
(cd formal/HoiminOracle && lake exe generate_resume_selection -- --check corpus/resume-selection.jsonl)
(cd formal/HoiminOracle && lake exe generate_resume_selection -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --test session_handler resume_selection_matches_lean_oracle -- --test-threads=1
```

## Lean model of resume recheck and ownership

After selecting a run, the loader obtains ownership or uses a lock
it already holds. An active peer prevents the recheck. The
conditional update compares the selected run ID, fingerprint,
incomplete flag, and original budget. A mismatch or zero updated rows
returns `candidate_changed`. A SQLite update refusal returns the
database error. These paths preserve the row state and any ownership
the loader held before the attempt, and release a newly obtained lock.
Only a successful update publishes new ownership.

The Lean predicate depends on the four compared fields. A budget
change prevents the update even when the new budget would fit the
request. Changing only `finished` or the internal SQLite row ID leaves
the predicate satisfied. A newer run inserted after selection does not
enter the recheck. A budget change reverted before recheck leaves the
original value visible to the conditional update.

The 45 cases apply changes from a separate SQLite connection after
the actual selection and before acquisition. They compare the
returned result, every run row, the loader's retained ownership,
and the ability of an independent handler to acquire the selected
run's lock. Temporary SQLite triggers exercise ignored and rejected
updates. The row-ID replacement and budget restoration cases use
explicit fixture edits. The audit examines one recheck after a stable
set of intervening edits. Six sensitivity variants cover the
compared fields, previous and provisional locks, and error reporting.

All 45 cases matched on macOS in default and `contracts` builds.
The session unit suites passed 161 and 173 tests respectively. The
session handler target passed 104 tests with one existing ignored
test, and the workflow suite passed 222. Lean used a 2 GiB limit and
single-job Rust used a 3 GiB limit.

```sh
(cd formal/HoiminOracle && lake exe generate_resume_recheck -- --check corpus/resume-recheck.jsonl)
(cd formal/HoiminOracle && lake exe generate_resume_recheck -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --lib resume_recheck_matches_lean_oracle -- --test-threads=1
```

## Lean model of finish transaction and retry

The finish handler checks ownership before opening its transaction.
An owned row is eligible when its complete flag is clear, even if it
has already been marked finished. A missing row or a complete flag
produces a state error after the transaction begins. This includes
the SQLite schema's legal combination of a set complete flag and a
clear finished flag.

Lean proves that refusals preserve the stored row and ownership and
that success sets the requested flags and releases ownership.
After a rejected commit, the same handler can retry with a different
completion flag once the fault is cleared. After success, another
finish requires the handler to acquire ownership again.

The 100 cases replay an attempt and a retry through the Rust handler.
A separate SQLite connection holds a competing write transaction.
Temporary triggers abort or ignore the update or introduce a deferred
foreign-key violation at commit. An independent observer reads the
committed flags and probes the locks for both the target run
and an unrelated run. Eight sensitivity variants exercise ownership,
row eligibility, update errors, rollback, and lock release. This audit
covers individual transaction failures and immediate retries after the
fault is cleared.

All 100 cases matched on macOS in default and `contracts` builds.
The session unit suites passed 162 and 174 tests respectively. The
session handler target passed 104 tests with one existing ignored
test, and the workflow suite passed 222. Lean used a 2 GiB limit and
single-job Rust used a 3 GiB limit.

```sh
(cd formal/HoiminOracle && lake exe generate_finish_transaction -- --check corpus/finish-transaction.jsonl)
(cd formal/HoiminOracle && lake exe generate_finish_transaction -- --sensitivity)
CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p hoimin-cli --lib finish_transaction_matches_lean_oracle -- --test-threads=1
```
