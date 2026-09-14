# Issue #489: Valid Python candidate contract corpus

## Contract

Define source fixtures, eligible/ineligible sites and stable runtime expectations once in Lean. A Rust test adapter first compiles original bytes with CPython 3.14, then compares direct Rust analyzer candidates and public plan output with the independent site table. Every output candidate passes the shared validator, original/hash/location checks and individual CPython compilation. Zero-output fixtures declare rejected sites explicitly; the corpus must include nonempty positive controls for every producer.

Cover token, structural AST, type-annotation and operator-import producers. Fixtures span expressions/defaults/annotations, match key/value/guard, except/except*, comprehension first iterable/body, module/function/class, source/destination shadowing, global/nonlocal, walrus and generic type parameters. Apply LF, CRLF, CR, mixed, BOM, no-final-newline, Unicode/tab/comment/grouped layouts to producer representatives. Run full/focused/include/exclude/line/symbol and candidate-cap boundary variants on declared compatible representatives. Do not require an impossible full Cartesian product: print the pair coverage and uncovered pairs with reasons and register all current canonical operators with a covered or deferred explanation. New operator registration must fail until reviewed.

## Model and adapter boundary

Reuse BindingFlow resolution, AnnotationScope directed writes, ComprehensionBinding ownership and ExceptionMatchBinding target binding. Extend only a small generic-name visibility rule and finite mapping-key semantics (integer/boolean canonical equality and integral complex pairs). Reuse CandidateSpan.replaceBytes for generated one-replacement expectations; its older location model is LF-based, so Unicode/BOM/newline coordinates are checked independently in Rust against real bytes, never claimed proven by that model.

Strict cases must expose their premises in source/CLI options and compare compile/runtime/candidates through public plan. Direct analyzer calls are additional internal observations, not the reason for promoting a case. Worksheet rows carry producer, syntax position, binding, source premise, public observation, mode and model limitation. Unsupported consumer/scope pairs remain explicitly uncovered; existing annotation internal-fixture cases are not relabeled strict by analogy.

All finite rules have positive/negative witnesses and deliberately broken variants: comprehension-local walrus, ignored type parameters, lexical-only key comparison and bad byte replacement. Search the two-name and tiny-key domains only, with depths and case counts printed. Prove the reusable invariants separately from finite execution. CPython grammar, native scope/runtime, all operators and all integer/complex values are outside the Lean proof.

## Engineering

Test-only Rust/Lean/fixtures and documentation; no production Python analyzer/loader. Add serial bounded Lean CI build/freshness/sensitivity entries and update the workflow contract test. Dedicated Cargo target uses two jobs, no debug info/incremental; coordinate Lean with the parent and use the existing 30-second/2-GiB resource guard for every build/generator invocation.
