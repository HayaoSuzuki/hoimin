# Task 1 report: nullable annotation syntax

## Delivered change

`type_nullable_remove` now receives parser facts privately. Union removal uses
Ruff's parenthesized-range lookup with the enclosing `BinOp`, preserving the
retained operand's source grouping. Optional removal finds its own `[` after
the complete base expression, preserves the bracket interior, and supplies a
parenthesized replacement only when multiline or comment trivia would lose the
bracket continuation context. Plain `Optional[int]` and `int | None` still
produce `int`.

The analyzer regression requires one nullable-removal candidate, asserts its
replacement, applies its reported byte span, and reparses the whole module. It
covers the issue reproduction, both union directions, parameter and return
annotations, and comments. The CPython contract uses the public `plan` output,
then evaluates `typing.get_type_hints` results after applying the manifest span.
It covers variable, parameter, and return sites; unparenthesized multiline
Optional syntax; comments; nested grouping; both union directions; and ordinary
single-line controls. The reproduction observes `{int, str, NoneType}` before
the mutation and `{int, str}` after it.

## TDD evidence

RED, before production edits:

```text
CARGO_TARGET_DIR=/private/tmp/hoimin-python-operator-target \
HOIMIN_OPERATOR_TEST_PYTHON=/Users/hayao/RustroverProjects/hoimin/.venv/bin/python \
cargo test --offline -p hoimin-cli --lib \
  analyzer::rust::rust_tests::nullable_removal_preserves_multiline_annotation_syntax \
  -- --exact

FAILED: Optional grouped union
left:  "int\n | str"
right: "(int\n | str)"
```

GREEN, after the localized token-boundary implementation:

```text
cargo test --offline -p hoimin-cli --lib \
  analyzer::rust::rust_tests::nullable_removal_preserves_multiline_annotation_syntax \
  -- --exact
1 passed; 0 failed
```

## Focused verification

Commands use `CARGO_TARGET_DIR=/private/tmp/hoimin-python-operator-target`.
The CPython contract also uses
`HOIMIN_OPERATOR_TEST_PYTHON=/Users/hayao/RustroverProjects/hoimin/.venv/bin/python`.

| Command | Result |
| --- | --- |
| `cargo test --offline -p hoimin-cli --lib analyzer::rust::rust_tests::nullable_removal_preserves_multiline_annotation_syntax -- --exact` | 1 passed |
| `cargo test --offline -p hoimin-cli --test operator_function_contracts` | 14 passed |
| `cargo fmt --all -- --check` | Exit 0 |
| `git diff --check` | Exit 0 |

The controller owns the CLI matrix, workspace suite, MSRV check, and Clippy
gates specified by the task plan.

## Self-review

1. Confirmed the union helper uses the enclosing annotation `BinOp`, never
   Call-specific parenthesis handling, and leaves annotation-level parentheses
   outside the candidate span untouched.
2. Confirmed the Optional opening bracket is selected only after the complete
   parenthesized base range, while the closing bracket is the final subscript
   token. No raw bracket scan or parse-and-discard fallback was added.
3. Confirmed the contracts check generated manifest candidates and evaluated
   annotations rather than source text alone. Existing source order,
   eligibility, schemas, and ordinary single-line replacement text remain
   unchanged.

## Scope and concerns

No production concerns found in task scope. The requested broad final gates are
intentionally deferred to the controller.

## Review round 1 repair

Review found two uncovered source-boundary cases and one test-organization
warning. For `x: (int\n | str\n | None)`, the retained left union operand was
multiline but had no operand-local parentheses. The union helper now adds a
grouping context when an unparenthesized extracted operand contains a newline,
carriage return, or comment. For `Optional[\n    (int | str)\n]`, the Optional
fast path previously compared trimmed text and discarded the bracket interior's
leading and trailing whitespace. It now preserves an already-grouped operand
only when the full interior matches exactly; otherwise multiline/comment
interior is emitted inside fresh grouping.

The new unit cases assert the exact replacement and reparse both reported
examples. The public-plan contracts evaluate both forms through
`typing.get_type_hints`. The previous oversized contract test is split into
annotation-site, Optional-layout, and union-layout tests, each retaining the
shared manifest-span assertion.

RED evidence:

```text
nullable_removal_preserves_multiline_annotation_syntax
FAILED: None trailing ungrouped multiline union
left:  "int\n | str"
right: "(int\n | str)"

nullable_optional_removal_preserves_source_layout
FAILED: Optional grouped union surrounding whitespace
left:  "(int | str)"
right: "(\n    (int | str)\n)"
```

GREEN verification used the same target directory and CPython interpreter as
above.

| Command | Result |
| --- | --- |
| `cargo test --offline -p hoimin-cli --lib analyzer::rust::rust_tests::nullable_removal_preserves_multiline_annotation_syntax -- --exact` | 1 passed |
| `cargo test --offline -p hoimin-cli --test operator_function_contracts` | 16 passed |
| `cargo clippy --offline --workspace --all-targets --all-features -- -D warnings` | Exit 0 |
| `cargo fmt --all -- --check` | Exit 0 |
| `git diff --check` | Exit 0 |

The source-boundary repair adds no fallback parsing, schema change, or public
interface. Broad workspace and independent CLI verification remain with the
controller.

## Review round 2 repair

The round-one grouping check treated every raw `#` as comment trivia, so a
supported retained expression such as `resolve("#") | None` unnecessarily
became `(resolve("#"))`. The same condition changed
`Optional[resolve("#")]`. The replacement logic now detects line-continuation
and comment trivia only outside Ruff token spans, including explicit newline and
comment tokens. A `#` or newline inside a string token is therefore ordinary
expression content, while genuine comments and multiline layout still require
grouping.

RED regression output showed both literal-hash cases produced
`(resolve("#"))` rather than `resolve("#")`. Unit exact-output/reparse cases
and public-plan CPython contracts now cover the union and Optional forms; the
existing multiline and comment cases remain in the same test groups.

| Command | Result |
| --- | --- |
| `cargo test --offline -p hoimin-cli --lib analyzer::rust::rust_tests::nullable_removal_preserves_multiline_annotation_syntax -- --exact` | 1 passed |
| `cargo test --offline -p hoimin-cli --test operator_function_contracts` | 16 passed |
| `cargo clippy --offline --workspace --all-targets --all-features -- -D warnings` | Exit 0 |
| `cargo fmt --all -- --check` | Exit 0 |
| `git diff --check` | Exit 0 |

No candidate policy, schema, or fallback parsing behavior changed.
