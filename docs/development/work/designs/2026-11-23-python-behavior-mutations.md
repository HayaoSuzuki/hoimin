# Python behavior mutation expansion

Implement all fourteen candidate domains from the follow-up assessment. Discovery
remains static Rust analysis; emitted Python runs only through the existing test
runner. New IDs are enabled by default and individually excluded through the
existing CLI. Existing stable IDs retain their identity.

## Contracts and ownership

| Domain | Operator / change | Observable contract |
| --- | --- | --- |
| In-place operations | `augmented_to_binary` | Replace in-place dispatch with ordinary binary dispatch; evaluate receiver/index/RHS once in their original order |
| Unpacking | `assignment_target_swap` | Swap adjacent target roles while evaluating the RHS once; retain unpacking/assignment semantics |
| Return values | `return_value_none` | Evaluate the original return value, then return None; preserve preceding effects and function classification |
| Identity | `identity_equality_swap` | Exchange identity/equality comparisons while retaining operand order and comparison-chain laziness |
| Else suites | `else_suite_remove` | Disable loop/try else effects while retaining compiler-visible bindings, free reads and generator classification |
| Zip | `zip_strict_toggle` | Add strict=True when omitted; invert dynamic strict values without duplicate argument evaluation; avoid duplicate literal-Boolean mutations |
| Dynamic boundaries | Extend index/slice neighbors; `range_bound_neighbor` | Shift integer positions by one, including computed values; evaluate once and preserve __index__ semantics |
| Comparison directions | `compare_direction` | Reverse ordering direction without changing whether equality is included |
| Regex | `regex_quantifier_neighbor`, `regex_greediness_toggle`, `regex_alternative_delete` | Change count bounds, matching preference or one alternative, preserving supported regex syntax and valid references |
| Tuple returns | Extend `return_tuple_swap` | Adjacent swaps for larger tuples and evaluated expressions; retain original evaluation order |
| Boolean clauses | Extend `condition_clause_delete` | Cover general Boolean value expressions and while conditions; retain surviving values/order and compiler facts |
| Format options | `fstring_format_option_swap` | Change alignment, sign, grouping or presentation while preserving interpolation evaluation |
| Context managers | `context_exit_omit`, `context_exit_result_negate` | Skip a selected sync/async context's exit, or negate a protocol method's suppression result after its effects |
| Yield sites | `yield_statement_remove` | Omit one yielded element while retaining operand effects, generator classification and surrounding cleanup |

Context exit omission must preserve entry and target binding, including ordinary
and asynchronous manager protocols and nested/multiple items. A local proxy binds
special methods through the original class hierarchy, in the order requested by
native `with`/`async with`. Standard-library stacks were rejected during validation
because their class-method calls do not preserve custom descriptor binding.
Generated type methods must not retain project objects in heap-type cycles.
Helper names must be hygienic. The mutation must not suppress a failed entry. Suppression
result mutation targets __exit__/__aexit__ method return decisions, keeping cleanup
effects and exceptions; nested unrelated functions are not protocol methods.
The decision wrapper defers truth testing until native exceptional exit, after
the method's own `finally` suites. Generator protocol methods are excluded.

Removing syntax must not accidentally change locals/free variables, class cells,
generator/coroutine classification, docstrings or scope directives. Edits preserve
evaluation counts unless that effect is the named fault model. Pattern, annotation
and other existing exclusions remain explicit. Deep grouping, cancellation, source
encoding, candidate caps and bounded payload admission apply to every producer.

## Architecture

Keep central collector ownership of selection/admission/traversal. Private modules
own semantic source construction: assignment/results, boundary/call semantics,
regex parsing, format fields and context/yield protocols. Prefer existing typed
selection, token and lexical helpers. No new CLI framework or dependency is needed.
Represent choices using enums and exhaustive matches. Preserve old candidate
ordering except where expanded sites necessarily add candidates.

Generated helper snapshots are separate from ordinary name occurrences. Their
typed before-operand and after-operands phases retain per-name temporary shadows
and qualified import visibility. Visiting an original name cannot overwrite the
proof used by an injected load.

The computed subscript adapter stores the original operand on its instance.
Each invocation clears its own receiver cell before calling the conversion.
A retained conversion exception therefore releases the adapter's temporary
references, and a receiver that retains the adapter can retry. The spelling
also works inside comprehension iterables, where Python prohibits assignment
expressions.

Selection anchors and edit spans are distinct: a computed index on a later line
is selected there even when its edit covers the receiver too. Display locations
and candidate identity still describe the physical edit. An exit fallthrough
can select a compound statement's header and edit its terminating newline.
For a file whose every resolved target slice requires explicitly selected
lines, discovery has already checked that the candidate's anchor satisfies
those lines. Planning awards explicit-line priority on that basis. Saved-plan
verification repeats target resolution and rediscovers the requested IDs.

Regex count validation shares the parsed width graph and capture references.
Reverse propagation retains every assertion's contribution; a bounded sparse
representation falls back to streamed exact propagation using linear scratch
space. Dense overlapping constraints can require more preprocessing, with
cancellation checks. Candidate limits do not imply a constant discovery cost.

## Five design reviews

1. Coverage: each of the twelve table recommendations plus context and yield has
   an explicit deliverable; no difficult domain is silently deferred.
2. Distinction: in-place changes differ from dropping the old value; per-return
   changes differ from erasing a body; zip avoids already-covered literal flips.
3. Semantics: operand snapshots must preserve evaluation order, __index__ conversion,
   short-circuiting and return/yield suspension. CPython witnesses are required.
4. Scope and grammar: dead syntax must preserve lexical facts; moved targets and
   suite edits need comments, inline suites, semicolons and delimiter boundaries.
5. Complexity: existing semantic modules are reused where they own the same facts;
   context lifecycle needs its own deep helper rather than collector special cases.

Initial software-design-philosophy score: 7.5/10 (six of eight diagnostics).
Implementation locality and boundary discoverability were review obligations:
central wiring needed one owner and new helper contracts needed documentation.

After domain reviews, seven of eight diagnostics passed: **8.75/10**. Modules have
one semantic responsibility, hide substantial source-construction logic behind
typed admission, and document evaluation, lifetime and cancellation contracts.
Review corrections replaced ambiguous helper snapshots and lossy width limits
with explicit ownership. The whole-branch review identified a locality issue
in the duplicate closing-bracket calculation. The final correction gives the
name-index visitor and subscript producer the same typed site constructor and
helper-name inventory.

The final independent scoped review scored the corrected design **10/10**, with
all eight diagnostics passing. Shared helper placement and the admitted-target
proof remove the remaining implementation dependencies. Responsibilities,
interface depth and documentation, ownership, discoverability and design review
also passed. This evaluates the design boundaries; runtime verification remains
the separate obligation described below.

## Verification

Use one scenario per parameterized invocation, AAA and bounded properties. Check
actual CPython effects, object identity, aliases, destructor/exit ordering, raised
exceptions, closures, yields, cancellation and syntactic validity. Python 3.14 and
3.15 and normal Windows process execution are required. Keep private project data
out of all artifacts. No private project is needed for these language witnesses.
