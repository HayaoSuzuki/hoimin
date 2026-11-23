# Mutation operator reference

Use this reference to interpret a candidate and decide what behavior your tests
should distinguish. Find an ID below, or use the generated
[selection index](cli.md#operator-selection) to check its default state and family
membership. The [family index](cli.md#selector-families) lists exact expansions.
Topic headings on this page are not additional CLI selectors.

For selection commands and release-specific documentation, see
[choose mutation operators](../guides/usage.md#mutation-operators).
Reading help and documentation does not import your project or run its tests.

## Reading the examples

An arrow shows one possible before/after change, not all candidates. Code in
backticks is a Python fragment; supply the surrounding function, imports or values
described in the entry. A **behavior example** describes the result of a structural
rewrite without promising its exact emitted source. Use `hoimin plan` and
`hoimin verify plan.json --top 10 --dry-run` to inspect actual candidates in a saved plan.

Each mutant applies one source replacement. A replacement can contain a wrapper
or span several statements. Other operators can produce candidates at the same
location. Selection, candidate caps, inline exclusions and conservative analysis
can further restrict the examples below.

A survivor means that the configured command accepted the changed program. It
does not establish a bug or an equivalent mutant. Compare the altered behavior
with your specification. An exception caused by a mutation can be useful evidence,
but does not by itself show that a test is missing.

## Shared applicability rules

Runtime operators target parsed syntax, not matching text in comments or strings.
Type annotations and explicit type-alias positions are excluded where a runtime
edit would instead change a type expression. Annotation operators have their own
section below. Individual entries describe additional restrictions.

Builtin call changes require the relevant names to resolve to builtins; shadowed
or ambiguous bindings suppress candidates. Standard-library changes require
recognized imports and supported aliases. The analyzer does not execute imports.
Its static evidence is not a guarantee against external monkey patching or every
dynamic alias. Qualified calls, starred arguments and keyword forms are supported
only where the individual analyzer can preserve their contract.

Older collection and structural method changes are syntax-directed: a method
named `sort` need not belong to a list. The `set_union_intersection_swap` operator
instead requires evidence of a builtin set. Read the preview before interpreting
a method mutation as a change to a particular runtime type.

Edits that remove an expression can remove its effects deliberately. Some use
deletion guards to exclude binding or suspension constructs such as assignment
expressions, `await` and `yield`; others retain unreachable syntax to preserve
compiler-visible facts. Neither approach treats ordinary calls as pure. Entries
below describe each policy. Sites without room for required grouping are excluded
to respect Python's delimiter-depth limit.
The [development guide](../../development/guides/contributing.md#extending-collection-and-structural-mutations)
provides implementation details and links to the analyzer tests.

## Comparisons, arithmetic and Boolean expressions

### `compare_eq_ne`

`a == b` → `a != b`, and the reverse. Probe equality distinctions, including
overloaded comparisons. Applies to comparison syntax, not assignment or keyword
arguments; changes one comparison token in a chain.

### `compare_order`

`a < b` → `a <= b`; also exchanges `>` and `>=`. Probe equality at an ordering
boundary. Operands remain in place; this does not reverse the ordering direction.

### `membership`

`item in values` → `item not in values`, and the reverse. Probe membership and
absence. Applies to comparisons, not the `in` keyword of a loop or comprehension.

### `identity`

`a is b` → `a is not b`, and the reverse. Probe object identity or sentinel
handling. This does not replace identity with equality.

### `boolean_and_or`

`a and b` → `a or b`, and the reverse. Probe combined conditions and short-circuit
effects. Python returns an operand, so non-Boolean results can change too.

### `binary_add_sub`

`a + b` → `a - b`, and the reverse. Probe addition versus subtraction in binary
expressions, including overloaded operations. Unary signs use `unary_sign`.

### `augmented_add_sub`

`total += value` → `total -= value`, and the reverse. Probe accumulated state.
Applies to augmented assignment; ordinary assignment is not a candidate.

### `binary_mul_div`

`a * b` → `a / b`, and the reverse. Probe scaling, result types and zero divisors.
Applies to binary syntax, not multiplication inside string text.

### `augmented_mul_div`

`total *= value` → `total /= value`, and the reverse. Probe in-place scaling and
state updates. Result types and overloaded in-place dispatch can change.

### `binary_floor_mod`

`a // b` → `a % b`, and the reverse. Probe quotient versus remainder, particularly
for negative values. Applies to binary expressions.

### `augmented_floor_mod`

`total //= value` → `total %= value`, and the reverse. Probe retained quotient or
remainder in an augmented assignment; it does not add a `/=` mutation.

### `binary_div_floor`

`a / b` → `a // b`, and the reverse. Probe rounding, negative inputs, result types
and overloaded division. Evaluates each operand once in source order. Augmented
assignments are excluded; `*`/`/` and `//`/`%` remain separate operators.

### `unary_sign`

`-value` → `+value`, and the reverse. Probe sign and unary-method behavior.
Applies to unary syntax; binary subtraction has its own operator.

### `remove_not`

`not ready` → `ready`. Probe negated conditions and the distinction between a
Boolean result and the original operand. Does not remove the `not` in `not in`.

### `boolean_literal`

`True` → `False`, and the reverse. Probe literal decisions and flags, including
Boolean match patterns. Mapping-pattern key edits that duplicate a sibling key
are excluded, including equality between `True` and `1`.

### `binary_power`

`a ** b` → `a * b`. Probe exponentiation rather than multiplication. Binary power
only; this does not change unpacking syntax.

### `binary_matmul`

`a @ b` → `a * b`. Probe matrix multiplication versus elementwise or ordinary
multiplication. Applies to binary expressions, not decorators.

### `augmented_power`

`value **= exponent` → `value *= exponent`. Probe exponentiation in an update.
The target remains an augmented-assignment target.

### `augmented_matmul`

`value @= matrix` → `value *= matrix`. Probe in-place matrix multiplication.
The change can dispatch a different overloaded method.

### `bitwise_and_or`

`a & b` → `a | b`, and the reverse. Probe masks and overloaded set-like operations.
Boolean keywords and type-union annotations are not this operator's target.

### `bitwise_shift`

`a << b` → `a >> b`, and the reverse. Probe shift direction. Applies to binary
syntax; operands and their evaluation order are retained.

### `bitwise_xor`

`a ^ b` → `a & b`. Probe exclusive combination versus intersection. There is no
reverse `&` → `^` candidate from this operator.

### `bitwise_invert`

`~value` → `+value`. Probe bitwise inversion. The replacement can invoke unary
positive on an overloaded object; it is not necessarily an identity operation.

### `augmented_bitwise_and_or`

`mask &= value` → `mask |= value`, and the reverse. Probe mask updates. Applies
to augmented assignment, including overloaded in-place methods.

### `augmented_bitwise_xor`

`mask ^= value` → `mask &= value`. Probe exclusive updates versus intersection.
Other augmented operators are not expanded by this ID.

### `augmented_bitwise_shift`

`mask <<= count` → `mask >>= count`, and the reverse. Probe shifted state.
Applies to augmented assignment only.

### `augmented_to_assignment`

`total += value` → `total = value`. Probe whether the previous value contributes
to an update. Requires a simple-name target; attribute and subscript targets are
excluded. The right-hand expression is preserved.

### `operator_function`

`operator.add(a, b)` → `operator.sub(a, b)` is one example. Trusted callable
references can also change when passed to another function. Covers the supported
Python 3.14 `operator` comparison, numeric, truth, identity, in-place and sequence
callables. Some changes use wrappers: `contains` negates membership, `getitem`
becomes containment, and `setitem`, `delitem` or `call` can suppress the operation
while still evaluating supplied arguments. Requires a unique unmodified supported
import. Excludes `attrgetter`, `itemgetter`, `methodcaller` and `length_hint`;
callable identity and introspection are not preserved. See the
[implementation mapping](../../development/guides/contributing.md#extending-collection-and-structural-mutations).

## Collections, indexing and slicing

### `collection_any_all`

`any(values)` → `all(values)`, and the reverse. Probe empty, mixed and uniformly
truthy/falsy iterables, including short-circuit consumption. Requires one explicit
positional argument and resolved builtin names.

### `collection_list_tuple`

`list(values)` → `tuple(values)`, or `[a, b]` → `(a, b)`, and the reverse. Probe
mutability and sequence type. Only supported constructor calls and load-context
literals qualify; exception-handler type positions and assignment targets do not.

### `collection_set_frozenset`

`set(values)` → `frozenset(values)`, and the reverse. Probe mutability and
hashability through supported builtin constructor calls. Does not wrap set literals
in a `frozenset` call.

### `collection_append_insert`

`items.append(value)` → `items.insert(0, value)`, and the reverse supported shape.
Probe front versus back insertion. Requires explicit positional arguments; arbitrary
insertion positions and an `append`/`pop` exchange are not supported.

### `collection_min_max`

`min(values)` → `max(values)`, and the reverse. Probe which extreme is selected.
Requires resolved builtins and a supported argument shape; it does not rewrite
user-defined methods with those names.

### `collection_set_add_discard`

`items.add(value)` → `items.discard(value)`, and the reverse. Probe addition versus
removal. Requires a one-positional-argument method call; receiver type is not inferred.

### `collection_set_remove_discard`

`items.remove(value)` → `items.discard(value)`, and the reverse. Probe behavior
when an element is absent. Requires one positional argument; a method name does
not prove that the receiver is a builtin set.

### `collection_string_starts_ends`

`text.startswith(prefix)` → `text.endswith(prefix)`, and the reverse. Probe which
end is checked. Supported positional method arguments are retained; the analyzer
does not infer that the receiver is a string.

### `collection_string_split_rsplit`

`text.split(sep, 1)` → `text.rsplit(sep, 1)`, and the reverse. Probe which separators
are consumed when splitting is bounded. Requires an explicit `maxsplit`, positional
or keyword; an unbounded `split(sep)` is excluded.

### `structure_append_extend`

`items.append(value)` → `items.extend([value])`, and the reverse. Probe the method
contract, including custom receivers. Reverse edits require a one-element list
without starred elements, not an arbitrary iterable. This can be equivalent for
an ordinary list.

### `structure_mapping_get_subscript`

`mapping.get(key)` → `mapping[key]`, and the reverse. Probe missing-key behavior.
Requires a simple-name or attribute receiver, one explicit key and a load-context
subscript. Calls with a default value, store/delete targets and computed receivers
are excluded.

### `structure_sort_reverse`

`items.sort()` → `items.reverse()`, and the reverse. Probe sorting versus reversing
the current order. Only calls without arguments qualify; no receiver type is inferred.

### `structure_sorted_reversed`

`sorted(items)` → `reversed(items)`, and the reverse. Probe ordering and result
kind. Requires resolved builtins and one explicit positional argument without
keywords. An iterable accepted by `sorted` need not support `reversed`.

### `structure_index_neighbor`

`items[-1]` → `items[0]` or `items[-2]`. Probe neighboring positions in a
load-context subscript. Integer literals retain direct source edits, including
decimal, hexadecimal, octal and binary spellings, separators and large integers.
Literal zero without a unary minus produces only the positive neighbor.
Named and computed indices shift exact integers directly. Other integer-index
objects use a wrapper that shifts their value when the receiver requests `__index__`;
values without that protocol pass through. Receiver and index expressions remain in
their original scope and evaluation order. This requires resolved builtin helpers.
Annotation, assignment and deletion targets are excluded.
Line selection uses the index expression's position. Computed edits replace the
whole subscript, so their reported edit location can precede the selected line.

### `structure_slice_neighbor`

`items[:-1]` → `items[:0]` or `items[:-2]`. Probe slice boundaries and strides.
Supports integer literals and computed lower, upper and step expressions, with
the same context exclusions as `structure_index_neighbor`. Computed positions
retain Python's deferred `__index__` conversion order; runtime `None` remains an
omitted bound. A step is never changed
to zero: `items[::-1]` produces a `-2` step, not a zero step. Custom receivers can
observe the wrapper object; inspect those candidates against their own contract.

### `structure_slice_bound_omit`

`items[a:b]` → `items[:b]` or `items[a:]`. Probe whether each limit matters.
Requires a one-dimensional load-context slice without an explicit step. Bounds
must be names or integer literals, including grouped negative integers. Calls,
attributes, compound bounds, annotations and assignment/deletion targets are excluded.

### `structure_slice_bound_move`

`items[:n]` → `items[n:]`, and the reverse. Probe prefix versus suffix selection.
Uses the same restrictions as bound omission, but requires exactly one bound.
`items[:]` and two-bound slices do not qualify.

### `container_element_delete`

`[a, b]` → `[b]` is one candidate. Probe required members in supported list,
parenthesized tuple and dictionary displays. Set displays are excluded.
Removes one element or entry; starred/unpacked
elements, assignment targets and unsafe-to-remove expressions are excluded.
Comprehensions are not container displays.

### `comprehension_filter_remove`

`[x for x in xs if allowed(x)]` → `[x for x in xs]` in behavior. Probe whether
filtering is necessary. Supports list, set, dictionary and generator comprehensions;
replaces one filter predicate with `True`, keeping its loop. Boolean-literal
predicates and predicates containing assignment expressions are excluded.

### `comprehension_filter_invert`

`[x for x in xs if allowed(x)]` → `[x for x in xs if not allowed(x)]` in behavior.
Probe accepted versus rejected elements. Supports the same comprehension forms;
the predicate still evaluates once for each item that reaches that filter.

### `filter_predicate_swap`

`filter(predicate, values)` → `itertools.filterfalse(predicate, values)` in
behavior, and the reverse. Probe a lazy selection and its complement, including
`None` predicates. Requires a resolved builtin/import and two explicit positional
arguments. The replacement preserves laziness; it does not precompute a list.

### `set_union_intersection_swap`

`{1, 2}.union(other)` → `{1, 2}.intersection(other)`, and the reverse. Probe
combination versus common membership. Requires a confirmed builtin set or
frozenset receiver and supported arguments. Unlike older method operators, an
arbitrary object's `union` method does not qualify. Receiver type determines result type.

### `aggregate_sum_count`

**Behavior example:** `sum(values)` → the number of consumed items; `len([1, 2])`
→ the sum of those numeric elements. Probe count versus total. A one-argument
builtin `sum` consumes its original iterable once when replaced. The reverse
direction requires an explicit numeric list or tuple; arbitrary `len(obj)` and
`sum(values, start)` are excluded.

## Statements, conditions and function results

### `statement_delete`

`notify(value)` as a standalone statement → `pass`. Probe required call effects.
Only expression statements whose value is a call qualify; assignments and return
statements do not. Binding/suspension deletion guards apply.

### `integer_literal_neighbor`

`count = 3` → `count = 2` or `count = 4`. Probe nearby constants in eligible runtime
integer expressions. Supports decimal, hexadecimal, octal and binary spellings,
digit separators, unary plus/minus and arbitrary-size integers. Zero produces
`-1` and `1`. Index/slice interiors, patterns, annotations and assignment targets
do not qualify. Index/slice neighbors use their own operators and guards.
Large decimal increments can use an exact power of ten to stay within Python's
decimal literal digit limit.

### `condition_constant`

`if ready:` → `if (True):` or `if (False):`. Probe whether both paths matter.
Supports eligible `if`/`elif` tests and conditional expressions such as
`a if ready else b`. Existing Boolean literals are excluded. Statement conditions
use the expression deletion guards. Conditional-expression edits retain the
original test in unreachable syntax to preserve compiler bindings and free reads.

### `condition_clause_delete`

`if ready and permitted:` → `if permitted:` is one candidate. Probe each clause's
contribution to an `and`/`or` expression, including `while` conditions and ordinary
value expressions. Removes one operand,
preserving the others and their order. Removing a clause can expose previously
short-circuited effects. Direct `if`/`elif` conditions use deletion guards for
binding and suspension constructs and exclude nested Boolean conditions within
the same short-circuit expression. Other sites keep the original expression in a dead
branch, preserving lexical facts without evaluating the removed clause.

### `while_condition_false`

`while pending:` → `while (False):`. Probe whether loop execution matters.
Skips evaluation of the original condition and runs an existing `else` suite.
Existing Boolean-literal conditions and
conditions rejected by deletion guards are excluded.

### `function_body_erase`

**Behavior example:** a synchronous function that performs work without returning
a value → the same function with `pass` after its docstring. Probe whether its
effects are observed. Excludes asynchronous functions, protocol methods,
generators, non-None value returns and already empty/no-op bodies.
The signature and docstring remain.

Line selection uses the first erased statement after the docstring, not any
later line in the body; symbol selection uses the erased function. Preview
shows the whole replacement span, subject to the shared 2 MiB serialized-candidate
limit. A survivor does not establish whether the function ran or was observed.

### `function_body_return_constant`

**Behavior example:** `def size() -> int: return calculate()` → a body returning
`0` or `1`. Probe dependence on the computed result. Requires supported builtin
`bool`, `int` or `str` return annotations as direct unshadowed names, and a value
return. An annotation is a heuristic, not a runtime type guarantee. Uses `False`/`True`,
`0`/`1`, or `""`/`"A"` respectively. Excludes async functions, generators and
protocol methods; preserves docstrings and skips an identical sole literal return.
Body effects are intentionally removed along with the original computation.

### `return_tuple_swap`

`return key, value` → `return value, key`. Probe positional return contracts.
For larger tuples or effectful operands, exchanges each adjacent pair after
evaluating both elements in source order. Calls, attributes and nested values
qualify; pairs containing a starred element are excluded. Other tuple elements
keep their positions and evaluation order. Requires a synchronous nongenerator
function. Two distinct literal elements can use a direct source swap; names and
other evaluated operands retain their original evaluation order.

### `enum_member_replace`

`Status.READY` → `Status.DONE` when both are recognized members of the same local
enum. Probe branch distinctions between enum values. Requires an unconditional
top-level same-module `Enum`, `IntEnum` or `StrEnum` with one trusted base and
homogeneous integer/string literals or supported `auto()` calls. Aliases share a
canonical destination. Dynamic/ambiguous definitions, `Flag`/`IntFlag`, mixed
values, decorators/metaclasses, reserved attributes, integers exceeding `u64`
magnitude and unsupported string/auto names are excluded. Local/rebound names,
annotations, patterns and class-body self references do not qualify. Unsupported
definitions report `enum_definition_skipped`; analysis does not import the module.

### `string_literal_empty`

`label = "ready"` → `label = ""`. Probe required string contents. Applies to eligible
nonempty runtime string literals. Implicit concatenation, bytes, formatted/template
strings and their fields, docstrings, annotation/type-alias roles and other
protected string roles are excluded. String segments have a separate operator.

### `string_segment_empty`

`f"prefix:{value}"` → `f"{value}"` is one example. Probe literal segments while
keeping interpolated expressions. Produces a separate candidate for each nonempty
top-level literal segment of a supported concatenated or formatted string.
Protected string roles, interpolation/debug text and nested format
specification literals are excluded.

### `conversion_call_remove`

`int(value)` → `(value)`. Probe conversion, copying and validation. Supports the
builtin `int`, `float`, `complex`, `bool`, `str`, `bytes`, `list`, `tuple`, `dict`,
`set` and `frozenset` with one explicit positional argument, including standalone
call statements. Shadowed names, starred/keyword arguments, generators and expressions
with binding/suspension constructs are excluded.

### `stdlib_conversion_call_remove`

`Decimal(value)` → `(value)` for a resolved `decimal.Decimal` import. Probe
standard-library conversion. Supports explicit known constructors (including
`Fraction` and selected standard-library enum classes) and supported Decimal/
Fraction factory methods. Requires one positional argument;
ambiguous imports, escaping/rebound factory classes and unsafe argument shapes
are excluded. It does not remove arbitrary imported constructors.

The recognized Python 3.14 calls are:

| Module | Supported calls |
| --- | --- |
| `decimal` | `Decimal`, `Decimal.from_float`, `Decimal.from_number` |
| `fractions` | `Fraction`, `Fraction.from_float`, `Fraction.from_decimal`, `Fraction.from_number` |
| `http` | `HTTPStatus` |
| `socket` | `AddressFamily`, `SocketKind`, `MsgFlag`, `AddressInfo` |
| `signal` | `Signals`, `Handlers`, `Sigmasks` (Unix only) |
| `ssl` | `VerifyMode`, `VerifyFlags`, `Options`, `AlertDescription`, `SSLErrorNumber`, `TLSVersion` |
| `re` | `RegexFlag` |
| `calendar` | `Day`, `Month` |
| `inspect` | `BufferFlags` |
| `annotationlib` | `Format` |

A unique unconditional module-level import must establish identity; module and
direct-import aliases are supported. Availability and valid enum values follow
the target Python platform. Rebinding, local/conditional imports, escaped/modified
module objects and dynamic namespace access suppress candidates. Factory methods
also require their class not to escape its immediate constructor/factory callee;
aliases, extracted methods and annotation references can therefore suppress factory
candidates while leaving direct constructors eligible. External monkey patching,
import-path shadowing and arbitrary dataflow remain outside these static guarantees.
`typing.cast`, custom enums, enum base classes and `ctypes` are excluded.

Numeric equality can survive a removed conversion: `HTTPStatus(200)` compares
equal to `200`. Where your contract requires the enum type, assert that type as
well as its value. See the [design and verification record](../../development/work/maintenance/2026-10-13-stdlib-conversion.md).

### `method_call_remove`

`value.copy()` → `(value)`. Probe a no-argument method's result or effects,
including standalone call statements. Evaluates the receiver once while removing attribute
lookup and invocation, including descriptor effects. Arguments, starred forms
and receivers rejected by conversion deletion guards are excluded.

### `optional_keyword_delete`

`render(value, pretty=True)` → `render(value)` when the same-module function has
an optional `pretty` parameter. Probe whether an explicit option matters.
Requires a uniquely bound undecorated synchronous top-level function in the same
file and validates the original argument binding. Required parameters, calls
before the definition ends, escaped/aliased or rebound functions, dynamic namespace
access, variadics, type parameters, `__` names, unpacking and unsafe deleted values
are excluded. Uses the existing default object without evaluating the deleted
argument; argument reconstruction can normalize formatting and comments.

## Loops, iterators and generators

### `break_continue`

`break` → `continue`, and the reverse. Probe loop termination versus the next
iteration. Applies to parsed loop-control statements; it does not alter text in
strings or identifiers containing these words.

### `continue_remove`

`continue` → `pass`. Probe statements later in the same iteration. This can be
equivalent when no remaining work is reachable in that iteration.

### `loop_break_insert`

**Behavior example:** append `break` at the end of an eligible `for`, `async for`
or `while` body. Probe repeated iteration and loop-`else` behavior. Paths that
`continue`, return or raise before the insertion do not reach it. Direct body-level
`break`, `continue`, `return` or `raise` excludes a loop; conditional exits can
remain eligible. Supported inline and multiline suites qualify.

Reaching the insertion skips loop `else`; a conditional `continue` can bypass it,
so the mutation does not guarantee exactly one iteration. Nested loops receive
separate candidates. Line selection uses the insertion's final source anchor.

### `iterator_stop_insert`

**Behavior example:** enter an eligible `__next__` method and immediately
`raise StopIteration()`. Probe nonempty iterator behavior. Requires the supported
synchronous protocol method and resolvable stop exception. Decorated methods,
generator bodies, relevant global declarations and an already immediate matching
stop raise are excluded; docstrings are preserved.

### `async_iterator_stop_insert`

**Behavior example:** enter an eligible async `__anext__` method and immediately
`raise StopAsyncIteration()`. Probe asynchronous iteration. Uses protocol-entry
guards analogous to `iterator_stop_insert`; synchronous `__anext__` and async
generator bodies are excluded.

### `list_comprehension_to_generator`

`[f(x) for x in xs]` → `(f(x) for x in xs)`. Probe eagerness, reusable results
and container type. Targets list-comprehension syntax, not an arbitrary list
variable. Deferred effects and error timing can change.

### `generator_return_insert`

**Behavior example:** insert an immediate `return` at the entry of an eligible
generator function while retaining its yield-containing body. Probe whether output
is consumed. This preserves generator classification; it does not turn the
function into an ordinary function returning `None`. Requires a yield in the
function's own scope; synchronous and asynchronous generators can qualify.
Preserves a docstring and skips a body already starting with `return` after it.

### `generator_skip_first`

**Behavior example:** construct `(x for x in xs)`, consume its first item if one
exists, and return the remaining generator. Probe first-item handling and timing
of effects. Requires a synchronous generator expression and unshadowed `next`.
Async clauses/body suspension and a syntactically empty leftmost iterable are excluded.

Implicit generator expressions used as the sole call argument also qualify.
An `await` in the leftmost iterable evaluates during construction and can be
supported; it does not by itself make the generator's body asynchronous.

### `generator_empty`

**Behavior example:** construct `(x for x in xs)`, then return an empty generator.
Probe required output. The original construction still evaluates its leftmost
iterable and acquires its iterator, but does not drain the body. Uses the synchronous-generator restrictions
of `generator_skip_first` without requiring a builtin `next` binding.

### `yield_from_to_yield`

`yield from values` → `yield values`. Probe delegation versus yielding the iterable
itself, including forwarded generator behavior. Targets `yield from` syntax only;
the operand is preserved.

## State, ordering and object contracts

### `none_truthiness`

`value is None` → `not value` in behavior; `value is not None` → a truthiness check.
Probe empty/zero values that differ from missing values. Requires a single identity
comparison with exactly one `None` operand; equality tests and comparison chains
are excluded. Wrappers retain the supported expression's evaluation behavior.

### `mutable_default_share`

**Behavior example:** `def collect(items=None):` with an initial
`if items is None: items = []` → a function whose default is the shared `[]`.
Probe state leaking across calls. Requires the supported initial None-initializer
pattern with an empty list or dict; decorated/generic functions and more complex
initialization are excluded. The initializer remains in the body.

The initial guard contains exactly one assignment with no `elif`/`else`. Only
omitted arguments share the default; explicitly supplied `None` still executes
the initializer and creates a fresh container.

### `nonlocal_remove`

`nonlocal total` → `pass`. Probe updates to an enclosing binding. Targets the
declaration itself; later reads can raise `UnboundLocalError` once assignment
makes the name local. Global declarations are not changed by this ID.

### `mapping_setdefault_to_get`

`mapping.setdefault(key, default)` → `mapping.get(key, default)`. Probe insertion
of a missing key. Requires supported explicit positional method arguments; the
receiver type is not inferred and argument evaluation remains.

### `bisect_boundary_swap`

`bisect.bisect_left(values, key)` → `bisect.bisect_right(values, key)`, and the
reverse. Probe positions among duplicates. Requires a recognized standard-library
import and supported call arguments; arbitrary same-named functions are excluded.

### `deepcopy_to_copy`

`copy.deepcopy(value)` → `copy.copy(value)`. Probe independence of nested mutable
state. Requires a recognized import and supported one-argument call; explicit
memo arguments and arbitrary methods named `deepcopy` are excluded.

### `deque_popleft_to_pop`

`queue.popleft()` → `queue.pop()`. Probe FIFO versus LIFO behavior. Requires a
no-argument method call; the name alone does not prove a deque receiver.

### `gather_return_exceptions`

`asyncio.gather(task)` → `asyncio.gather(task, return_exceptions=True)`.
Probe raised failures versus exception objects returned in results. Requires a
recognized `asyncio.gather` import and at least one explicit non-generator
positional argument. Calls with keywords, including an existing
`return_exceptions` option, or unpacking are excluded.

### `itertools_take_drop_swap`

`itertools.takewhile(predicate, values)` → `itertools.dropwhile(predicate, values)`,
and the reverse. Probe prefix versus remaining suffix. Requires a resolved import
and supported two-argument call; both remain lazy.

### `fstring_conversion_swap`

`f"{value!s}"` → `f"{value!r}"` is one conversion change. Probe displayed
representations in formatted-string fields. Replaces each explicit `!s`, `!r`
or `!a` with either of the other two conversions. Fields without a conversion
and literal text are excluded.

Nested format fields qualify when they have explicit supported conversions.
Implicit debug `repr` and template-string conversions are excluded.

### `comparison_not_implemented`

**Behavior example:** a supported rich-comparison method's `return NotImplemented`
→ `return False`. Probe reflected comparison and fallback behavior. Requires
an undecorated synchronous nongenerator rich-comparison method directly in a
class, returning the builtin by name. It is not a global replacement of every
`NotImplemented` expression.

The method names are `__eq__`, `__ne__`, `__lt__`, `__le__`, `__gt__` and `__ge__`.
Nested function returns and shadowed `NotImplemented` names are excluded.

### `exception_cause_remove`

`raise Error() from cause` → `raise Error() from None`. Probe explicit diagnostic
causes and traceback chaining. Only a named cause qualifies; `from None`, calls,
awaits and compound cause expressions are excluded. This suppresses chaining,
rather than merely deleting the `from` clause.

Implicit `__context__` remains, but its display is suppressed. Judge a survivor
against the API's diagnostic contract, including whether an explicit cause matters.

### `dataclass_factory_share`

**Behavior example:** a supported `field(default_factory=list)` creates one empty
list shared by all new instances instead of a fresh list each time. Probe field
isolation. Supports builtin `list`, `dict` and `set` factories in direct annotated
fields of a recognized top-level dataclass. The emitted wrapper keeps a factory callable; it does not
use a mutable dataclass `default`. Explicit bases, metaclasses, type parameters
and extra decorators are excluded. Class suites contain only annotated name
fields, `pass` and strings; methods, plain assignments, control flow and private/
duplicate field names are excluded. Imports and builtin factories must be trusted.
Definition-time allocation, sharing and changed factory metadata are intentional.

### `sort_key_swap`

`sorted(rows, key=lambda row: (row.a, row.b))` → a key ordered as `(row.b, row.a)`.
Probe priority among tuple-key components. Requires resolved builtin `sorted`
and a lambda with exactly two distinct supported components (names, attributes
or literal-index chains). Calls, slices and signed negative indices in components
are excluded, as are `list.sort` and arbitrary callable keys.

Each key component evaluates once, but their evaluation order changes.
Properties and custom subscripts can execute user code; the analyzer does not
prove these reads pure.

### `lambda_capture_late`

**Behavior example:** a supported loop callback `lambda item=item: item` → a
callback reading `item` from its enclosing scope when called. Probe per-iteration
capture versus late binding when the callback argument is omitted. Explicit
callback arguments remain honored. Requires the recognized default-capture pattern;
arbitrary closures and complex defaults are excluded.

Requires a one-parameter lambda inside a same-scope ordinary `for` loop whose
simple-name target matches the captured name. Class-body loops, comprehensions,
async/unpacking targets, nested scope crossings, the matched loop's `else`, and
positional-only/keyword-only/variadic parameters are excluded. An added getter
closure and sentinel object change defaults/closure metadata and traceback shape.

### `unicode_digit_predicate_swap`

Exchanges a no-argument `isdecimal()`, `isdigit()` or `isnumeric()` call
with either of the other predicates. Superscript two (`²`) is a digit but
has no decimal value. One-half (`½`) has a numeric value and fails both
`isdigit()` and `isdecimal()`. Method names are matched syntactically,
so custom receivers may expose different behavior.

### `unicode_digit_ascii`

Narrows each of those three predicates to ASCII. For example, fullwidth
two (`２`) passes `isdigit()` and fails the narrowed check. A generated
lambda evaluates the receiver once, calls the original predicate and calls
`isascii()` only when the predicate succeeds. This also applies to custom
receivers and can expose an additional method lookup, frame or reference lifetime.

### `unicode_normalization_form`

Changes a recognized normalization form in `unicodedata.normalize` or
`unicodedata.is_normalized` along one axis. For example, NFC can become
NFD (decomposition) or NFKC (compatibility). The analyzer uses the decoded
form of a string literal and accepts trusted module and direct import aliases.

### `unicode_normalization_remove`

Returns the supplied text from `unicodedata.normalize(form, text)`. The
mutation evaluates the original callee, form and text in order. The synthetic
call omits the normalization function's type and form validation, and can
change frame and temporary-reference lifetimes.

### `string_casefold_lower`

Exchanges `s.casefold()` and `s.lower()`. For example, case folding maps
German sharp s (`ß`) to `ss`, while lowercasing keeps `ß`. A custom receiver
with either method name also qualifies.

### `string_affix_strip`

Exchanges `removeprefix(prefix)` and `lstrip(prefix)`, or
`removesuffix(suffix)` and `rstrip(suffix)`. Removing the prefix `ab`
from `ababaX` yields `abaX`. Stripping the character set `ab` yields
`X`. A single positional argument is required. Custom methods can match.

### `encoding_errors_ignore`

Changes the error policy of an encoding or decoding call to `ignore`.
For example, `data.decode('utf-8')` silently discards invalid byte
sequences under the mutation. An explicit policy expression still evaluates
at its original position. The rule accepts simple method receivers, matched
by name, and trusted `codecs.encode` or `codecs.decode` aliases.

### `encoding_errors_replace`

Changes the encoding or decoding error policy to `replace`. Invalid UTF-8
then decodes to replacement characters. This operator and
`encoding_errors_ignore` handle omitted, positional and keyword policies.
Both skip a literal policy already equal to the destination.

### `encoding_errors_strict`

Changes an explicit encoding or decoding policy to `strict`. Invalid input
can now raise an exception. All three encoding operators preserve evaluation
of dynamic policies and reject unpacked arguments, duplicate slots and
uncertain imported codec identities.

### `base_order_shuffle`

`class Combined(Left, Right): ...` → `class Combined(Right, Left): ...`. Probe
method resolution and cooperative inheritance. Requires distinct simple-name
direct bases; computed, repeated or unsupported bases are excluded. Larger lists
use stable source-derived permutations, not a promise of exhaustive permutations.

Two bases exchange places; three or more produce one deterministic source-derived
shuffle, falling back to a swap if unchanged. Decorators, type parameters and class
keywords are excluded; imported names and nested classes can qualify. The analyzer
does not resolve runtime base identities, MRO, implicit metaclasses or subclass
hooks. Reordering dependent bases can raise `TypeError`; inspect the failure before
inferring an assertion gap.

### `override_method_delete`

**Behavior example:** remove an eligible overriding method so that lookup reaches
the parent implementation. Probe subclass specialization. Requires the supported
statically resolved single-inheritance relationship and ordinary method shape;
dynamic inheritance and unsupported method forms are excluded.

Both this operator and `super_call_move` require ordinary top-level classes in the
same module, with an earlier parent that has no explicit bases. Methods must be
undecorated synchronous instance methods with matching parameter names/kinds and
no defaults, annotations, variadics, nested definitions, suspension, special names
or private mangled names. Imported, nested, conditional and multiple inheritance,
class escapes, method aliases/writes, lookup hooks, dynamic namespaces and nonliteral
class attributes are excluded. External monkey patching remains outside the model.

### `super_call_move`

**Behavior example:** move a standalone `super().save()` delegation within an
eligible overriding `save` method. Probe whether parent work occurs before or
after local work. Requires the supported single-inheritance, same-method call
shape described under `override_method_delete`; arbitrary nested or value-consuming
`super` calls are excluded. Requires builtin zero-argument `super`, exactly one
delegation and at least two straight-line expression statements. A first delegation
moves to last; another position moves to first. Control flow, walrus expressions
and starred arguments are excluded. The initial docstring and comment slots stay fixed.

## Cleanup, pattern dispatch and call contracts

### `finally_unwrap`

**Behavior example:** `try: work()` / `finally: cleanup()` → run `work()` then
`cleanup()` on normal completion. Probe cleanup during early return, escaping
exceptions or loop exits. A normal-path cleanup assertion can pass both programs.
Supports protected suites with a final body; it retains lexical suites using
header rewrites rather than blindly dedenting their contents.

### `match_case_delete`

**Behavior example:** a matching `case 1:` no longer dispatches to its body.
Probe whether that case's behavior is distinguished from later cases. The
rewrite retains unreachable syntax needed for compiler bindings. Invalid pattern
bindings and unsupported nesting depth are excluded; the case is not simply
deleted from source.

### `match_guard_constant`

`case pattern if allowed(value):` → a guard forced true or false after pattern
binding. Probe guard behavior. The guard's expression is retained syntactically
but short-circuited at runtime. Existing Boolean literals and wrappers exceeding
supported nesting are excluded; forcing true cannot make a failing pattern match.

### `match_or_alternative_delete`

`case 1 | 2:` → `case 1:` is one candidate. Probe each alternative. Requires a
supported OR pattern with valid compatible bindings; edits must preserve valid
dispatch and binding structure. It does not remove alternatives from ordinary
bitwise expressions or type unions.

### `statement_order_swap`

`record(); publish()` on adjacent statements → `publish(); record()` in behavior.
Probe ordering of effects. Supports adjacent calls, assignments or assertions
when at least one can have an effect. Direct local-name dependencies, directive
comments and unsupported statement shapes exclude a pair; this is not a general
proof that the statements commute.

### `stdlib_escape_remove`

**Behavior example:** `re.escape(text)` → return the original text after evaluating
the call's supplied arguments. Probe escaping contracts. Supports resolved
`re.escape`, `html.escape`, `urllib.parse.quote` and `quote_plus`; URL quoting
requires supported text input. Each argument evaluates once in original order;
unresolved imports and unsupported call shapes are excluded.

### `call_argument_swap`

**Behavior example:** `send(source(), destination())` evaluates `source()` first
and `destination()` second, then passes their values in exchanged argument roles.
Probe role-sensitive arguments. Swaps adjacent explicit arguments. Positional
swaps can use dynamic callees; keyword or mixed swaps require proven parameter
roles. `*args` and `**kwargs` suppress swaps. The emitted wrapper is not the naive
source edit `send(destination(), source())`.

### `call_result_default`

**Behavior example:** call a function known to return `int`, then replace the
consumed result with `0`. Probe use of a result separately
from call effects. Supports known return types and directly awaited coroutines;
the original call still runs once. Other supported defaults are `False`, `0.0`,
`0j`, empty strings/bytes and empty builtin list/tuple/dict/set containers.
Unknown result types and unused standalone
results are excluded. Replacing a result can shorten the object's lifetime.

## Exceptions

### `exception_type_pair`

`except ValueError:` → `except TypeError:`; similarly `raise ValueError(...)`
can become `raise TypeError(...)`. Probe handler boundaries and raised contracts.
Safe pairs are ValueError/TypeError, KeyError/IndexError, AttributeError/KeyError,
FileNotFoundError/PermissionError, ConnectionError/TimeoutError,
ImportError/ModuleNotFoundError and ZeroDivisionError/OverflowError. Supports
resolved simple names in `except`, `except*` and supported primary raise
expressions; arguments and explicit causes remain. Shadowed names, qualified or
dynamic types, all tuple handler members and termination exceptions are excluded.

### `exception_hierarchy`

`except MissingError:` → `except ConflictError:` when both are visible siblings
under a project-defined `AppError`. Probe project exception contracts. Reads class
definitions and explicit imports without importing modules. Supports visible direct
parents, children and siblings in `except`, `except*` and `raise`; raise replacements
must inherit `Exception`'s constructor without custom `__init__`/`__new__`.

Dynamic/ambiguous definitions, re-exports, multiple inheritance, termination
exceptions and exception groups are excluded. Eager import cycles and import-depth
bounds of 256 or more also exclude definitions. No imports are inserted. All
allowed project Python files become fingerprint inputs; dependency changes can
invalidate plans and resumed results. Static indexing cannot rule out every
dynamic rebinding. See [hierarchy boundaries and diagnostics](../../development/guides/contributing.md#user-defined-exception-hierarchies).

### `exception_bare_to_exception`

`except:` → `except Exception:`. Probe handling of exceptions outside `Exception`,
such as termination signals. Applies to ordinary handlers with a valid builtin
binding, not `except*`.

### `exception_exception_to_bare`

`except Exception:` → `except:`. Probe whether broadening a handler hides
termination. Requires the final ordinary handler without an `as` binding, not
`except*`; broadened handling can change whether execution stops.

### `exception_base_boundary`

`except Exception:` → `except BaseException:`, and the reverse. Probe the
termination boundary. Requires resolved supported names in ordinary or `except*`
handlers. Supported exception-group handler tuples also qualify.

### `exception_tuple_add_pair`

`except (ValueError,):` → a tuple also including its supported paired type.
Probe broadening handler coverage. Requires a tuple of recognized builtin
exception names and passes resolution/duplicate-member guards. Ordinary handlers
require parentheses; `except*` also supports unparenthesized exception lists.
Project exception tuples and arbitrary expressions are excluded.

### `exception_tuple_remove_member`

`except (ValueError, TypeError):` → a handler retaining only one member.
Probe whether each accepted type matters. Requires an ordinary handler's
parenthesized tuple of recognized builtin names with multiple members. Does not
create an empty handler or leave a lone termination exception. For `except*`,
parenthesized and unparenthesized supported tuples qualify, including singleton
removal to an empty tuple that catches no group members. Exception-group classes
are excluded as `except*` match types.

## Rounding and precision

Rounding can affect numeric value, result type, Decimal exponent, context flags
and traps. For scoped contexts, check precision-sensitive results inside the scope
and restoration of the outer context afterward. Static recognition of a rounding
call does not guarantee that its runtime argument is finite or even numeric.

### `rounding_function_swap`

`round(value)` → `math.floor(value)` or `math.ceil(value)` in behavior, with
corresponding exchanges among these functions. Probe tie and directional rounding.
Requires resolved one-argument calls. Different numeric methods can run and
non-finite inputs can raise; explicit `ndigits` belongs to a separate mutation.

### `round_digits_remove`

`round(value, digits)` → `round(value, None)` in behavior. Probe requested
precision and result type. The supplied `digits` expression still evaluates
before its value is replaced; unsupported call forms are excluded.

### `decimal_rounding_mode_swap`

`value.quantize(quantum, rounding=ROUND_DOWN)` → another recognized rounding mode
in a supported Decimal context. Probe directional and tie policies. Supports known
mode values in recognized quantization, context construction and context
assignment; arbitrary dynamically computed modes are excluded.

### `decimal_quantum_shift`

`Decimal("1.00")` used as a known quantization quantum → a quantum with exponent
one greater or smaller. Probe scale requirements. Requires a recognized finite
Decimal quantum in the supported operation; it does not mutate every Decimal
literal. Inspect the preview for the exact quantum spelling.

### `decimal_context_precision`

`Context(prec=6)` → precision `1` in a supported context form. Probe precision
loss. An explicit recognized precision of at least three becomes one; one or two
increases by two. Unknown precision expressions and unsupported context writes
are excluded.

### `intermediate_rounding_remove`

`round(round(value, 2) + fee, 2)` → final rounding without the inner rounding, in
behavior. Probe intermediate versus final precision. Requires recognized nested
rounding or Decimal quantization through supported arithmetic. Inner numeric
values and options still evaluate in source order; arbitrary standalone rounding
is not removed by this operator.

## Regular expressions

These operators require a resolved standard-library `re` binding or trusted
compiled receiver. Recognized patterns include literals and uniquely initialized
module/local literal bindings. Shared literal uses are changed at the selected
use without changing other consumers. Unsupported ambiguous future set syntax
and groups nested more than 255 levels are excluded.

### `regex_match_boundary`

`re.fullmatch(pattern, text)` → a supported prefix/substring matching call;
`r"^name$"` → a supported start/end assertion change. Probe match span, input
edges and newline behavior. Compiled receiver edits preserve `pos`/`endpos`.
Dynamic patterns can qualify for method changes when argument contracts agree;
pattern edits require recognized pattern text. With runtime flags, a literal must
parse in ordinary or verbose mode for a method candidate.

### `regex_character_set`

**Behavior example:** Unicode `r"\d+"` → an ASCII-decimal character class at one
selected atom. Probe the accepted alphabet. Covers decimal, word, whitespace,
complement, boundary and supported case-insensitive classes. Local/scoped modes
matter; byte patterns have no Unicode-mode mutation. A narrowed `\D` exclusion
accepts more characters, including Unicode decimal digits. A `\b` boundary
depends on word status on both sides, including nonword input edges.

For an ASCII-only field, an original `\d+` accepting fullwidth `９` can already
violate the specification before mutation testing. For a Unicode decimal field,
a test accepting `９` distinguishes it from `[0-9]+`. The mutant does not decide
which repertoire your specification requires.

### `regex_ascii_mode`

**Behavior example:** a text pattern compiled in Unicode mode → the same pattern
with root ASCII mode. Probe all mode-dependent atoms together. Toggles the root
mode while preserving scoped overrides and other flags; the compiled pattern's
exposed flags change. If both inline `(?a)` and `re.A` declare the mode, one
candidate changes both. Bytes and unrecognized dynamic pattern/mode forms do not
receive a Unicode-mode toggle.

## Type annotations

Run a type checker as the configured command when evaluating these mutations.
A runtime test suite might not observe annotation changes. The analyzer recognizes
supported annotation syntax and typing providers; it does not add missing imports
or rewrite arbitrary expressions that merely resemble types. See the
[annotation-provider details](../../development/guides/contributing.md#abstract-set-annotation-providers)
for name-resolution boundaries.

### `type_nullable_remove`

`value: int | None` → `value: int`, or the supported `Optional[int]` equivalent.
Probe whether callers or implementations need the nullable part. Requires a
recognized nullable annotation; runtime bitwise expressions are excluded.

### `type_nullable_add`

`value: int` → `value: int | None` in supported annotation syntax. Probe whether
code handles a possibly missing value. Already nullable or unsupported annotation
forms are excluded; this does not insert a runtime `None` value.

### `type_list_sequence`

`list[int]` → a supported `Sequence[int]` annotation, and the supported reverse
direction. Probe dependence on mutability or concrete list APIs. Requires available,
recognized provider spellings; no import is inserted for a missing replacement.

### `type_set_abstract_set`

`set[int]` → a supported abstract-set annotation, and the supported reverse
direction. Probe concrete mutable-set requirements. Provider resolution and
replacement availability restrict the candidate; runtime set construction is separate.

### `type_dict_mapping`

`dict[str, int]` → a supported `Mapping[str, int]` annotation, and the supported
reverse direction. Probe concrete dictionary operations versus the mapping
interface. Requires recognized existing provider spellings; no imports are added.

### `type_iterable_iterator`

`Iterable[int]` → `Iterator[int]`, and the reverse when supported providers are
available. Probe iteration versus iterator protocol requirements. Annotation-only;
does not call `iter` or change runtime object construction.

### `type_sequence_iterable`

`Sequence[int]` → `Iterable[int]` when supported providers are available.
This operator is one-way. Probe indexed/reiterable sequence requirements versus general iteration.
Only resolved annotation forms qualify; the type checker's nonzero exit kills
the mutant just as any configured command failure would.

## Literal and dispatch extensions

These operators are enabled by default. Use `--exclude-operators ID` to disable
an individual operator; existing explicit operator selections remain explicit.

### `float_literal_neighbor`

`threshold = 1.0` → the adjacent smaller or larger finite binary floating-point
value. Probe exact thresholds and rounding boundaries. Supports signed finite
float literals, including exponent notation and subnormal values. Replacements
remain floats. Infinite literals and imaginary literals are excluded.

### `bytes_literal_empty`

`payload = b"data"` → `payload = b""`. Probe required binary content. Empty the
whole nonempty bytes expression, including implicit concatenation. The replacement
retains bytes type and keeps comments between concatenated tokens. Annotation,
pattern and assignment-target exclusions apply.

### `fstring_format_spec_remove`

`f"{value:08.2f}"` → `f"{value}"`. Probe required formatting independently of
value evaluation. Removes a format specification, including nested format fields;
the removed fields' effects are intentionally skipped. When the specification
contains nested fields, the replacement renders an empty specification and
retains their name references in an unreachable expression. Specifications
containing a named assignment expression or `yield` are excluded. Debug fields
retain their debug text.

### `fstring_format_value_neighbor`

`f"{value:08.2f}"` → `f"{value:08.3f}"` is one example. Probe literal width and
precision boundaries in recognized standard format specifications. Each candidate
changes one ASCII decimal component by one. A literal component can qualify
when another component is supplied by one nested field, as in
`f"{value:0{width}.2f}"`. Format text whose escapes change its decoded spelling
and unrecognized format syntax are excluded.

### `conditional_branch_swap`

`first if ready else second` → `second if ready else first`. Probe whether the
condition selects the intended result. The condition evaluates once; only the
selected branch evaluates. Identical branch source text is excluded.

### `await_remove`

`await work()` → `work()`. Probe whether completion and suspension are required.
The operand still evaluates once, but an awaitable is no longer awaited at this
site. This can expose an unawaited coroutine, missing effects or an incorrect
return type. The enclosing async function remains async.

### `dict_unpack_order_swap`

**Behavior example:** `{**base, **overrides}` gives `base` precedence on duplicate
keys. Only adjacent unpack operands qualify. Both operands evaluate and expand
into isolated plain dictionaries in their original order. A failure in the first
operand's mapping protocol prevents the second operand from evaluating.
The snapshots then combine in reverse order.

This also changes first-occurrence key insertion order. Custom key equality
calls and collision timing can differ during recombination. In particular, a
collision with an earlier display entry can occur after a later operand has
already run, although the original display would have stopped before that
operand. Read the candidate before interpreting a failure involving custom keys.

### `dict_unpack_delete`

`{**base, **overrides}` → a dictionary without one unpacked operand's entries.
Probe required inherited entries. Skips that operand's evaluation deliberately,
while retaining unreachable syntax needed for lexical bindings and generator
classification. Other entries retain their order.

### `del_remove`

**Behavior example:** `del cache[key]` leaves the item in place. Probe required
deletion of names, attributes or items. Supports multiple targets and inline
suites. The replacement retains compiler binding facts in an unreachable
assertion message; target expressions and deletion hooks do not execute.

### `exception_handler_swap`

**Behavior example:** adjacent typed handlers exchange order, changing which
overlapping handler runs first. Supports ordinary and exception-group handlers.
A bare handler stays last. A clause whose scope directives cannot safely move
past another clause is excluded.

### `exception_cause_omit`

`raise Error() from None` → `raise Error()`. Probe required suppression or explicit
chaining of exceptions. Nontrivial cause expressions become unreachable syntax
after the raise so compiler bindings remain; their evaluation is skipped.
The primary exception expression remains unchanged.

### `match_case_swap`

**Behavior example:** adjacent cases exchange order. Probe priority among
overlapping patterns or guards. Captures and suites move with their cases.
An unguarded irrefutable case cannot move ahead of another case. Scope-directive
ordering guards also apply.

### `match_pattern_wildcard`

`case [1, name]:` → `case [_, name]:`. Probe structural constraints within
sequence, mapping and class patterns. Replaces a binding-free subpattern with a
wildcard. Existing captures, including an enclosing `as` capture, remain. A dead
guard expression retains free-variable reads from the removed constraint.
Already-irrefutable and starred subpatterns are excluded.

### `match_class_position_swap`

`case Point(x, y):` → `case Point(y, x):`. Probe positional contracts governed by
`__match_args__`. Exchanges two positional subpatterns, retaining captures and
keyword patterns. Identical subpattern source text is excluded. Redundant grouping
moves with its subpattern to preserve the Python delimiter-depth limit.

## Assignment, result and boundary behavior

### `augmented_to_binary`

`items += extra` → `items = items + extra`. Probe the difference between in-place
and ordinary binary dispatch, including aliases to a mutable value. Attribute and
subscript targets qualify: the receiver, key, getter and right operand are each
evaluated once in their original order before the native setter. Complex targets
use an expression wrapper. An unsupported or excessively nested target is excluded.

### `assignment_target_swap`

`left, right = pair` → `right, left = pair`. Probe unpacking destinations. Swaps
adjacent targets in tuple/list assignments, including nested, chained and starred
targets. The right-hand side stays in place; target-side effects move with their
targets. This operator does not exchange values after assignment.

### `return_value_none`

**Behavior example:** `return compute()` → evaluate `compute()`, then return
`None`. Probe dependence on a particular return value without removing the return
expression's effects. Supports synchronous and asynchronous functions and retains
binding/suspension syntax. Bare returns and explicit `return None` are excluded.

### `identity_equality_swap`

`left is right` → `left == right`, and the reverse; likewise `is not` and `!=`.
Probe identity versus value equality. Each comparison operator in a chain is an
independent candidate. Operands and short-circuit evaluation stay in place;
equality can deliberately invoke user-defined comparison methods.

### `else_suite_remove`

**Behavior example:** a `for`, `while` or `try` statement's `else` suite becomes
unreachable. Probe behavior on normal completion. The original suite remains in
unreachable syntax, preserving local variables, closure reads and generator
classification. `finally` and surrounding control flow remain active.

### `zip_strict_toggle`

`zip(left, right)` → `zip(left, right, strict=True)`. An explicit nonliteral
`strict=value` is negated after evaluating its value once. Boolean literals stay
with `boolean_literal`. Probe mismatched iterable lengths
and deferred iteration errors. Requires a resolved builtin `zip`; an ambiguous
`**kwargs` source for `strict` is excluded.
Dynamic-option candidates select the `strict` value's position.

### `range_bound_neighbor`

**Behavior example:** `range(stop)` → a range whose stop is one larger or smaller.
Explicit start and step arguments also receive neighboring candidates. The
original call evaluates and converts its arguments once; a wrapper builds the
mutant from the resulting range's integer fields. Steps never become zero.
Requires a resolved builtin `range` and supported explicit positional arguments.

### `compare_direction`

`left < right` → `left > right`; likewise `<=` and `>=`. Probe reversed ordering
without changing equality inclusion. Every ordering operator in a chain can be
changed independently while its operands retain their evaluation order.

## Regex structure and format options

### `regex_quantifier_neighbor`

`r'a{2,4}'` → a pattern with a neighboring minimum or maximum repeat count.
Shorthand quantifiers also qualify. Bounds remain ordered and nonnegative;
lazy/possessive suffixes remain. Lookbehind width and capture-reference constraints
exclude edits that would invalidate a supported pattern. Uses the same recognized
regex calls, literal decoding and flag analysis as the existing regex operators.

### `regex_greediness_toggle`

`r'a+'` → `r'a+?'`, and the reverse. Probe which match is preferred when multiple
lengths qualify. Exact-count and possessive quantifiers are excluded. Escapes,
character classes and literal question marks are not quantifier suffixes.

### `regex_alternative_delete`

`r'(?:one|two)'` → `r'(?:two)'` or `r'(?:one)'`. Removes one branch and its
adjacent separator at a time, including nested and empty alternatives. Edits
that remove referenced captures, rebind numeric references or invalidate a
lookbehind are excluded. Captures are not renumbered in source to compensate.

### `fstring_format_option_swap`

`f'{number:>10}'` → `f'{number:<10}'` is one candidate. Probe alignment, sign,
grouping and presentation type in Python's format mini-language. Dynamic width
and precision fields keep their evaluation order. Known builtin value types and
option combinations filter invalid alternatives; unknown values and custom
`__format__` methods can still reject an otherwise valid standard specification.

## Context exits and individual yields

### `context_exit_omit`

**Behavior example:** enter a selected `with` or `async with` manager and run its
body, but omit that manager's exit callback. Probe cleanup and exit-time exception
handling. A local proxy binds the original special methods through the class
hierarchy and retains entry failure, target assignment and other managers'
unwinding. Requires unambiguous builtin helpers. The analyzer does not import or
execute the project during discovery. Stack or object introspection can observe
the generated wrapper.

### `context_exit_result_negate`

**Behavior example:** invert the suppression decision returned by a class's
`__exit__` or asynchronous `__aexit__`. The returned value is wrapped so that its
truthiness is inverted when an exceptional `with` exit requests it, after the
method's cleanup has run. Normal exits do not test the decision. Bare returns
become `return True`; a fallthrough path gains `return True`. The return expression
still runs once. Generator methods, nested helper functions and unrelated
same-named module functions are excluded. Direct callers can observe the wrapper.

### `yield_statement_remove`

**Behavior example:** skip a standalone `yield value` suspension while still
evaluating `value`. Probe individual yielded elements. An unreachable yield keeps
the function a generator, including when this is its last yield. Async generators
qualify. Existing cleanup remains active, while `send` and `throw` reach the next
remaining suspension. `yield from` and yields whose result is used are excluded.
