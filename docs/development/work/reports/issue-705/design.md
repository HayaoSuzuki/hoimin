# Top-level condition clause deletion (#705)

Opt-in condition_clause_delete handles only complete if/elif tests that are BoolOp
with at least two operands. Remove one top-level operand per candidate. Reconstruct
remaining operand text in parentheses joined by the original and/or, then wrap the
complete result. Operand-internal source/comments remain; inter-operand formatting
is normalized. Wrapping preserves mixed nesting and keyword adjacency. Do not bool()
coerce operands. Evaluation order and Python short circuit of retained operands remain;
deleted expressions and their effects disappear. Exclude the whole condition if it
contains Named/Await/Yield/YieldFrom. Shared retention deduplicates identical edits.

## Design self-review

1. Eligibility: only statement if/elif roots; nested BoolOp is retained as one operand.
2. Syntax: each raw AST operand receives parentheses, including lambda and multiline
   operators; outer wrapping prevents joining with if/elif keywords.
3. Meaning: retain the same operator and operand order, but removing a short-circuit
   operand can expose effects of later expressions; document this deliberately.
4. Resources: emit one replacement at a time, polling cancellation in both loops;
   shared bounded prefix and identity deduplication avoid a new unbounded candidate list.
5. Proof: Lean list deletion/order and two-operand Boolean witnesses are model-only;
   CPython executes concrete mixed-expression mutants to check correspondence.

Adjacent equal operand source text is skipped before constructing replacements.
At one fixed span/operator/scope the retention order is emission order, so generation
stops after max_candidates + 1 distinct edits. This still supplies the overflow witness
and preserves the shared prefix, even when other operators compete for retention.
