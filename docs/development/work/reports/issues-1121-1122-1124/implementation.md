# Slice-bound and division operator implementation

The maintainer requested implementation after the research decision recorded
in commit `1efe26f5`. This document describes that subsequent change on PR #1133;
the [research report](README.md) and its Python JSON files retain their original
measurement scope. See the [implementation plan](../../plans/2026-10-28-issues-1121-1124-implementation.md)
and [implementation review record](implementation-review.md).

## Current behavior

| ID | Scope | Candidates per eligible site |
| --- | --- | --- |
| `binary_div_floor` | Binary `/` and `//` exchange, retaining existing multiplication/modulo mutants | 1 additional candidate |
| `structure_slice_bound_omit` | `xs[a:b]` omits either complete bound | At most 2 |
| `structure_slice_bound_move` | `xs[:n]` / `xs[n:]` moves the separator to the opposite side | 1 |

All three are enabled by default. `--exclude-operators` excludes each named ID;
`structure_ops` includes both slice operators. The catalog now contains 94
operator IDs, with 75 in the default runtime set. Explicit selectors keep their
usual semantics and do not acquire unrelated defaults.

Slice changes require a one-dimensional load-context subscript without a step
value. Bounds must be names or integer literals, including negative, grouped,
large, hexadecimal or underscored integer spellings. Calls, attributes,
compound expressions, non-integer literals, annotations and direct store/delete
targets are excluded. No-bound slices and an additional all-range mutation
are not included. Slices with an empty step field still have no step value.

Omission removes the bound's complete parenthesized range. Comments inside the
deleted expression are part of that deletion; unrelated surrounding text stays
in place. Movement retains slice text and moves only the colon, preserving
comments adjacent to the separator as well as those inside grouped bounds.
The independent review caught the initial adjacent-comment loss, reproduced by
a failing test and fixed before completion.

Division changes only an AST-confirmed binary operator token. Function positional
separators, annotations, strings, operator-function calls and augmented division
are excluded. Each operand is still evaluated once, left to right. Integer
operands can change result type, float operands remain floats, and overloaded
operands dispatch the corresponding dunder method. Zero division still raises
at runtime. Such exceptions are execution outcomes, not evidence of a test defect.

## Ownership and safety

The core catalog owns enum/wire identity, default membership and selector-family
membership. CLI ranking assigns the new operators existing arithmetic or
behavioral reasons; ranking scores and rule version are unchanged. The Python
analyzer owns applicability and byte ranges. No target imports or added
third-party dependencies participate in discovery.

Both producers use existing candidate selection, bounded prefix retention,
payload charging and cooperative cancellation. UTF-8 source offsets remain byte
ranges. Movement checks the target before copying its replacement. The change
adds at most two slice candidates or one extra division-token candidate per
site, without another traversal or a new retained full-source collection.

Issue #1122's proposed scheduling policy remains declined. No new deferral status,
plan/report schema, resume state or progress meaning is added. Saved plans retain
their concrete operator selection; generate a new plan to investigate the new
defaults. The version floor is unchanged, and no artifact conversion is required.

## Verification and evidence boundary

The new analyzer tests check exact edits, UTF-8/comments/parentheses, exclusions,
target selection, truncation and cancellation. Public CLI tests cover default
discovery, opt-out and weak/strong `plan`/`verify` behavior for omission, both
movement directions and both division directions. CPython contract tests cover
operand order, int/float result behavior, overloaded dispatch and zero division.

Final commands, results, environment limits, reviewer findings and resource
cleanup are recorded in [implementation-review.md](implementation-review.md).
Only macOS-hosted executions are claimed here; platform-independent Rust code
and source tests do not establish Linux/Windows runtime verification. Research
efficacy and equivalence limitations remain in force.
