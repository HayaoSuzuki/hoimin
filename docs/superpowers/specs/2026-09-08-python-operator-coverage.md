# Python operator coverage

## Scope

Extend runtime mutation discovery to the operator syntax and operator functions
documented by [Python 3.13](https://docs.python.org/ja/3.13/library/operator.html).
Use the existing token and AST candidate producers. Preserve bounded discovery,
selection, profile filtering, annotation exclusion, and deterministic ordering.

The helper APIs `attrgetter`, `itemgetter`, `methodcaller`, and `length_hint` are
not operator syntax/function counterparts and are outside this change. Python
3.14-only APIs are outside the compatibility target.

## Syntax additions

| Selector | Original | Replacement |
| --- | --- | --- |
| `binary_power` | `**` | `*` |
| `binary_matmul` | `@` | `*` |
| `augmented_power` | `**=` | `*=` |
| `augmented_matmul` | `@=` | `*=` |
| `bitwise_xor` | `^` | `&` |
| `bitwise_invert` | `~` | `+` |
| `augmented_bitwise_and_or` | `&=`, `|=` | `|=`, `&=` |
| `augmented_bitwise_xor` | `^=` | `&=` |
| `augmented_bitwise_shift` | `<<=`, `>>=` | `>>=`, `<<=` |

Existing mutation pairs remain unchanged. All additions belong to the default
runtime set. Bitwise additions also belong to `bitwise_ops`. New syntax operators
are arithmetic ranking candidates. Operator tokens must come from their AST
operator position, never decorators, unpacking, strings, comments, or annotations.

## Function forms

Add the default runtime selector `operator_function`. Resolve `import operator`,
`import operator as op`, and `from operator import add as plus` conservatively.
Only unconditional module imports with a unique, unmodified binding are trusted.
If any binding, parameter, deletion, wildcard import, dynamic namespace operation,
or explicit module attribute write makes identity uncertain, skip the affected
binding (or all bindings where the affected name cannot be determined). Relative
imports, other modules, conditional imports, and local imports are not trusted.
This deliberately sacrifices candidates rather than mutating unrelated methods.
The analysis is intra-module, not a proof against external monkey-patching or
custom import loaders. It assumes `import operator` loads the standard module;
it does not resolve the project's import search path. Explicit `__dict__`/`vars`
namespace access and writes to `__import__` must not bypass the conservative
mutation guards.

Apply mutations to loaded callable references, including higher-order uses, not
import declarations, stores, annotations, match patterns, or arbitrary attributes.
Match guards remain ordinary expressions. Pattern grammar cannot accept lambda
or generated import expressions; exclude patterns instead of emitting invalid
Python. Direct calls
retain their original argument text, order, count, and evaluation frequency.
For a module alias, replace only the member name for function-to-function pairs.
For a from-import alias, a qualified `__import__('operator').replacement` lookup
is permitted only when `__import__` has no binding or dynamic uncertainty in the
module. Do not inject imports or change all uses by mutating an import statement.

| Functions | Mutation |
| --- | --- |
| `eq`, `ne`; `lt`, `le`; `gt`, `ge` | exchange each pair |
| `add`, `sub`; `mul`, `truediv`; `floordiv`, `mod` | exchange each pair |
| `pow`, `matmul` | `mul` |
| `and_`, `or_`; `lshift`, `rshift` | exchange each pair |
| `xor` | `and_` |
| `neg`, `pos` | exchange |
| `abs` | `neg` |
| `index`, `inv`, `invert` | `pos` |
| `not_`, `truth`; `is_`, `is_not` | exchange each pair |
| `iadd`, `isub`; `imul`, `itruediv`; `ifloordiv`, `imod` | exchange each pair |
| `ipow`, `imatmul` | `imul` |
| `iand`, `ior`; `ilshift`, `irshift` | exchange each pair |
| `ixor` | `iand` |
| `concat`, `iconcat` | exchange (allocation versus in-place dispatch) |
| `countOf`, `indexOf` | exchange |
| `contains` | positional-only two-argument lambda returning `item not in container` |
| `getitem` | `contains` (lookup versus membership) |
| `setitem`, `delitem` | positional-only lambda returning `None` without the write/delete |
| `call` | callable accepting positional-only target and arbitrary arguments, returning `None` |

Recognize documented dunder aliases and preserve dunder spelling when the
destination has that documented alias; otherwise use the canonical destination.
Lambdas preserve argument evaluation but intentionally remove the operation. No
claims of semantic equivalence are made; protocol differences are the mutation.
Identity/introspection of a mutated callable is not preserved.

## Verification and delivery constraints

- Work only in `.worktrees/python-operator-coverage`; preserve dirty main files.
- Commit implementation, tests, and design/verification documentation together.
- Do not launch Windows Actions or add automatically triggered Windows jobs.
- Avoid repeated GitHub API polling; use a 180-second check interval after push.
- Use test-driven development, meaningful external Python behavior assertions,
  and independent code review before PR completion. Do not mutate test modules.
- Keep generated mutation candidates inside the existing bounded AST producer.
- Do not add a dependency or change Python's supported version floor.
- PR prose describes changes and verification only. Do not merge the PR.
