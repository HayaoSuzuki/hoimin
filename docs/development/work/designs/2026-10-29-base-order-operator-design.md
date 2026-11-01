# Default base-order operator

The user requested product implementation after the six-example research found
useful assertion gaps, explicitly choosing opt-out behavior. Add
`base_order_shuffle` to the default runtime set, with individual exclusion via
`--exclude-operators base_order_shuffle` and explicit selection via `--operators`.

Emit one edit for an undecorated, non-generic class without class keywords whose
direct bases are at least two distinct simple names. Imported names and nested
classes are allowed; never import a target or inspect runtime MRO. Reject starred,
attribute, call and subscript bases, duplicate normalized names, metaclass/other
keywords, decorated and generic classes. Preserve all source trivia by replacing
only each name in its original slot, in a single first-to-last-name edit.

Two bases swap. Three or more use Fisher-Yates driven by a domain-separated
BLAKE3 XOF over the original base-list span. If unchanged, swap the first two.
There is no new RNG dependency, runtime entropy or seed configuration. Stable
source produces stable candidates across selectors and repeated plans; different
source spans can produce different permutations. Uniform sampling is not promised.

This syntactic mutation does not prove MRO validity, distinct runtime identities,
or correct use of super. Reordered dependent bases, implicit metaclasses or hooks
can fail at class creation. Normal test-process outcomes remain authoritative;
nonzero exits can include runtime errors, not only missing assertions.

Reuse existing scope/line selection, cancellation, candidate caps, logical payload
accounting, ranking and plan serialization. Gate replacement construction on
selection. Keep the operator out of existing selector families. Append its enum
identity to preserve existing declaration order. Catalog/default counts become
102/83. Add analyzer, CLI opt-out, source-compilation and saved-plan verification
tests, update public documentation and retain historical research receipts.
