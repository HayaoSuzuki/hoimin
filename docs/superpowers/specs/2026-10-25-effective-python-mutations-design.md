# Effective Python behavior mutations

The user approved implementing the five first-wave proposals after the sample
review. Keep the existing Rust/Ruff AST analyzer and opt-out policy; do not add
Python instrumentation, dependencies or broad type/alias inference.

Operators (all default-enabled and individually excludable):
- none_truthiness: a single `is None` comparison becomes `not (value)`; `is not
  None` becomes `not not (value)`. Support None on either side. Exclude comparison
  chains, None/None and annotation/pattern contexts. Preserve boolean results and
  exactly one evaluation; introduce no builtin names.
- mutable_default_share: a None parameter default becomes [] or {} only when the
  first body statement after an optional docstring is `if parameter is None:`
  with one simple assignment `parameter = []` or `{}`, no elif/else and no other
  targets/statements. Retain the guard/body so explicitly supplied None still
  creates a fresh object. Support ordinary/async functions and methods, keyword-
  only/positional-only parameters. Exclude decorated definitions and type params
  initially. Never rewrite dataclass default_factory to an invalid mutable field.
- nonlocal_remove: replace a nonlocal declaration with pass, retaining inline
  suite syntax/comments. One candidate per declaration, including multiple names;
  surrounding generator kind/source remains unchanged. No global mutation.
- yield_from_to_yield: remove only the from keyword token in a YieldFrom expression,
  retaining all other source and original operand evaluation once.
  No replacement in annotation/pattern contexts. Delegation and its result/send/
  throw behavior deliberately change; ordinary iteration value assertions suffice.
- mapping_setdefault_to_get: rename a setdefault attribute call to get for one or
  two ordinary positional arguments and no keywords/starred args, on the existing
  simple-receiver grammar. This is a syntactic method operator (like current
  mapping/method operators), not a proof of builtin dict type. Custom same-name
  methods can be affected. Keep receiver/argument source and evaluation unchanged.

With the expansion below, defaults become69; catalog88; public opt-out regression set26. Existing explicit
and saved selections remain explicit. None/nonlocal/yield delegation rank as
high-value control; shared default and mapping insertion as behavioral. Keep the
existing ranking rule version and schema policy, as previous operator additions do.

Use a focused rust/effective_python.rs module for builders, with small collector
hooks. Shared entry inspection stays in function_entry.rs. Selection gates precede
source copies; cancellation/admission failure stops work. Preserve ordered candidate
prefixes, budgets, UTF8 source ranges, docstrings and LF/CRLF/CR spelling.

Alternatives considered: general flow/type inference would cover more defaults and
methods but substantially increase scope; external runtime wrappers would perturb
scope and evaluation. Choose local AST patterns and document exclusions instead.

Validate using new RED->GREEN analyzer tests and independent CPython3.14.9/3.15.1
observations, including cross-call default sharing, explicit None, empty values,
outer closure updates, generated value shape and dictionary contents. Check capped
prefixes, line/symbol selection, saved plans and opt-out behavior. Each stage gets
at least five recorded self-reviews; one fresh whole-branch review precedes the
expensive final workspace suite. Limit cargo build jobs and test threads to2;
clean owned build/site outputs after verification. Create a PR against main.


## Research references

These independently designed mutations draw on the author's published examples,
not a claim to have read the full book prose. Source revision:
`fa87298a0ba042fc07fabadb599e5c3f6becf2d7`.

- [Item36: missing/dynamic defaults](https://github.com/bslatkin/effectivepython/blob/fa87298a0ba042fc07fabadb599e5c3f6becf2d7/example_code/item_036.py).
- [Item51: separate instance defaults](https://github.com/bslatkin/effectivepython/blob/fa87298a0ba042fc07fabadb599e5c3f6becf2d7/example_code/item_051.py).
- [Item33: nonlocal updates](https://github.com/bslatkin/effectivepython/blob/fa87298a0ba042fc07fabadb599e5c3f6becf2d7/example_code/item_033.py).
- [Item45: yield delegation](https://github.com/bslatkin/effectivepython/blob/fa87298a0ba042fc07fabadb599e5c3f6becf2d7/example_code/item_045.py).
- [Item27: dictionary initialization](https://github.com/bslatkin/effectivepython/blob/fa87298a0ba042fc07fabadb599e5c3f6becf2d7/example_code/item_027.py).

## Additional boundary, copy and queue mutations

The requested expansion adds `bisect_boundary_swap`, `deepcopy_to_copy`, and
`deque_popleft_to_pop`, all enabled by default and individually excludable.
There are now 69 default operators, 88 catalog entries and 26 opt-out fixture IDs.

`bisect_boundary_swap` swaps only `bisect_left`/`bisect_right`, not `bisect` or
insort variants. `deepcopy_to_copy` requires exactly one positional argument,
no keywords and no starred arguments: copy does not accept deepcopy's memo.
Both require the existing conservative CallableImports proof for top-level
stdlib imports. Module attributes change only the member token; imported
members use the existing guarded __import__ convention. Rebindings, escapes,
shadowed imports and dynamic namespaces are excluded by the shared resolver.

`deque_popleft_to_pop` changes zero-argument popleft calls on simple names or
attribute receivers. This is a syntactic method mutation, including custom
methods; it does not infer deque types. The reverse direction is excluded
because pop is common on lists, sets and dictionaries.

All three preserve receiver/argument evaluation count and order, comments,
selection on changed tokens, source ranges and candidate budgets. Witnesses
exercise duplicate boundaries, nested mutable state and a queue with two items
on CPython 3.14 and 3.15. No third-party dependency or schema change.
