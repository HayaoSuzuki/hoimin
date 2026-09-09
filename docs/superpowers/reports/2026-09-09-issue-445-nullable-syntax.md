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
