# Issue #441 structural syntax repair

## Delivered scope

Structural call replacements now edit the original full call span. Method
renames retain parenthesized callees, and append/insert/extend conversions edit
only the method name and argument delimiters. Mapping conversions use Ruff's
parenthesized-expression range so a multiline receiver remains grouped. The
subscript conversion identifies `[` and `]` as parser tokens, so a bracket in a
receiver comment cannot become the delimiter.

The regression tests cover grouped and nested sort/reverse calls, multiline
mapping receivers, a comment containing `[`, tuple multiline keys, grouped
append/extend/insert calls, CRLF, and a Unicode prefix. Every expected candidate
is asserted exactly and applied back to the complete module before reparsing.
The Rust-driven CPython integration test invokes the public plan command,
applies its reported byte span, and checks grouped mapping and append behavior
including one evaluation of the key/value expression.

## TDD evidence

Baseline: `CARGO_TARGET_DIR=/private/tmp/hoimin-python-operator-target cargo
test --offline -p hoimin-cli --lib structure_` passed 7 tests.

RED: after adding the focused tests, `cargo test --offline -p hoimin-cli --lib
structure_replacements_` failed 2 of 3 tests. The observed replacements removed
the opening parenthesis from grouped calls and mapping receivers, and chose the
`[` in `# [ receiver comment` rather than the subscript delimiter. The initial
external integration fixture also proved the public-plan assertion meaningful:
it found four unrelated append candidates instead of the intended one; the
fixture was corrected to use `insert` for event logging, then the real grouped
append candidate was exercised.

GREEN: the same focused unit group passed 3 tests and the focused external
contract passed 1 test after localized edits were implemented.

## Verification record

Commands use `CARGO_TARGET_DIR=/private/tmp/hoimin-python-operator-target`.
The external contract also uses
`HOIMIN_OPERATOR_TEST_PYTHON=/Users/hayao/RustroverProjects/hoimin/.venv/bin/python`.

| Command | Result |
| --- | --- |
| `cargo fmt --check` | Exit 0 |
| `cargo test --offline -p hoimin-cli --lib structure_` | 9 passed |
| `cargo test --offline -p hoimin-cli --lib collection_` | 11 passed |
| `cargo test --offline -p hoimin-cli --lib analyzer::rust::rust_tests` | 152 passed, 2 ignored |
| `cargo test --offline -p hoimin-cli --test operator_function_contracts` | 10 passed |
| `cargo clippy --offline -p hoimin-cli --all-targets -- -D warnings` | Exit 0 |
| `git diff --check` | Exit 0 |

## Self-review

1. Delimiter ownership: checked that call-builder edits begin from
`call.range()` and that mapping receiver ranges include surrounding parentheses.
2. Token boundaries: checked that only parser `Lsqb`/`Rsqb` tokens identify a
subscript and that extend removal uses its list's real bracket tokens.
3. Compatibility: checked existing structure and collection exact-output tests,
full analyzer unit tests, external public-plan behavior, formatting, and
warnings-denied Clippy. The candidate identity scheme, eligibility, ordering,
and ordinary candidates remain unchanged; repaired replacements correctly
produce new candidate IDs where replacement text changes.

The controller owns the remaining whole-workspace, MSRV, and CLI matrix gates.
