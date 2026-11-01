# Issue #561: gate method replacement construction

The analyzer must avoid constructing a full-call replacement when its operator is not selected. It must still traverse that call's receiver and arguments, preserving selected child mutations. Candidate descriptors, stable IDs, ordering, and truncation remain unchanged.

## Cause and scope

`collect_method_call` and `collect_structural_method_call` call six replacement helpers before `make_candidate` checks selection. `replace_within_call` copies the whole call and may reallocate while editing. Nested append calls therefore repeatedly copy their large inner literal even with only `binary_add_sub` selected. `mapping_get_to_subscript_replacement` formats a full expression directly. The symmetric `subscript_to_mapping_get_replacement` has the same allocation issue and belongs to the same operator.

Add `request.operators.contains(...)` to each relevant match guard, before argument eligibility work where practical:

| Producer | Operator |
| --- | --- |
| append→insert, insert→append | CollectionAppendInsert |
| append→extend, extend→append | StructureAppendExtend |
| get→subscript, subscript→get | StructureMappingGetSubscript |
| sort→reverse, reverse→sort | StructureSortReverse |

The subscript guard belongs after index/slice candidate collection so unrelated operators remain available. Do not return from the AST visitor or combine append's two independent families into a single gate. Annotation, arity, parenthesis, comment, trailing-comma and receiver eligibility stay intact. Short attribute-name replacements and unrelated producers are outside scope.

## Alternatives

Guarding inside `make_candidate` is too late. Guarding helper internals would require passing selection into syntax-only helpers and still enter expensive helper code. A lazy closure API would alter every candidate producer unnecessarily. Local guards follow the existing collection-literal guard and keep the change bounded.

## Regression evidence

Use test-only thread-local counters at all seven helper entrances, plus the actual full-call `to_owned` and both mapping `format!` boundaries. These counters count work, not live memory or allocator bytes. Direct helper calls and selected-family cases establish positive controls so zero cannot pass because instrumentation is disconnected. Unselected cases cover all directions; append tests select each family independently. Nested append depths 1, 8, 16 and 32 retain the innermost addition with candidate limit 1 and zero method work.

Record a baseline descriptor fixture before changing guards. Compare all descriptor fields and stable IDs via public discovery, and verify each bounded prefix and truncation against the baseline. Include zero, exact, and overflow limits. Existing full analyzer tests cover annotation and call syntax restrictions.

## Measurement

Run the existing standalone System-allocator probe against debug libraries before and after the guard change. Fixtures and runtime construction stay outside the measurement; the probe asserts the single inner `+`→`-` candidate. Count successful alloc/alloc_zeroed/realloc requests at least 500,000 bytes at depths 1, 8, 16 and 32, comparing equally sized append/ignore inputs. Repeat the after measurement. Report cumulative requested bytes and call counts only as allocation improvement; elapsed debug time is observational and neither peak RSS nor a speedup estimate. Store reproducible commands and raw results with the audit.

## Constraints

- Rust 2024, minimum Rust 1.88; no new dependencies or public API changes.
- Keep all changes in the issue-561 worktree and use `/private/tmp/hoimin-issue-561-target`.
- Commit design and plan before implementation; three substantive self-reviews each of design, plan, implementation and tests are recorded in the review log.
- Root agent owns the knowledge catalog update and PR publication.
