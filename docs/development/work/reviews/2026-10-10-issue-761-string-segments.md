# Issue #761: string segment emptying

Work branch: `feat/issue-761-string-segment-empty`, base `d6f4be7`.
The user requested default enablement after the initial opt-in proposal.
`string_segment_empty` therefore joins the default runtime selection; explicit
selections and saved plans remain exact. The historical 43 operators stay fixed.

## Design self-review

1. Scope: one candidate per expression, using its first eligible nonempty source
   segment; do not enumerate all pieces. Include mixed ordinary/f-string adjacency.
2. Evaluation: delete only a top-level fixed f-string span. Preserve interpolation
   source, debug text, conversions and format specifications, including nested fields.
3. Roles: retain the old string exclusion index. A separate segment index excludes
   docstrings, aliases, interpolation contents and complete t-strings; shared facts
   continue excluding annotations and patterns.
4. Source: decoded emptiness and AST source ranges handle escapes and doubled braces.
   Reuse the existing original-encoding offset conversion and source hash checks.
   Validate boundary-sensitive forms and document any conservative exclusions.
5. Compatibility: append the operator enum variant, preserve existing variant order,
   default to 53 runtime operators and keep explicit selection/legacy selection exact.
   Additional default candidates can change capped selection and run time.

## Implementation-plan self-review

1. Start with failing public CLI tests for selection and concrete replacements,
   then implement the operator and syntax-aware helper.
2. Exercise external CPython 3.14 for syntax, interpolation order/count, conversion,
   dynamic format specs, raw/triple strings, Unicode, braces and escapes.
3. Cover ordinary/mixed concatenation, intervening comments, decoded-empty pieces,
   source encoding/newline offsets, nested exclusions and unchanged old selections.
4. Verify saved plans with strong/weak assertions, source tampering and candidate
   limits. Measure candidate increments on actual repository Python tools and review
   representative changes without equating a string change with a useful defect.
5. Update usage, generated CLI reference and analyzer knowledge with verified sources.
   Run strict Rust checks, focused and broader regressions; inspect disk use and clean
   Cargo artifacts periodically. Commit all documentation on the work branch.

## Implementation self-review

1. Operator selection: append the enum variant and canonical ID; legacy43 remains
   unchanged. Default53 and exact explicit sets pass the core selection tests.
   Ranking classifies the new operator as Behavioral; the fixed-category test now
   enumerates all 72 canonical IDs.
2. Exclusion isolation: the original `all` index still excludes entire f/t-strings.
   The segment index instead excludes interpolation ranges and complete t-strings,
   alongside common docstring/type-alias ranges. Shared AST facts exclude annotations;
   pattern traversal still suppresses candidates. Nested fields cannot escape this.
3. Literal selection: source-order iterators stop after the first decoded nonempty
   piece. Nonrecursive AST literal iteration does not select debug-derived text or
   format-spec text. Cancellation is polled while iterating parts and elements.
4. Boundary correction: independent review found that `'a'"b"` became `"""b"`,
   and `f'a''b'` became `f'''b'`. A failing CPython regression reproduced the bug.
   Empty ordinary tokens now separate adjacent quotes with spaces; a wholly fixed
   f-string token becomes `f""`, preserving f-string expression semantics. Bare
   expressions receive no new indentation. No interpolation is removed.
5. Integration and resource costs: reuse candidate construction, selectors, source
   encoding conversion, retention/caps and source validation. A dedicated collector
   method keeps `visit_expr` under the strict Clippy line limit without an exception.
   Generated CLI reference and usage describe opt-out and capped-selection effects.
6. Inventory coverage: the whole-workspace run exposed a missing registration in
   `crates/hoimin-cli/tests/fixtures/valid-python-operators.json`. Register the operator with an explicit
   reason that its dedicated executable tests are authoritative outside the initial
   cross-producer corpus. This follows the existing string/operator registrations;
   it does not claim a generated Lean oracle for the new operator.

The read-only reviewer found the quote-boundary bug above, then inspected the fix and
reported no remaining Critical, Important or Minor issues. The reviewer did not run
competing Cargo commands or claim the unfinished validation/documentation was done.

## Test self-review

1. Red/green: the initial public default-selection test failed with zero segment
   candidates, then passed with the implementation. The separate quote-boundary test
   failed with CPython `SyntaxError`, then passed after the boundary correction.
2. Syntax and values: external CPython 3.14 checks prefix/suffix, multiple fields,
   raw/triple quotes, Unicode, named escapes, escaped quotes/newlines and doubled
   braces. No-whitespace ordinary/mixed adjacency is tested with short/triple quotes,
   preceding empty pieces and later bare expression statements.
