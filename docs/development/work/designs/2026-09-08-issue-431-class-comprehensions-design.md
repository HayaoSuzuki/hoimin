# Issue #431: class comprehension lookup scopes

## Goal and contract

Discover operator_function mutations in class-body list/set/dict comprehensions and generator expressions when references resolve through the implicit comprehension scope. Keep the leftmost iterable in its enclosing lookup scope. Preserve module-wide rebinding, dynamic namespace, private-name and metaclass safeguards. No CLI/schema changes or new dependencies. Rust MSRV remains 1.88; controlled Python is 3.14.

## Design

Extend ClassLookupScan with explicit comprehension traversal. Visit the first iterable under the existing scope; visit targets, remaining iterables, filters and produced expressions under Function scope; restore the prior scope on return. Nested comprehensions naturally follow the same rule. Apply this to all four expression kinds. ImportScan remains the conservative identity authority; do not loosen its binding rules.

Alternatives: removing the class guard would admit unrelated metaclass bindings; treating every comprehension child as Function would incorrectly admit the leftmost iterable. Reusing the builtin flow resolver would be substantially broader than this identity-index fix.

## Validation

Analyzer regression tables cover module/from aliases, four comprehension forms, dict keys and values, filters and later iterables, nested forms, and a class-local first-iterable negative control. Include bound-target alias and private-name negatives. Add external Python contracts using the real public plan and byte-span replacement: a metaclass supplies an unrelated callable for direct class lookup while comprehension output changes from [5] to [-1]. Baseline and mutant use fresh interpreter processes.

## Delivery

Keep design, plan and verification report in this worktree and PR. Add no source comments except where required by Rust safety/lint contracts. Name functions and data to express scope changes. User authorized autonomous design through PR; no merge is included.
