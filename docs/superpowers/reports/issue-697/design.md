# Integer literal neighbors (#697)

Expose opt-in integer_literal_neighbor. Accept unsigned decimal digit spellings
and unary-minus decimal expressions, magnitudes through u64::MAX. For mathematical
value n emit n-1 then n+1, omitting neighbors outside [-u64::MAX, u64::MAX]. Wrap
all replacements in parentheses: (-1) must stay a power base, and (1).attribute
must remain valid. Do not rewrite the positive operand again inside a negative
literal. Exclude bool, nondecimal/underscored/float/complex spellings, annotations,
alias type expressions, match patterns, store/delete targets and the entire slice
expression of a subscript. Existing structural operators keep their own rules.

Use the existing decimal parser and candidate pipeline. A contextual traversal
flag prevents generic literal mutations in subscript slices/targets. Continue
walking those nodes for existing operators. Alternatives: token replacement loses
signed-expression precedence; arbitrary precision expands scope unnecessarily.

## Design self-review

1. Scope: range/call/return/assignment values included; declaration-only types and
   match patterns excluded by existing facts plus explicit pattern guard.
2. Signed syntax: unary-minus expression owns its number; suppress child duplicate.
3. Precedence: always parenthesize, including positive replacements before dot.
4. Bounds: use i128 arithmetic over u64 magnitudes; omit out-of-domain neighbors,
   never wrap; 0 includes -1 unlike legacy unsigned index rules.
5. Integration: opt-in, Arithmetic rank; suppress entire subscript slice expression
   including nested arithmetic/calls, not only direct integer subscripts.

Lean proves the mathematical neighbor and bound properties in a model; Rust AST
correspondence, byte preservation and Python compilation are separate tests.

## Formal correspondence and limits

| Premise / observation | Lean | Rust boundary | Mode |
| --- | --- | --- | --- |
| Mathematical value | Int n | decimal_literal_value returns i128 | model-only |
| Supported magnitude | Int limit | u64::MAX converted to i128 | model-only |
| Retained neighbors | filtered list | two checked in-domain replacements | model-only |

Lean IntegerLiteralNeighbor proves adjacency, bounds and changed value for every
retained model neighbor. Three finite boundary examples and an unsigned-zero
broken-rule witness check sensitivity. Limit 3 is a model-only reduction, not
production's u64 limit. Syntax, contextual exclusion and AST-to-number translation
are not proved by these theorems. Atomicity/replay are inapplicable to this pure
map/filter; duplicate candidate identity remains the shared pipeline's contract.
External deadline 20s; observed Lean exit 0 in 2.492s, no expensive enumeration.

Final CLI order follows existing ranking/ID tie-breakers, not numerical neighbor
order. Tests compare expected membership and separately require repeated plans to
have identical ordered candidate arrays.
