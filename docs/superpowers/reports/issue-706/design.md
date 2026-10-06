# Container element deletion (#706)

Opt-in container_element_delete removes one element from a nonempty load-context list,
explicitly parenthesized tuple or dict (key/value together). Reconstruct the entire
literal from AST element slices, wrapping each retained expression. Normalize separators
and inter-element comments; preserve internal expression text. Empty forms are [], (), {};
nonempty tuples always receive a trailing comma. Skip starred/unpacked containers and
any literal containing binding/suspension expressions. Nested literals are independent
candidates when eligible. Ordinary calls remain eligible and deleted effects disappear.

## Design self-review

1. Delimiters: never split source on commas; AST slices protect strings, lambdas and
   nested containers. Tuple singleton remains a tuple and dict key/value stay paired.
2. Eligibility: load-only list/tuple plus dict; skip unparenthesized tuples, sets,
   comprehensions, unpacking and binding/suspension, including nested occurrences.
3. Runtime roles: shared annotation ranges plus PEP613 alias index, pattern guard and
   a separate assignment-target context exclude nonruntime/target expressions; ordinary
   subscription expressions remain eligible.
4. Bounds: stream replacements; skip adjacent equal element fragments and emit at most
   max+1 distinct edits at each fixed span/operator. Shared identity retention deduplicates.
5. Correspondence: Lean proves one element removed and remaining order. CPython checks
   container kinds, exact contents, source syntax and deleted-call effects separately.
