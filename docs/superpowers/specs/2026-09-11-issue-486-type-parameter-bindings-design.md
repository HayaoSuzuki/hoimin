# Issue 486: Resolve PEP 695 type-parameter bindings in their annotation scope

Issue: https://github.com/tokyogas-tech/hoimin/issues/486

## Failure and required behavior

Generic function and class type parameters can have the same names as builtins. The current NameResolutionBuilder visits type-parameter expressions without creating their bindings, so calls whose source or replacement name refers to a type parameter can be emitted as builtin mutations. Represent the binding's real scope and suppress only substitutions whose two endpoints are not definitely builtins. Preserve valid builtin mutations outside that scope.

Use the [Python annotation-scope reference](https://docs.python.org/3/reference/executionmodel.html#annotation-scopes) and [PEP 695 scoping rules](https://peps.python.org/pep-0695/#scoping-behavior), checked against CPython3.14.7. A type-parameter binding is not merely an ordinary local in the function or class body. It also governs appropriate header expressions and remains visible through nested lexical scopes.

## Scope representation and evaluation boundaries

Add the smallest explicit annotation/type-parameter scope representation needed between the enclosing scope and the generic declaration's body. Keep ordered module/class binding analysis, static function locals, directives, conditional flow and comprehension behavior intact. Inspect every exhaustive scope matcher and ancestor lookup; adding a variant without updating parent lookup can hide or invent bindings.

Function decorators and ordinary argument defaults are evaluated outside the generic type-parameter scope. Function annotations and type-parameter bound/default expressions use the appropriate annotation scope. Generic class bases and keyword expressions see the type parameters; decorators remain outside. Type parameters remain available to body code, nested functions and comprehensions according to lexical rules. Type aliases need deliberate handling only if their affected expressions enter runtime candidate resolution; inspect and document existing type-position suppression before expanding this patch. Keep #263 runtime-candidate exclusion in annotations/type positions: CPython identity observations can establish annotation binding without requiring positive runtime candidates there. Positive header controls should use eligible decorators, ordinary defaults, class bases or keywords.

Annotation scopes immediately inside a class can access its namespace. Ordinary method bodies and comprehensions do not thereby gain access to ordinary enclosing class bindings. A generic method must still see the class's type parameters through the annotation scopes. Distinguish these two parent-lookup paths explicitly and test both. Type parameters cannot be rebound with nonlocal; preserve legal global/nonlocal controls for ordinary names without manufacturing invalid Python fixtures as acceptance evidence.

Existing KnownImports and operator-function type-parameter handling can inform name extraction and traversal conventions. Avoid an unrelated refactor of all resolver systems. This independent branch starts from4adf809 and does not assume the #481 comprehension walrus fix.

## Verification

First reproduce false public plan candidates on CPython-valid generic declarations. Cover both mutation source and destination shadowing, generic functions/classes, TypeVar/TypeVarTuple/ParamSpec where legal, nested closures/comprehensions and restored builtin names after the declaration. Include collection, any/all, min/max, sorted/reversed and exception replacement families without relying on a single operator's behavior.

Use executable CPython identity observations for evaluation boundaries: decorators/defaults outside, annotations/bases inside, enclosing-class annotation access versus ordinary method lookup, and nested class/function type-parameter visibility. Lazy annotations and type-parameter bounds/defaults must be explicitly evaluated when their observation is needed. Require original compilation and exercise actual public candidate IDs/spans rather than only private name-resolution state.

Pair exclusions with real positive candidates at each boundary, and include an actual run reproducing a reported incorrect mutation result when feasible. Preserve original bytes. A bounded Lean scope-lookup model is useful only with derived lookup semantics, executable expected results and correspondence to public Rust observations. It must state the limits of its Python-source abstraction; a literal table of desired answers is not a formal audit. Any Lean invocation uses the existing serial30second/2048MiB/250ms guard, -j1 and -DElab.async=false. If generators/CI change, update their exact Python workflow inventory and run that module.

Run focused public/binding tests, full workspace with all features, fmt and Clippy for all targets/features. No production Python analyzer or blanket generic-declaration exclusion. IDE MCP currently has no open hoimin project; use Cargo diagnostics unless that changes.

## Design self-review

1. Read the authoritative scope rules and current NameScopeKind/NameResolutionBuilder. Identified the missing intermediate binding scope and every parent-lookup branch affected by adding it.
2. Separated generic header evaluation, body lexical visibility and enclosing-class lookup. Ordinary function locals alone cannot express these differences; compile-valid positive and negative controls are required.
3. Require public candidate observations and CPython evaluation of lazy expressions, with bounded formal correspondence only if it adds evidence. Keep the independent walrus issue and unrelated resolver systems outside the patch.

## Selected lookup architecture

Represent tracked type-parameter names as static locals in a dedicated TypeParameters scope. Direct header lookup can consult an immediately enclosing class and the ordered outer module. Function/comprehension body lookup traverses type-parameter bindings while skipping ordinary class bindings. Class-body lookup needs its own path through the parameter scope: preserve the existing ordered outer-module lookup without granting access to an ordinary enclosing class namespace. Splitting ordinary parameter defaults from annotation traversal keeps these occurrence scopes explicit.

This is the controller's accepted implementation shape within the original contract, not a compatibility waiver. Validate nested generic class header/body differences and legal global/nonlocal interactions with original CPython compilation. No new Lean/CI artifact is planned; the scoped runtime-binding observations and public candidate matrix provide direct correspondence for this change.
