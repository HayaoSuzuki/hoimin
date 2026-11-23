# Python reference mutation expansion

Implement the recommendations from the Python 3.15 language-reference comparison. The analyzer remains static Rust source analysis. New operators are enabled by default and individually excluded using the existing operator selection interface.

## Behavioral contracts

| Domain | Change | Observable distinction |
| --- | --- | --- |
| Integer spellings | Extend integer neighbors to bases, separators and arbitrary precision | Off-by-one boundaries independent of spelling |
| Conditional expressions | Swap branches; extend condition forcing and clause deletion | Conditional selection and short-circuit requirements |
| Bytes | Empty bytes literals, including concatenated literals | Required binary content, preserving bytes type |
| Floating point | Adjacent finite representable values | Rounding and threshold boundaries; retain float type |
| F-strings | Include explicit ASCII conversion; remove format specs and vary literal numeric width/precision | Representation, padding and precision requirements |
| String segments | Emit each eligible segment separately | Required prefixes, separators and suffixes |
| Await | Remove await, retaining operand evaluation | Required coroutine completion and suspension |
| Dictionary unpacking | Adjacent unpack precedence swap; unpack deletion | Override policy and required inherited entries |
| Deletion | Disable each del statement while retaining lexical bindings | Required name/attribute/item removal |
| Exceptions | Strengthen except-star type edits, handler ordering and explicit cause omission | Exception-group partition, first matching handler and chaining |
| Patterns | Case ordering, binding-free constraint weakening, class positional ordering | Structural requirements and first matching case |

An operator changes its named behavior, not arbitrary surrounding evaluation. Dictionary precedence swaps preserve operand evaluation order and validate each operand's mapping protocol in isolation before evaluating the next. They rebuild dictionaries: key insertion order and custom key collision timing can change, including running a later operand before detecting a collision with a preceding display entry. This boundary is documented and tested rather than claimed as full Python mapping trace preservation. Handler/case swaps intentionally change dispatch order. Removing a construct may intentionally remove its evaluation, but must retain compiler binding facts or decline a site with an explained constraint. Pattern edits must preserve captures and syntactic reachability.

## Architecture

Private semantic modules own literal representations, expression behavior and dispatch. They emit span replacements through the existing bounded candidate collector. The collector owns selection, annotations and assignment-target exclusion, payload admission and cancellation. Append stable typed operator variants; do not dispatch on strings. Avoid a generic mutation framework and avoid one public interface per operator.

Existing integer index/slice policies retain their zero exclusions while sharing representation support. Candidate emission is bounded and cancellable; expression replacements are rendered after admission, while literal builders allocate bounded text before final admission. Existing protected string roles, annotations, pattern restrictions and source encoding rules remain applicable.

## Design review passes

1. Scope: all eleven recommended domains have explicit deliverables; existing operators are extended instead of duplicated when their fault model already matches.
2. Python semantics: dictionary source swapping would also reorder effects; preserve evaluation. Dead deletion syntax preserves local-name classification. Pattern weakening cannot discard captures.
3. Syntax: parenthesize expression replacements, preserve float/bytes types, guard terminal irrefutable cases and bare handlers, and exercise inline suites and adjacent string tokens.
4. Resources and identity: use streaming callbacks, cancellation and the established candidate sink. Append IDs and preserve old IDs and selection anchors; no target-code execution during discovery.
5. Complexity: numeric spelling belongs in one helper, field formatting in one string module, dispatch in control modules. Keep domain facts out of the central visitor. Tests must execute observable witnesses in addition to parsing.

## Software design philosophy assessment

Initial score: 8.75/10. Seven diagnostics pass: domain purposes are concise; emit interfaces hide source construction; shared selection stays centralized; binding and evaluation contracts are explicit; design review is included; each domain owns its policy; modules are discoverable. The eighth (demonstrated strategic simplification) remains pending until implementation review verifies that new traversal wiring does not duplicate admission logic. Reach 10/10 by consolidating repeated wiring and documenting only actual invariants.

Final source-review score: **8.75/10**, seven of eight diagnostic rows. The
independent whole-branch review rated the design 9/10 and found no actionable
blocker after the lexical and delimiter fixes. This evaluation does not replace
the execution checks in the implementation plan.

| Diagnostic | Result | Evidence or remaining work |
| --- | --- | --- |
| Concise module purpose | Pass | Numeric representation, format fields, expression effects and dispatch have separate owners |
| Deep interfaces | Pass | Emit callbacks hide token, binding and grammar decisions |
| Implementation changes remain local | Partial | Numeric renderers and collector agree on one additional grouping pair; move numeric delimiter admission into the numeric module to eliminate this coupling |
| Interface contracts documented | Pass | Source roles, stop callbacks, lexical retention and grouping requirements are explicit |
| Design reviewed with code | Pass | Five design passes plus independent task and whole-branch reviews |
| Modules hide important decisions | Pass | Radix arithmetic, field grammar, dead syntax and clause movement stay in their domains |
| Boundaries discoverable | Pass | Private modules and their emitter comments identify ownership |
| Strategic improvement included | Pass | Deferred expression rendering, shared wildcard invalidation names and try traversal reduce repeated knowledge |

The remaining coupling is a small explicit internal contract. Literal builders
also allocate bounded replacement text before final sink admission; defer those
allocations if profiling identifies material cost. Neither warrants introducing
a generic rewriting framework for this change.

## References

- [Expressions](https://docs.python.org/ja/3.15/reference/expressions.html)
- [Lexical analysis](https://docs.python.org/ja/3.15/reference/lexical_analysis.html)
- [Simple statements](https://docs.python.org/ja/3.15/reference/simple_stmts.html)
- [Compound statements](https://docs.python.org/ja/3.15/reference/compound_stmts.html)
