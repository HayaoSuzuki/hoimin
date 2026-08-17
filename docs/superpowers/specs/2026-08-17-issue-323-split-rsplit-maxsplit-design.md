# Issue 323: `split`/`rsplit` explicit `maxsplit` design

## Scope

Stop emitting `collection_string_split_rsplit` candidates for calls that do
not supply `maxsplit`. Preserve candidates when `maxsplit` is supplied as the
second positional argument or as the named `maxsplit` keyword. The change is
limited to the Rust analyzer, its regression tests, and the operator contract
documentation; it adds no dependency and changes no other operator.

## Root cause

`collect_method_call` currently gates `split`/`rsplit` with
`has_supported_same_contract_arguments`. That shared predicate rejects starred
positional arguments and unnamed `**kwargs`, but it does not distinguish the
argument that makes left-to-right and right-to-left splitting observably
different. Consequently, `text.split()` and `text.split(",")` produce
equivalent `rsplit` mutants.

## Considered approaches

1. Add a split-specific argument predicate that composes the existing shared
   safety check with explicit `maxsplit` detection. This is recommended because
   it names the semantic requirement, is independently testable through the
   analyzer, and cannot alter `min`/`max` or `startswith`/`endswith`.
2. Add the positional/keyword expression directly to the match guard. This is
   slightly shorter but hides the operator contract inside dispatch logic and
   makes later argument-policy changes harder to review.
3. Replace the shared predicate with a complete per-method Python signature
   validator. That could reject additional invalid original calls, but it is a
   broader behavior change unrelated to the equivalent-mutant defect.

Use approach 1.

## Argument contract

Define a private `has_supported_split_rsplit_arguments` predicate. It first
requires `has_supported_same_contract_arguments`, preserving the current
rejection of `*args` and `**kwargs`. It then requires either at least two
positional arguments or a keyword whose name is exactly `maxsplit`.

The analyzer remains syntax-directed and does not evaluate argument values or
infer the receiver type. In particular, this issue does not add constant
evaluation for negative `maxsplit` values and does not broaden validation of
other call signatures.

## Data flow

`collect_method_call` continues to receive the parsed `ExprCall`, method name,
and attribute span. Only the `split`/`rsplit` match arm calls the new predicate.
When it returns true, the existing method-name span replacement and
`CollectionStringSplitRsplit` identity are unchanged. When it returns false,
no candidate is added.

## Testing

Add one focused regression that places omitted, separator-only, second
positional, and named-keyword forms in the same source. It must observe no
operator candidates for `split()`, `split(sep)`, `rsplit()`, or `rsplit(sep)`,
and both replacement directions for positional and keyword `maxsplit` forms.
Keep the broad collection fixture's reverse-direction positive case by giving
its `rsplit` call an explicit positional `maxsplit`; its existing reparse loop
continues to validate replacement syntax.

Run the focused regression, all Rust analyzer tests, formatting, Clippy, the
workspace Rust tests, and the Python `unittest` contract suite. Update the
README operator table and development guide so the public contract says that
the split-direction swap requires explicit `maxsplit`.

