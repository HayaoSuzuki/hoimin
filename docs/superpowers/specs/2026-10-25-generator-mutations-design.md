# Generator behavior mutations

The user approved four generator-related operators as a subsequent PR. Add
`list_comprehension_to_generator`, `generator_return_insert`, `generator_skip_first`
and `generator_empty`, default-enabled and individually removable (opt-out).
Defaults become61 operators and the complete catalog80, including parent features.

- List comprehension→generator expression: replace square delimiters with
  parentheses, preserving the interior source, including async comprehensions and
  Python3.15 unpacking syntax. This tests one-shot iteration, truthiness and lazy
  evaluation. Do not exclude empty lists: their truthiness change is useful.
- Generator return insertion: insert bare return after an optional docstring in a
  function with own-scope yield/yield-from, including async generators. Retain all
  original yield source so the function remains a generator. Skip an existing
  first return and docstring-only/non-generator functions. Nested yields do not
  make an enclosing function a generator; evaluated defaults can.
- Skip first: wrap a known synchronous Generator expression with a private lambda
  that calls builtin next once with default None, then returns the same generator.
  This eagerly consumes its first item at construction. Empty dynamic inputs stay
  empty without introducing StopIteration. Require static builtin-next resolution.
- Empty: construct the original synchronous Generator expression, then replace
  its result with an empty generator. Preserve the leftmost iterable's construction
  and evaluation once; do not drain any input. This does not reproduce side effects
  of actually consuming a whole stream. Skip-first/empty exclude syntactically
  known-empty list/tuple iterables and async generator expressions.

Approaches: arbitrary iterator names require broad flow/alias/type inference;
explicit Generator AST expressions establish the object kind locally. Full draining
can fail to terminate and does unnecessary work; use one next call or an empty
replacement. Generators may still run arbitrary user code during next: existing
process/resource budgets apply. A surviving mutant is evidence to inspect, not a
proof of a production defect or swallowed exception.

Use wrappers that retain and parenthesize the original fragment; implicit sole-call
argument Generator ranges may omit parentheses. Empty wrapper:
`((original), (_ for _ in ()))[1]`. Skip wrapper:
`(lambda __hoimin_iterator, __hoimin_next: (__hoimin_next(__hoimin_iterator, None), __hoimin_iterator)[1])((original), next)`.
The lambda parameter is local to the wrapper; the original expression is evaluated
as its argument, in its original scope.

Share function-entry source layout and own-scope inspection with iterator_stop.
Preserve global-declaration exclusion and CR/LF/CRLF/semicolon/docstring behavior.
For next proof, record range-keyed construction facts after returning to parent
scope. Start sites can collide with an implicit generator's first element; nested
generators can also share an end offset across different scopes. Keep construction
facts separate from ordinary names, retaining existing loop-back uncertainty. Track next alongside
inserted builtin names, outside the existing exception-pair catalog. Class, generic,
wildcard, dynamic and loop binding uncertainty must still suppress unsafe proof.

Preserve cancellation, selection, candidate ordering/prefix and payload limits.
Avoid building retained-sized replacement strings for unselected expressions.
No generic return/Exception/BaseException insertion, arbitrary iterator alias
tracking, full draining or async next insertion in this PR. Validate all four with
CPython3.14/3.15, perform five self-reviews each stage and one fresh final review,
then create/attach a PR based on iterator-stop-insert.

The proven builtin `next` is passed from the construction scope after evaluating the original generator, avoiding different lexical lookup inside the wrapper lambda.
