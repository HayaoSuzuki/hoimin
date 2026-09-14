# Issue #471: Negative index and slice neighbors

## Contract

Recognize a plain ASCII decimal integer, optionally under one unary minus, in load-context indices and slice lower/upper/step positions. Replace the whole AST expression including its minus; preserve surrounding parentheses and bytes outside that range. Parentheses inside the unary expression may be replaced with the expression. Expressions, unary plus, repeated signs, hex, underscores, and floats remain excluded. Existing annotation/store/delete suppression remains in place.

Keep the original unsigned decimal input range 0..18446744073709551615 and existing +1 then -1 generation order. Add negative spellings with magnitude at most 18446744073709551615. Neighbors outside that signed-magnitude range are omitted; larger Python integers are skipped without panic. Unsigned zero keeps only +1 for compatibility; unary-minus zero has neighbors +1 and -1. Filter zero for slice steps in both directions. Thus -1 yields 0/-2 except steps yield only -2; -2 yields -1/-3.

## Implementation choice

Use i128 internally after parsing the magnitude as u64. This preserves the existing unsigned upper boundary, unlike replacing u64 with i64. Arbitrary-precision arithmetic would broaden scope and is unnecessary for this bounded extension. Restrict the lower neighbor to zero for unsigned spellings and to -u64::MAX for negative spellings. Read literal text from the operand AST range, not from the entire minus expression, so spaces/comments/parentheses remain valid input.

## Verification

Rust analyzer tests independently enumerate expected candidates for all four positions, signed zero, range endpoints and overflow, excluded spelling, annotations/store/delete, and parenthesized multiline input. Existing positive cases retain their candidates. A public CLI integration test uses CPython 3.14 to compile each plan replacement and verifies a tail-element test on [7, 7, 9, 7]: unary sign -1 to +1 survives, while neighboring -2 is killed. Baseline must pass and original source bytes must remain unchanged. Test command and subprocesses use bounded hoimin timeouts.

## Evidence boundary

Base: 8b33167. The prior structural operator design excluded negatives; this issue deliberately extends that contract. No Lean model or Python production code changes are needed. CPython compiler and real verify evidence apply to the exercised fixtures and host; they do not prove all Python expression semantics or native resource enforcement on other systems.
