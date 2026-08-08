# Issue 265 Exception Handler Type Context Design

## Goal

Prevent the generic `collection_list_tuple` operator from changing exception
handler type tuples into lists while preserving ordinary collection mutations
and the dedicated exception mutation operators.

Issue #265 is a correctness fix. Python accepts an exception class or a tuple
of exception classes in `except` and `except*` clauses, but a list replacement
fails when the handler is matched. Emitting that replacement creates an
artificially killed mutant and can inflate mutation scores.

## Scope

This change covers the type expression of every parsed `except` and `except*`
handler, including nested handlers and parenthesized tuple expressions. It
changes only AST collection-shape candidates produced by
`collection_list_tuple`.

The following remain unchanged:

- ordinary list and tuple calls and literals;
- list and tuple literals in handler bodies;
- token operators found in ordinary runtime expressions;
- `exception_type_pair` and the explicitly selected risky exception
  operators;
- candidate descriptors, IDs, ordering, deduplication, profiles, limits, and
  diagnostics;
- parser support and syntax diagnostics.

No dependency, CLI option, report field, plan schema, or operator ID is added.

## Considered Approaches

### 1. Collector-local exception-type context (selected)

`AstCandidateCollector` records whether it is currently walking an exception
handler type expression. Both ordinary and starred handlers enter this context
before walking their type and leave it before walking the handler body. The
list and tuple literal collectors decline `collection_list_tuple` candidates
while the context is active.

This is the smallest representation of the Python syntax role. It avoids a
new global range query, preserves traversal into nested expressions, and does
not broaden the suppression to unrelated operators.

### 2. Record handler-type ranges in `AstFacts`

Range facts would allow any producer to query the handler type role. This is
unnecessary for the current AST-only bug and would add another linear range
lookup immediately before Issue #267 replaces those lookups with indexes.

### 3. Skip handler-type traversal

Skipping the type expression entirely would prevent the invalid candidate,
but it would also make the policy implicit and could suppress present or
future syntax-directed exception candidates. The selected approach preserves
the traversal and gates only the generic collection-shape operator.

## Architecture

### Exception-type traversal helper

`AstCandidateCollector` gains a private context depth rather than a boolean.
A depth remains correct if a future visitor path nests a type-context walk and
prevents an inner exit from clearing an outer context.

A helper walks a handler type with balanced entry and exit:

```rust
fn visit_exception_type(&mut self, expression: &'ast Expr) {
    self.exception_type_depth += 1;
    self.visit_expr(expression);
    self.exception_type_depth -= 1;
}
```

The implementation must not expose the depth publicly. Checked addition is
not required for a parser-bounded AST traversal, but the exit must remain
balanced even as the visitor structure evolves.

### Ordinary `except`

`visit_except_handler` continues to collect dedicated exception candidates
before recursively walking the handler. Instead of delegating the whole node
to the generic Ruff walker, it explicitly:

1. walks the optional handler type through `visit_exception_type`;
2. visits the handler body normally.

The optional `as name` binding is already recorded by `AstFacts`; the
candidate collector does not need to revisit it as an expression.

### `except*`

The existing `Stmt::Try` starred-handler branch already walks handler types and
bodies explicitly to keep dedicated risky exception operators disabled. It
must use the same `visit_exception_type` helper for the type expression. This
keeps the generic collection rule identical for `except` and `except*` while
retaining the existing dedicated-operator policy.

### Candidate gate

`collect_list_literal` and `collect_tuple_literal` keep their existing load
context and annotation checks and add the exception-type context check. Other
collection and structure collectors are not changed by this issue.

The gate is local to candidate creation. Traversal continues so nested
expressions and cancellation checks retain their existing behavior.

## Behavioral Examples

No generic tuple-to-list candidate is emitted for either handler:

```python
try:
    work()
except (ValueError, TypeError):
    recover()

try:
    work_group()
except* (ValueError, TypeError):
    recover_group()
```

The tuple in the body remains eligible:

```python
try:
    work()
except (ValueError, TypeError):
    fallback = (left, right)
```

When explicitly selected, supported dedicated exception replacements inside
the handler type remain eligible because they are collected before the generic
type-expression walk.

## Testing

### Analyzer regressions

Tests construct literal expected candidates and prove:

- tuple handler types in `except` emit no `collection_list_tuple` candidate;
- tuple handler types in `except*` emit no such candidate;
- parenthesized and nested handler types are covered;
- ordinary list/tuple literals before, after, and inside handler bodies remain
  eligible;
- safe and risky dedicated exception candidates retain their exact spans,
  replacements, and ordering;
- every retained replacement still reparses.

The production change that makes these tests fail is removal or misapplication
of the exception-type context gate. Expected values are hand-written rather
than produced by analyzer helpers.

### Production CLI regression

A production-binary integration test uses a small project whose test command
raises an exception and executes the tuple handler. Discovery is restricted to
`collection_list_tuple`. The inventory must contain the nearby ordinary
collection candidate and must not contain the handler-type range. The run then
executes the handler path through the real CLI and produces a parseable final
report without the invalid handler mutant.

This test observes Hoimin's candidate and report behavior. It does not assert
Python's independently documented rule merely by executing a hand-written
list handler.

### Verification

Before the PR is created, run:

- focused analyzer unit and integration tests;
- the focused production CLI regression;
- `cargo fmt --all -- --check`;
- workspace Clippy with warnings denied;
- `cargo test --workspace`;
- frozen Python unittest discovery;
- `git diff --check`.

## Documentation and Delivery

README operator guidance states that generic list/tuple literal mutations do
not apply to exception handler type positions. `docs/development.md` records
the collector-context invariant for future analyzer changes.

The design, implementation plan, production changes, tests, and documentation
live in the Issue #265 worktree and one pull request. Pure documentation
commits use `[skip ci]`; the final branch contains a non-skip commit so GitHub
Actions validates the complete tree. The PR contains `Closes #265` and is
squash-merged after every required check succeeds.
