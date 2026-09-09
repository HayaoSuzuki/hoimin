# Issue 445: Preserve syntax when removing nullable annotations

## Scope and contract

Baseline: main 58817cfa76bcb7ce47e3bd7c1bb1109ee917df19. The nullable-removal operator must produce a valid remaining type expression for supported Optional[T], None | T and T | None annotations. Variable, parameter and return annotations must preserve grouping, multiline continuation, and comments in the retained type. Do not suppress valid candidates or adjust result classification to hide syntax errors.

The user authorizes design, implementation, tests and PR creation autonomously, with separate worktrees and at least three self-reviews of both design and plan. Worktree: .worktrees/issue-445-nullable-syntax. Branch: fix/issue-445-nullable-syntax.

## Current behavior

nullable_removal slices the remaining AST expression.range(). That excludes enclosing parentheses. Optional[(int\n | str)] becomes int\n | str, so module and return annotations are invalid. The actual CLI reports killed / score 1.0 with only py_compile. This is a replacement-generation defect, not a result-policy defect.

## Alternatives and decision

1. Reparse every generated module and discard failures: rejects useful candidates and repeats module work. Rejected.
2. Wrap every replacement in parentheses: syntax safe for supported type expressions, but changes ordinary output and discards comments outside the AST range if extraction remains unchanged. Not the default.
3. Preserve the source delimiters and trivia of the retained expression, introducing grouping only when removal of Optional's brackets removes its continuation context. Selected.

Thread parser tokens through the private annotation replacement helpers. For union removal, use Ruff's parenthesized_range with the containing BinOp to retain the operand's grouping; retain any outer annotation parentheses already outside the candidate span. For Optional, use the actual Subscript bracket boundaries after the complete base expression, and preserve its inner source including comments and whitespace. Return the current simple expression text for an ordinary trivia-free argument. Otherwise enclose the preserved interior in parentheses, replacing the continuation context formerly supplied by brackets. Do not search raw text for brackets because comments and grouped attribute bases may contain them.

If a retained union expression is multiline without its own grouping, provide a grouping context instead of depending on the insertion site's incidental surrounding parentheses. Keep functions named for their syntax responsibilities and avoid source comments except truly necessary API documentation.

## Compatibility and bounds

Rust MSRV 1.88. No new dependency or public schema/API. Preserve operator eligibility, candidate span, source-order behavior, and ordinary single-line replacements. Corrected replacement text can change candidate IDs by the existing ID algorithm. Keep token searches bounded to the annotation or relevant subexpression; no full-module scan per candidate.

## Validation

Add analyzer regressions that assert candidates remain present and apply replacement bytes to a parsable module. Add CPython contracts that inspect the actual annotation's type members before and after (NoneType disappears, other members remain), including variable/parameter/return contexts. Include comments, nested grouping and single-line controls. Execute the actual CLI with py_compile alone and require baseline Exit(0), survived Exit(0), score 0 and CLI exit 1. Run workspace/all-features, MSRV all-targets/all-features, Clippy warnings denied, fmt and diff checks. Include tracked verification evidence in this worktree.

## Design self-review 1 — syntax contexts

Checked Optional with and without explicit operand parentheses, union operands on both sides, and annotation-level parentheses outside the candidate span. Found that preserving AST operand parentheses alone does not fix `Optional[int\n | str]`; the selected Optional interior strategy explicitly supplies a continuation context. Retain leading/trailing comments inside Optional brackets, not only comments between operand AST tokens.

## Design self-review 2 — boundaries and compatibility

Reviewed Ruff's parenthesized-range API and the prior structural fix. Use the actual BinOp parent for union operands, and bound Optional delimiter lookup after the grouped base expression. Keep ordinary `Optional[int] -> int` and `int | None -> int` text stable. Candidate identity changes are limited to corrected replacement text under the existing algorithm. Invalid runtime typing constructs such as Optional[int,] are not evidence of a supported type; this task does not expand typing eligibility.

## Design self-review 3 — failure modes and scope

Checked that no change to result classification, discovery limits or profile filtering is required. Inserting parentheses around preserved Optional interior must also preserve trailing-comment newlines so the closing delimiter cannot be swallowed. The tests must require a candidate, not accept an empty result. Per-annotation bounded token lookup avoids turning correctness into repeated full-source work. No unresolved design gap remains.
