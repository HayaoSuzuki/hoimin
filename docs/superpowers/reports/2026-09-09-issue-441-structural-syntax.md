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

## Independent CLI validation

Using the actual built CLI and CPython 3.14.7, a 16-candidate matrix covered
parenthesized/multiline sort, append, insert, extend, mapping receivers and
bracket-containing comments. Before the change, 10 of these candidates caused
SyntaxError. After the change, all 16 parsed successfully, with every expected
candidate still present. A separate receiver/key matrix produced all 96 expected
mapping candidates, and each applied mutant parsed under CPython. This matrix
included nested parentheses, CRLF, tuple keys, comments containing brackets,
and both mapping directions.

A real `hoimin run --file case.py --operators structure_mapping_get_subscript
--max-mutants 1 --format json -- .../python -m py_compile case.py` used the same
multiline mapping fixture before and after. Baseline compilation passed in both
runs. Before: mutant killed with Exit(1), score 1.0, CLI exit 0. After: mutant
survived with Exit(0), score 0.0, CLI exit 1. The verification command only checks
syntax, so surviving is the expected result. The command also supplied the
fixture root, `--allow-best-effort-memory`, and `--min-free-space 1B` on macOS.

The Rust 1.88 all-workspace/all-targets/all-features locked check and full
warnings-denied Clippy passed. `cargo test --offline --workspace --all-features
-- --test-threads=1` completed with 1,585 passed, zero failed, and 13 ignored
across 67 result groups. Independent review results are recorded below.

## Round-one review repair

The preceding whole-workspace, MSRV, and full-Clippy record applies to the
pre-review-fix revision. Independent review found two structural boundary bugs:
AST argument expression ranges omit user parentheses, and selecting the first
square-bracket token in a full subscript range can select a receiver's inner
subscript.

The new RED exact-output/reparse test failed with all three concrete symptoms:
`items.append((value))` became `items.insert((0, value))`,
`items.insert((0), ((value)))` became invalid `items.append((value)))`, and
`obj.items[0].data[key]` used the receiver's first bracket. The repair gets each
argument's complete parenthesized range using its `Arguments` AST parent,
removes the first insert argument and separator independently so comments stay
in place, and selects an opening subscript bracket only after the preserved
receiver range.

The focused GREEN verification after the repair was:

| Command | Result |
| --- | --- |
| `cargo fmt --check` | Exit 0 |
| `cargo test --offline -p hoimin-cli --lib structure_` | 9 passed |
| `cargo test --offline -p hoimin-cli --lib collection_` | 11 passed |
| `cargo test --offline -p hoimin-cli --test operator_function_contracts` | 10 passed |
| `git diff --check` | Exit 0 |

The exact-output fixtures now include parenthesized append values, parenthesized
zero and value insert arguments, retained argument comments, and an inner
receiver subscript. The external CPython contract verifies both collection
directions with grouped argument evaluation. The controller owns the fresh
whole-workspace, MSRV, full-Clippy, and CLI-probe verification of this revision.

## Final review repair and test organization

Final review found that `extend_to_append_replacement` retained a singleton
list's trailing comma after removing its brackets. This produced
`items.append(value,,)` for `items.extend([value,],)`, and changed a grouped
list into a tuple for `items.extend(([value,]))`. The new RED exact-output test
also covers a tuple element and a trailing element comment.

The repair removes only the comma token between the single list element and the
list closing bracket. A call's trailing comma and a tuple element's internal
comma remain untouched. CPython contract cases verify the resulting call with a
trailing comma plus tuple and comment preservation.

Review also found Clippy `too_many_lines` failures in the two expanded tests.
The unit test is now split into named mapping/callee and collection-argument
cases; the CPython contracts are split into grouped mapping/append, grouped
collection arguments, and the two singleton-extend cases. This preserves the
behavioral assertions without lint suppression.

| Command | Result |
| --- | --- |
| `cargo fmt --check` | Exit 0 |
| `cargo test --offline -p hoimin-cli --lib structure_` | 10 passed |
| `cargo test --offline -p hoimin-cli --test operator_function_contracts` | 13 passed |
| `cargo clippy --offline --workspace --all-targets --all-features -- -D warnings` | Exit 0 |
| `git diff --check` | Exit 0 |