3. Evaluation: counted calls retain order/count; custom `__repr__`, `__str__` and
   `__format__` verify `!r`, `!s`, `!a` and dynamic widths. An initial test expectation
   put width before repr; CPython showed conversion occurs first, so the oracle was
   corrected to the observed language behavior, without changing production code.
4. Contracts: executable tests cover exclusion roles, repeated deterministic plans,
   opt-out, explicit old selection, restored legacy plans, candidate caps and target
   selection. UTF-8/BOM/Latin-1 crossed with LF/CRLF/CR checks original byte ranges;
   tampered offsets and changed source are rejected. Existing string_literal_empty
   tests still pass. The legacy token regression uses legacy selection so intentional
   new literal mutations do not weaken its protection against token misrecognition.
5. Meaning and usefulness: saved-plan execution kills strong fixed-text assertions
   and preserves survivors under weak value-only assertions, for both f-strings and
   ordinary concatenation. Real tools are measured and reviewed separately below;
   a survivor alone does not establish a useful defect or equivalence.
6. Broad-run follow-up: the initial workspace command passed the CLI library
   (804 passed, 12 ignored), e2e (68 passed, 1 ignored), analyzer integration
   (297 passed, 3 ignored) and preceding test binaries, then failed only the operator
   inventory registration. After fixing that data file, rerun the entire failed
   `valid_python_corpus` target (280 passed, 3 ignored), every subsequent CLI test
   target, the complete core suite and workspace doc tests. All follow-up runs passed;
   unchanged already-passing targets were not needlessly repeated.

## Verification and actual-project assessment

Focused checks passed: new string-segment tests (9), previous string-literal tests
(4), default-selection tests (2), CLI configuration tests (56), and core operator
selection tests (16). Strict `cargo clippy --locked --workspace --all-targets
--all-features -- -D warnings` passed, as did Rust formatting, generated CLI example
tests (7), and generated-reference `--check`. Full workspace targets were covered
across the initial run and the corrective continuation described above; this is
not a claim that the first workspace command exited zero.

Python `ruff check .`, `ruff format --check .` (43 files) and `ty check` passed.
OKF YAML and reserved-file checks covered all 34 knowledge pages; new source hashes,
source IDs/footnotes, analyzer links and index reachability were checked. The existing
abstract `DefaultOperatorSelection.lean` theorems quantify arbitrary disjoint sets;
they need no operator-domain edit. No new Lean proof of the Rust span implementation
is claimed. Existing audits about the earlier 71-ID domain remain historical.

After the Rust validation milestone, `cargo clean` removed 37,139 files (34.5GiB).
Free space was 191,445,606,400 bytes (approximately 178GiB) afterward. No build output
is committed. Subsequent verification that needs rebuilding will be cleaned again.

The existing Python tests for `tools/sbom.py` and `tools/ci_selection.py` passed
(85 tests, Python 3.14). Comparing previous52 against default53 on these actual tools
produced 678 versus 689 candidates: 11 additions (approximately 1.62%). All 678
existing candidate IDs were retained unchanged. The additions include SBOM filename
prefixes, pinned source URLs, package URLs, provenance notes and CI diagnostics/output.
Filename and machine-output separators have concrete consumer contracts; diagnostic
wording and prose are lower-priority review targets. This is a deliberately small
repository-tools sample, not evidence for candidate utility across arbitrary projects.

Representative verify runs used one job, an 8GiB workspace limit and a 10GiB free-space
reserve, with Python 3.14 running the same 85 tests as the baseline. SBOM `filename`
at `tools/sbom.py:38` (candidate
`m1_35a0300560ccf5bc225029d534bc46b4e233e72ede04dca6909ca994963b0297`)
lost `hoimin-v` and was killed. This violates the published asset naming convention.
CI `aggregate` at `tools/ci_selection.py:159` (candidate
`m1_bfbb3f5430654fff80080f8609d2a3bf51c129665a0bf7dab5c137db21973cd0`)
lost `: expected ` and survived. Existing tests assert rejection/status behavior,
not the exact diagnostic wording; this is a lower-priority wording change, not enough
evidence to call the tests deficient. Both fresh baselines exited zero; each verify
completed exactly one candidate, with no timeout/error/inconclusive result. All
evaluation temporary directories were removed. No production Python test was changed
just to kill the diagnostic survivor.

The first evaluation script incorrectly looked for an `asset_name` symbol rather
than its actual `filename` symbol; it stopped and removed its temporary directory.
A subsequent verify rejected a concurrent checkout edit with `workspace.original.changed`;
no mutation outcome was inferred from that rejected run, and its temporary files were
removed. Repeat evaluation uses an unchanged checkout while workers are running.

Documentation stays in the canonical hoimin usage and analyzer knowledge pages.
This operator needs no separate article in HayaoSuzuki.github.io.
