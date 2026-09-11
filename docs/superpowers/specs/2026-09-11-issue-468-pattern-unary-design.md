# Issue 468: Preserve literal-pattern syntax for unary signs

Issue: https://github.com/tokyogas-tech/hoimin/issues/468

## Contract and root cause

The [Python 3.14 literal-pattern grammar](https://docs.python.org/3.14/reference/compound_stmts.html#literal-patterns) allows an optional minus before a number, but no leading plus. The real/imaginary separator in a complex literal is a separate binary sign. Generating `case +1` from `case -1` therefore creates a syntax error instead of a valid mutant.

`AstFacts` visits the expressions inside patterns and records their unary operator tokens as if they were ordinary expressions. Token candidate collection trusts `operator_token_starts`, then `unary_sign_starts` selects minus-to-plus replacement. The AST candidate collector already tracks pattern context for other operators, but that context does not govern this facts pass.

## Design

Track pattern context in `AstFacts` during `visit_pattern`, restoring the previous context after recursively walking nested patterns. Do not register unary-sign operator tokens for pattern expressions. Their exclusion must happen at token eligibility, so they cannot fall back to a binary add/sub mutation. Keep ordinary unary signs in match subjects, guards, case bodies and later statements eligible. Preserve boolean literal patterns and valid binary separators within complex patterns.

Suppressing this unsupported unary-sign mutation is the selected behavior. Introducing a new sign-removal mutation would change the operator's semantics and is unnecessary to meet the issue contract. Existing candidate IDs, byte spans, ordering and selection behavior stay unchanged for remaining candidates. Selecting the invalid candidate from a previously saved plan must fail existing rediscovery/verification checks rather than run it. Other still-discoverable selections remain governed by the existing plan contract.

## Verification boundary

Use focused Rust regressions for direct, nested, grouped, sequence, OR, class and mapping-key patterns; integer, float, imaginary and complex numeric forms; and context restoration for ordinary signs. Compile all emitted candidates in the targeted unary-sign/boolean fixtures with CPython 3.14. Test actual plan/run on the issue's import-only reproduction: zero invalid unary candidates and no syntax-error kill. Include ordinary-expression and boolean positive cases, so excluding every token cannot satisfy the suite. Test selection/verify where useful without expanding unrelated operator policy.

No new Lean model is needed: the disputed contract is Python grammar and actual compilation, which the interpreter supplies directly. Existing formal corpus adapters remain in workspace checks. This change does not address duplicate mapping keys from other mutation operators (issue 485) or arbitrary parser/resource correctness.

## Design self-review

1. Root cause: checked both AST-fact token sets and token candidate eligibility; simply omitting unary classification would incorrectly allow binary fallback.
2. Boundary: nesting requires save/restore, while guards and bodies must leave pattern context. Complex binary signs and boolean patterns retain their existing behavior.
3. Scope and compatibility: choose conservative exclusion without inventing an operator variant; preserve remaining candidates and rely on existing saved-plan rediscovery for stale invalid IDs.
