# Issue 515: Skip enclosing class directives during lexical lookup

Issue: https://github.com/tokyogas-tech/hoimin/issues/515
Base: `61c654fd87cd2536082a101052d68e97d69928ca`.

## Failure and contract

A method inside `class C[tuple]` still captures the type parameter when C's body declares `global tuple`. At the base revision, the runtime builtin resolver applies that class directive during indirect lookup, skips the type parameter, and emits `list -> tuple`. CPython 3.14.7 executes the original successfully and the mutation raises TypeVar-not-callable TypeError. An actual public run reports a false killed mutant.

```python
class C[tuple]:
    global tuple
    def method(self):
        return list(range(2))
assert len(C().method()) == 2
```

Indirect lexical lookup skips the complete ordinary class scope, including its global/nonlocal directives. Direct class lookup still honors them. Function directives still apply in that function's lexical lookup. This rule also protects ordinary enclosing function bindings; it is not limited to generics or the list/tuple operator.

## Design and alternatives

Handle `Class && !direct` before directive dispatch in `NameResolutionIndex::resolve_scope`, returning `resolve_parent`. Keep the direct Class path's ordered lookup and `resolve_class_parent` fallback. The two parent functions must remain distinct: ordinary lexical lookup skips class locals, whereas direct class lookup preserves ordered module behavior and type-parameter capture.

Guarding both directive checks individually is equivalent but duplicates the skip condition. Special-casing type parameters or emitted candidates would leave ordinary closure cases wrong. No public type, schema, ranking, or fingerprint change is required. No comprehension execution-order work belongs to this branch; that is issue 514.

## Regression requirements

The public plan tests compare actual name candidates and spans, excluding independent collection-literal mutations. Cover ordinary and generic methods, lambda, eager comprehension, and consumed generator descendants. All must suppress a destination captured from the generic annotation scope. Positive controls are direct class body under its global declaration, a method with its own global declaration, and an unrelated outer module call.

An ordinary enclosing-function fixture must suppress a method replacement referring to that function's shadowing binding even when the intermediate nongeneric class declares global. CPython identities independently establish each binding. Preserve existing decorator/default/header/type-position, static-local, nonlocal, exception, and whole-module uncertainty tests. One actual run must show baseline Exit(0), empty mutants, zero killed, complete report, and unchanged source for the original false-kill shape.

## Formal boundary and correspondence worksheet

At the base revision, the shared BindingFlow model checks a class directive before `direct`, reproducing the same model defect. It has no TypeParameters constructor. The smallest useful correction changes its indirect-Class rule and proves that all class bindings/directives are skipped. Use ordinary closure cases for exact model/source correspondence; generic capture remains a separate CPython/public-CLI observation.

| Premise or observation | Lean representation | Production configuration | Public observation | Mode |
| --- | --- | --- | --- | --- |
| method skips class global to find outer function binding | function/class-global/function-shadow/module frames | ordinary enclosing function + class + method | no list-to-tuple candidate at designated call | strict |
| direct class global uses builtin module binding | direct class-global/function-shadow/module frames | direct class-body list call | retained list-to-tuple candidate | strict |
| method-owned global skips outer closure | function-global/class-global/function-shadow/module frames | method's own global tuple | retained candidate | strict |
| any indirect class directive is ignored | theorem over arbitrary Frame and rest | not every arbitrary frame list is valid Python | Lean equality only | model-only |
| TypeVar is captured across class global | model has no TypeParameters variant | generic class with global tuple | CPython identity + public candidate/run | not a Lean case; executable regression |

The worksheet uses `model-only` for the universal abstract frame claim; do not present it as a proof about all Python ASTs. Generated strict corpus rows use ordinary Python closures and actual candidate observations. Source validity, frame-path correspondence, and site identity are explicit adapter premises. No theorem substitutes for those checks.

Sensitivity must detect directive-first class traversal. Direct-class-global and method-owned-global positives also reject a broken rule that skips every directive. Atomicity and idempotency families do not apply to this pure scope lookup. Existing binding-flow joins/loop/finally sensitivity remains intact.

Use the existing generator/corpus. Imported model/proof modules remain cheap; corpus evaluation stays in the existing executable. Every Lean invocation uses the serial repository guard: 30 s wall, 2048 MiB RSS, 250 ms sampling, `-j1`, `-DElab.async=false`. Record failures and peaks without raising limits. Cargo execution, Git integration, and external publication are root-coordinated.

## Design self-reviews

1. Source review: followed Function -> Class indirect lookup and found the global/nonlocal check precedes the Class direct test. Chose complete class skipping, which also fixes an ordinary enclosing-function witness; no operator-specific exception.
2. Boundary review: CPython identity fixture confirms five descendant forms capture C's TypeVar while the class body and method-owned global see builtin tuple. Kept direct `resolve_class_parent` distinct and excluded invalid TypeVar nonlocal fixtures.
3. Correspondence review: inspected BindingFlow's actual class branch and its existing normal-directive theorem. Restricted new strict model cases to ordinary closure frames and identified generic capture as external execution evidence. Preserved existing conservative loop/module/local rules and split issue 514 into another branch.
