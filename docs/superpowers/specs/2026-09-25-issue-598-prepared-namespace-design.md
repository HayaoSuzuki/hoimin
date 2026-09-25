# Prepared class namespace name resolution (#598)

## Intent and contract

Builtin and exception pair mutations require both endpoint names to be builtins. A metaclass can populate the class namespace before its body executes, or supply a mapping with arbitrary lookup behavior. AST assignments alone cannot prove builtin identity in that namespace. Fix public `plan` and `run` so custom callables do not become mutation candidates or killed mutants. Preserve ordinary module, method, closure and statically safe class cases.

The baseline is `4e3bc2a97cad2e4ce8ae2b11c970fc2f5dff9bb2`. Issue #598 supplies the failing public input and a historical 12-case Lean audit. Its runtime identity predicate is distinct from a conservative static eligibility predicate: a custom metaclass returning an empty dict still cannot be trusted statically.

## Decision

Record `may_have_prepared_namespace: bool` on each name scope. Set it for a class whose header contains any positional base or keyword argument, including starred bases and `**keywords`. An absent argument list or empty `()` remains statically ordinary. This catches explicit, aliased, indirect and inherited metaclass selection without evaluating Python. `class C(object)` and `class C(metaclass=type)` intentionally lose direct class builtin-pair candidates; resolving their headers and inherited metaclasses is outside this fix.

For ordinary and annotation lookup, skip a class entirely when the lookup is lexical from a method/function/comprehension body. Apply explicit `global` lookup before the prepared-namespace guard. For a lookup that can read a prepared class namespace, return `Unknown`; neither missing AST bindings nor class assignments prove the behavior of arbitrary mapping `__getitem__`. Preserve existing nonlocal conservatism: CPython 3.14 class `nonlocal` reads can consult the prepared mapping before the cell, and enclosing function locals already fail builtin proof.

The class header executes in its enclosing context, before the new class scope exists. Function defaults and decorators inside a class execute in its namespace. A comprehension's first iterable executes there, while its body skips the class. Declaration annotation scopes retaining class access must receive the guard, including deferred annotations; type parameters that bind an endpoint still block it independently.

Alternatives rejected: blocking every class sacrifices safe bare classes; recognizing only explicit `metaclass=` misses inherited metaclasses and dynamic bases; inspecting a `__prepare__` body cannot certify arbitrary Python mappings.

## Verification and formal boundary

Use a Lean model with separate runtime identity and static eligibility. Runtime identity depends on injected endpoints only for class-visible loads. Static eligibility also requires proof of an ordinary namespace or lookup bypass (module/method/global/comprehension body). Prove eligible pairs have builtin endpoints under the stated fixture assumptions and that methods skip class preparation. Keep broken models for ignored preparation, source-only validation and class capture by methods; atomicity and idempotency do not apply to this pure lookup model.

Generate a versioned JSONL corpus from Lean, consumed by Rust tests invoking public `run_with_io` plan/run plus isolated CPython probes. Preserve all 12 historical scope/injection combinations with their runtime expectations; deliberately change the empty custom-class static expectation to zero and explain why. Add bare class, inherited and dynamic bases, explicit globals, closures, arbitrary mapping, first iterable/body, defaults, exception pairs and annotation controls. No Python helper code is required.

Integrate model import, generator target, committed corpus freshness and sensitivity checks into `formal/HoiminOracle` and existing CI. Use one Lean process at a time, `-j1`, per-proof heartbeat bounds, a 20-second initial deadline and existing resource guard. Do not expand finite search bounds. Distinguish semantic mismatch from infrastructure failure; Lean proves model properties, while replay supplies implementation evidence.

## Compatibility and exclusions

No CLI/schema/dependency changes. Global monkeypatching of builtins or `__build_class__`, arbitrary dynamic module changes, proving custom metaclass safety, and precision recovery for known base classes are excluded. Existing annotation behavior outside shared builtin resolution is unchanged. Runtime annotation probes use repository CPython 3.14; no claim is made for unexecuted interpreters or operating systems.

## Completion

Commit this design and its implementation plan before implementation. Record three distinct reviews of design, plan, implementation and tests, including concrete findings and fixes. Run failing regression before production changes, then focused gates, workspace tests, formatting/clippy, Lean freshness/sensitivity and OKF structural checks. Root coordinates independent review and publication.
