# Insert early iterator termination

Add default-enabled, individually removable `iterator_stop_insert` and
`async_iterator_stop_insert` operators. The user requested StopIteration insertion
limited to `__next__`, and its async counterpart in `__anext__`, with generator
functions excluded. These are separate operators from loop_break_insert.

Insert `raise StopIteration()` at a synchronous class method `__next__` entry,
or `raise StopAsyncIteration()` at an async class method `__anext__` entry,
after an optional docstring. This checks whether callers assert that the iterator
produces any expected values. It is not a mutation that drops only a later item.
Retain the original body after the inserted statement. Skip generator/async
generator methods, decorated methods, mismatched async forms, non-method functions,
methods that declare the inserted name global anywhere in their own scope,
and methods that already unconditionally raise the same exception at entry.
Decorated methods are conservatively excluded because wrappers can change the
iterator protocol. Preserve comments, inline suites, indentation and line endings.

Approaches: arbitrary insertion positions create many redundant mutants and need
reachability analysis; replacing the body loses the distinction from existing
body erasure. A single entry insertion after the docstring is a bounded, clearly
observable way to test early exhaustion. The insertion location is an implementation
choice stated to the user. Nested scopes must not make an outer method a generator.

Only emit when static name resolution proves the inserted exception name refers
to the builtin. Add entry resolution sites rather than inferring safety from
text. Track the two names for wildcard/dynamic/shadowing uncertainty without
expanding the existing exception-pair operators' exception catalog.

Preserve candidate prefixes, line/symbol selection, cancellation and payload
budgets. Explicit selections and saved plans are not expanded. Validate original
and mutated execution with CPython3.14/3.15; distinguish honest early exhaustion
from RuntimeError conversion. Use dedicated branch/worktree and five self-review
passes each at design, implementation, validation and delivery.
