# Issue #445 nullable annotation syntax repair

Nullable-removal candidates now preserve the source syntax required by the
retained type expression. Union removal retains explicit operand parentheses
through Ruff token boundaries. Optional removal locates its own brackets after
the complete base expression, keeps its original interior, and groups
multiline/comment-bearing interiors when removal would otherwise invalidate the
annotation.

Focused Rust regressions require one candidate and reparse the complete mutated
module for Optional, both union orders, parameter and return annotations, and
comments. The public-plan CPython contracts evaluate type members for variable,
parameter, and return annotations, including unparenthesized multiline Optional
forms, nested grouping, comments, and ordinary single-line controls. The issue
reproduction changes `{int, str, NoneType}` to `{int, str}`.

The initial focused regression failed before the implementation because
`Optional[(int\n | str)]` produced `int\n | str`. After the repair,
`cargo test --offline -p hoimin-cli --lib
analyzer::rust::rust_tests::nullable_removal_preserves_multiline_annotation_syntax
-- --exact` passed 1 test, and `cargo test --offline -p hoimin-cli --test
operator_function_contracts` passed 14 tests. `cargo fmt --all -- --check` and
`git diff --check` both exited 0. The controller owns full-workspace, MSRV,
Clippy, and independent CLI-matrix verification.

## Review round 1 repair

The retained side of an unparenthesized multiline union is now grouped before
replacement, independently of annotation-level parentheses outside the candidate
span. Optional bracket interiors retain leading and trailing whitespace around
an already-grouped operand; only an exact full-interior match may use the
existing grouping directly. Exact-replacement/reparse and public-plan CPython
contracts cover both repairs.

The public-plan contracts are split into annotation-site, Optional-layout, and
union-layout tests to keep each behavioral scope small enough for
warnings-denied Clippy. Focused verification passed: the analyzer regression
passed 1 test, `operator_function_contracts` passed 16 tests, and
`cargo clippy --offline --workspace --all-targets --all-features -- -D warnings`,
formatting, and diff checks exited 0. The controller owns the final workspace
and CLI-matrix verification.

## Review round 2 repair

Trivia detection now inspects gaps between Ruff parser tokens and explicit
comment/newline tokens. It groups actual multiline or commented source without
mistaking `#` inside a string literal for a comment. Regressions and public-plan
contracts cover both `resolve("#") | None` and `Optional[resolve("#")]`, while
the multiline/comment cases remain covered. The focused analyzer test passed,
the 16-test contract target passed, and warnings-denied all-targets/all-features
Clippy, formatting, and diff checks passed.
