# Builtin conversion call removal (#707)

Opt-in conversion_call_remove replaces a direct, statically resolved builtin conversion
call with the parenthesized source of its sole positional argument. Allowed names:
int, float, complex, bool, str, bytes, list, tuple, dict, set, frozenset. No keywords,
star expansion, generator expressions or binding/suspension expressions in arguments.
Reuse name-resolution facts; add complex to tracked builtin names. Shadowing, known
global/nonlocal rebinding and dynamic namespace uncertainty are conservatively excluded.
Do not claim protection against arbitrary external builtin monkey patching.

## Design self-review

1. Source/value: whole-call span to (argument) preserves precedence and exactly one
   evaluation, including lambda/conditional/tuple expressions and compact return syntax.
2. Conversion: __int__/validation/copy/iterator consumption disappear; equal observed
   types do not imply equivalence and do not cause candidate exclusion.
3. Scope: only direct builtin names, existing resolution proof at name occurrence;
   no attributes, alias functions or unknown binding guesses.
4. Roles/safety: runtime Value outside patterns, annotations and explicit aliases;
   recursively reject generator/binding/suspension expressions in the retained argument.
5. Integration/proof: legacy list/tuple name swapping remains an independent candidate;
   Lean models evaluate-once state threading and skipped conversion, not name resolution.
