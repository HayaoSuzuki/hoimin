# Issue 258 Token Syntax-Role Gating Design

## Problem

The Rust Python analyzer currently chooses several mutations from token text
alone. Python reuses tokens such as `in`, `*`, and `|` for both expression
operators and grammar constructs. The analyzer consequently emits replacements
for loop and comprehension `in`, star imports and unpacking, and match OR
patterns. Those replacements can be invalid Python and are then reported as
killed mutants rather than rejected candidates.

The candidate span validator cannot solve this problem: it verifies source
coordinates and fingerprints, but it does not know the token's grammar role.
Reparsing every candidate in production would detect the symptom only after
constructing the wrong candidate and would multiply parser work by the number
of candidates.

## Goals

- Emit raw-token mutations only for tokens that an AST node identifies as a
  supported expression or statement operator.
- Preserve the existing candidate IDs, spans, source ordering, filtering, and
  comment/trivia behavior for valid operators.
- Exclude loop/comprehension membership syntax, imports and unpacking, and
  match-pattern alternation from unrelated operator families.
- Prove in analyzer tests that emitted token candidates produce parseable
  Python.

## Non-goals

- Reparsing every candidate in the production analyzer.
- Moving collection, structural, type, or exception operators into the token
  pass.
- Broad semantic viability analysis beyond identifying the Python syntax role.
- Changing operator names, profiles, defaults, or report schemas.

## Considered approaches

### Recommended: AST-derived token-start allowlist

During the existing `AstFacts` traversal, record the start offsets of tokens
that belong to supported AST roles. For binary, boolean, comparison, unary, and
augmented-assignment nodes, search only the node's source range for the token
spellings valid for that node category. Record boolean literals and
`break`/`continue` from their exact AST ranges. The token pass then requires
the current token start to be in this allowlist before applying its replacement.

This retains the existing token-aware replacement code, including composite
`not in` and `is not` spans, while adding a grammar-derived eligibility gate.
Nested AST nodes may record the same offset; a set naturally deduplicates it.

### Alternative: emit every operator directly from AST nodes

Dedicated AST branches could construct every candidate. This provides strong
typing but duplicates token location logic for operators whose AST stores only
an enum, especially chained and composite comparisons. It is a larger rewrite
with more trivia and span regression risk.

### Alternative: reparse each replacement

Applying and reparsing every candidate would filter syntax errors but would not
prevent grammar-role mistakes that remain parseable. Its parser cost also grows
with candidate count. Parseability belongs as a test invariant, not the primary
production classifier.

## Architecture

`AstFacts` gains an `operator_token_starts` set. Helper methods inspect parser
tokens within a bounded AST range and record starts only when token text belongs
to the node's allowed spellings:

- `Expr::Compare`: equality, ordering, membership, and identity spellings;
- `Expr::BoolOp`: `and` and `or`;
- `Expr::BinOp`: supported arithmetic and bitwise spellings;
- `Expr::UnaryOp`: unary `+`, unary `-`, and `not`;
- `Stmt::AugAssign`: `+=` and `-=`;
- `Expr::BooleanLiteral`, `Stmt::Break`, and `Stmt::Continue`: their exact
  leading token.

The existing annotation exclusion remains in place so annotation unions and
shifts do not become runtime mutations. Existing `not_operands` and
`unary_sign_starts` facts continue to provide the replacement span and unary
classification after eligibility is established.

The raw-token loop rejects any replacement-bearing token whose start is not in
the AST-derived set. Composite operators are gated by their first token, which
is the start of the candidate span.

## Error handling and compatibility

Invalid input continues to produce the existing invalid-syntax diagnostic.
Failure to associate a token with a supported AST role is conservative: no
candidate is emitted. No new diagnostics or manifest fields are introduced.

Valid operator candidates keep their current byte spans and replacements, so
candidate fingerprints remain stable. The change intentionally removes only
candidates whose token was serving an unsupported grammar role.

## Testing

Analyzer regression tests will combine invalid roles and nearby valid
expressions, asserting that:

- loop/comprehension `in`, star import, positional/keyword unpacking, and match
  OR-pattern tokens are not emitted;
- valid comparison, membership, identity, boolean, arithmetic, bitwise, unary,
  augmented-assignment, boolean-literal, and break/continue mutations remain;
- every emitted token-family candidate can be applied and reparsed;
- chained and composite comparisons preserve their exact spans;
- annotations retain their existing bitwise exclusion.

Focused analyzer tests, the full Rust workspace suite, formatting, Clippy, and
the repository's Python contract tests form the merge gate.
