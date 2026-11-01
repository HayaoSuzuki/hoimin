# Issue 696: call statement deletion

Only independent `Expr(Call)` statements become `pass`. The whole AST statement
range is replaced so single-line suites and semicolon neighbours stay intact.
Reject any nested named expression, await, yield or yield-from, including inside
lambda arguments. This intentionally conservatively protects binding and generator
classification. Assignment, return, import and compound statements are unchanged.

Register `statement_delete` as opt-in, retain existing default sets and candidate
identity rules, and use AstCandidateCollector.add_candidate for selection/profile,
source bytes, bounded ordering and deduplication. Check cancellation during the
eligibility walk. No Python execution is used in production discovery.

Alternatives considered: deleting bytes entirely breaks an otherwise empty suite;
replacing all statement kinds can change lexical bindings. Neither is selected.

Lean models eligibility and replacement semantics only. It does not prove Ruff,
the Rust implementation, Python parsing, encodings or OS execution. Rust/public CLI
regressions separately exercise those boundaries. No paper effectiveness claim.

## Design self-review (five passes, before implementation)

1. AST scope: confirmed Expr(Call), not any expression; nested await/yield excluded.
2. Lexical bindings: found walrus removal can change local-name classification;
   added Named exclusion even though the original proposal primarily names await/yield.
3. Span boundaries: use statement rather than function/call token span; preserve
   indentation, semicolons and trailing comments outside it.
4. Integration: add only to canonical registry, not all_legacy/default; reuse shared
   candidate filtering instead of maintaining a separate selection path.
5. Proof boundary: eligibility model is model-only; syntax and observed side effects
   require independent CPython 3.14 and saved-plan verification.

## Formal correspondence worksheet

| Premise/observation | Lean | Production evidence | Mode |
| --- | --- | --- | --- |
| Independent call statement | callStatement | Stmt::Expr + Expr::Call match | model-only |
| Nested binding/suspension nodes | effects | cancellable RemovalCheck visitor | model-only |
| Prefix/suffix stay outside edit | string concatenation | CLI byte-span/CPython regressions | model-only |

Three kernel-checked theorems cover non-call exclusion, ordinary effects for
accepted statements, and unchanged neighbours for a single replacement.
Five finite examples cover ordinary/named/await/yield/yield-from. The deliberately
broken outer-node-only gate is distinguished by the one-node yield witness.
Atomicity and replay families do not apply to this pure eligibility model;
uniqueness is provided by the existing candidate pipeline, not proved here.
The correspondence rows remain model-only: the Rust tests are independently
specified, not a Lean-generated corpus. No bounded test is called a proof.
