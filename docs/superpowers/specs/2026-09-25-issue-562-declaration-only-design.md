# Preserve runtime bindings across declaration-only annotations

Issue: #562. Base: 65865ea. This is a candidate precision improvement within the existing conservative operator contract.

## Contract

A bare name annotation without a value does not change the runtime binding in module or class scope. Preserve both source and destination builtin resolution, including fallback from class bodies and later function lookups. Preserve unconditional module imports of `operator` and directly imported operator functions when only declaration-only annotations follow them. Repeated declarations have the same effect as one declaration.

Function-local annotations still declare lexical locals, even without a value. Valued assignments, existing local or enclosing shadows, namespace uncertainty, conditional imports, and operator module escapes retain their existing conservative treatment. Function-local imports remain out of scope. Attribute and subscript annotation targets must still be visited because their base/index expressions execute and can bind names or access a dynamic namespace. Annotation-expression traversal and deferred-annotation policies remain intact.

## Cause and selected design

The public CLI reproduces zero candidates for `any: int\nobserved = any([])\n`. `NameResolutionBuilder::visit_stmt` already distinguishes valueless name annotations but deliberately records tracked builtin names. Remove that builtin exception, retaining the function-local branch. Existing histories and possible-bindings then remain unchanged for module/class declarations.

`operator_functions::ImportScan` counts every Store name. Handle valueless bare-name AnnAssign before generic walking, visiting its annotation without counting its target when the current scope is module/class. Track whether the current statement scope is a function; save and restore this flag around function/class walking, with classes resetting it. Compound statements inherit it. This preserves the existing conservative function-local import guard, including nested functions and classes. All other targets continue through the generic visitor.

Alternatives considered: ignoring all AnnAssign would lose valued assignments and target-expression effects; ignoring all valueless Store nodes would lose function-local declarations. A general scope/index refactor would exceed this bounded change.

## Verification and limits

Consume the existing generated `formal/HoiminOracle/corpus/declaration-only.jsonl` unchanged in a strict public-plan regression adapter: compare exact candidate pairs and validate candidate spans by applying them. The 14 cases contain seven previously missing candidates plus seven controls. Existing Lean proves preservation, repeated declarations, and function-local non-fallback inside a small model; it does not prove the Rust analyzer. Verify model/corpus with the existing guarded audit runner.

Add Rust analyzer regression matrices for every collection source/destination pair; class/module declarations; repeated declarations; prior bindings; RHS assignments; function locals before and after use; dynamic namespace access; operator aliases; and evaluated attribute/subscript targets. Existing analyzer and annotation/operator integration suites protect deferred annotations and import conservatism. Run the workspace suite with two test threads and the isolated target directory.

No public schema, operator identifier, dependency, or CLI option changes.
