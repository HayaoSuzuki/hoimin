# Issue 514: Preserve first-iterable lookup before comprehension body effects

Issue: https://github.com/tokyogas-tech/hoimin/issues/514
Design base: `61c654fd87cd2536082a101052d68e97d69928ca`.
Status: implemented on main `820ff2a` (including issues 513 and 515); validation evidence is recorded in the implementation plan.

## Failure and contract

Issue 481 correctly routes a comprehension's named-expression target to its containing scope, but retains the named expression's physical end offset for the containing scope's ordered effect. A comprehension body appears before its first iterable in source and executes afterward. The completed index consequently treats the body's later write as already possible while resolving the first iterable.

```python
values = [(all := lambda values: "custom", item)[1] for item in [any((0, 1))]]
assert next(iter(values)) is True
```

On the design base, CPython 3.14.7 passes this source and a manual change of only `any((0, 1))` to `all((0, 1))` fails its assertion. Public `plan --operators collection_any_all` emits no candidate. The same observation was retained for list, set, dict and generator forms in `/private/tmp/hoimin-reaudit/bindings/first_iter_*_witness.json` during the preceding audit. These are base-revision observations, not verification of a future implementation.

When both endpoints are otherwise definitely builtin, retain candidates in the eagerly evaluated first iterable. The current comprehension's body write must not affect that earlier observation. Preserve subsequent possible-write uncertainty, static function locals, directive routing, lambda boundaries, and loop backedges. The change must apply through the shared name resolver to all tracked builtin/exception endpoints.

## Selected representation

Ordered binding facts already summarize construct completion: assignment and definition bindings use statement end, while an ordinary walrus uses its own expression end. Use the enclosing comprehension's completion boundary for a named write escaping its body. This boundary means when the outward summary becomes visible to the containing ordered scope; it is not a general runtime clock. For a generator the summary represents a possible future write from creation onward, not a claim that iteration has occurred.

Attach a required boundary to the private comprehension scope variant:

```rust
Comprehension { outward_effect_after: usize }
```

The value is the entire list/set/dict/generator expression's `range.end()`. Extend `visit_comprehension_expression` to receive it. Visit `first.iter` before constructing/entering that scope, as the current visitor already does. Replace existing equality/match checks with payload-aware patterns while retaining their behavior.

When `record_named_target` walks through consecutive comprehension parents, retain the maximum completion boundary of the skipped scopes, then route the target to the first non-comprehension scope using the existing `record_target`/`record_binding` machinery. The maximum is the outermost skipped expression's completion for valid nested ranges. A lambda/function is a boundary; do not continue through it.

Only the ordered event's visibility boundary changes. Call `record_binding` immediately so static locals, `possible_bindings`, global/nonlocal ownership and loop backedge summaries remain available to every later query against the completed index. Preserve the existing conditional-depth handling and MaybeBind/Unknown effect kinds.

## Alternatives considered

A global `EvalPoint` counter would separate physical positions from evaluation time throughout the resolver. It would also require reordering every currently pre-recorded assignment, definition, import, pattern and namespace effect. A counter-only conversion would introduce RHS/header regressions. That broader traversal migration is unnecessary for this bounded correction.

Per-candidate exceptions for first iterables would hide the binding-order error and duplicate it across operators. General ancestor snapshots could express the rule but introduce state competing with facts discovered later for static locals and loops. The structured expression summary fits the current precision without adding either mechanism.

## Boundaries that must remain correct

- The first iterable executes in the containing scope before the new comprehension scope is entered. Ordinary iteration targets remain local to the comprehension.
- Function locals are static even when the comprehension is empty or a generator is never resumed. A function's first iterable calling a statically local `any` remains suppressed; it can raise UnboundLocalError in CPython.
- Module whole-scope facts remain conservative for deferred functions and comprehensions. Only direct ordered observations gain the corrected first-iterable precision.
- Empty eager comprehensions and unconsumed generators still publish a possible write summary after the expression. Delayed generator writes remain conservative after assignment, import or deletion of the same name.
- Backedge recording remains immediate. In a two-iteration module loop, a later iteration's first iterable can see the preceding iteration's write, so the candidate remains suppressed.
- A lambda-local walrus does not bind into the containing comprehension/module. Lambda defaults use their real enclosing evaluation scope.
- Subsequent sibling expressions in the containing scope see a completed comprehension's summary. Nested comprehension bodies compose through the outermost skipped completion boundary.
- Python prohibits named expressions anywhere inside a comprehension iterable, including a nested comprehension placed in that iterable. Do not invent a runtime fixture with that invalid shape. Valid nesting in the produced body, sibling expressions outside an iterable, and the scope-summary invariant cover the relevant boundaries.
- Existing dynamic namespace uncertainty is preserved. The patch does not establish precise callback, exec, globals or custom namespace effects.

There is no production dependency on issue 515. If that branch is integrated first, preserve its indirect-Class rule while updating the Comprehension payload patterns. No public candidate-ID, plan, ranking or fingerprint schema changes are proposed. No runtime Python loader is introduced.

## Required executable matrix

Use exact name candidates and source spans. For collection_any_all there should be exactly one eligible call in each positive fixture; do not count unrelated literal mutations for other operator families.

| Fixture | Expected observation |
| --- | --- |
| module list/set/dict/generator body assigns destination all; first iterable calls any | one any-to-all candidate |
| same four forms assign source any | one candidate before assignment |
| generator above remains unconsumed | first-iterable candidate retained; a separate post-creation call remains suppressed |
| nested comprehension in produced body assigns all; outer first iterable calls any | first-iterable candidate retained |
| walrus in filter after first iterable | first-iterable candidate retained |
| unrelated target; first iterable lifted to preceding statement | existing candidate retained |
| all already shadowed before expression | no candidate |
| completed comprehension followed by any in a sibling expression | no candidate |
| empty comprehension or unconsumed generator followed by module call | Unknown/no candidate |
| resumed generator after intervening assignment/import/delete | no candidate |
| function-local any walrus and any in first iterable | static-local suppression; CPython catches UnboundLocalError |
| function-local all walrus/parameter | destination suppression |
| containing global/nonlocal directives | current conservative module/closure suppression retained |
| lambda-local walrus | unrelated containing-scope candidate retained |
| two-iteration module loop, eager or consumed generator body write | first-iterable candidate suppressed by backedge |
| ordinary `(all := any(...))` outside comprehension | RHS candidate retained |

Repeat representative first-iterable source/destination cases for list/tuple, min/max and sorted/reversed. Preserve the existing global/nonlocal, static-local and type-parameter suites. The actual run witness must now have a passing baseline and one valid killed name mutation; this is restoration of a detectable mutation, not the false-kill elimination of issue 515.

## Bounded Lean extension

The existing ComprehensionBinding model proves ownership routing, but observes only after `namedWrite`; it does not derive first-iterable versus body order. Extend it with a small containing-scope expression-summary interpreter using three nodes: observe one candidate site, sequence two summaries, and comprehension(firstIterable, outward writes).

Separate declaration/whole-scope facts from ordered facts. Declaration collection happens before interpreting observations: a function target updates whole/static locals even at the first iterable, while a module target updates whole-scope possible facts without changing its ordered-before fact. The comprehension interpreter evaluates its firstIterable, then publishes each routed possible-write summary to ordered facts. A generator uses the same conservative publication at creation. Body call execution, callback effects and concrete iteration counts remain outside this model.

| Premise/observation | Lean representation | Production/source correspondence | Mode |
| --- | --- | --- | --- |
| first iterable before current body write | comprehension node interprets first before publication | one any call in a valid module comprehension's first iterable | strict |
| later sibling sees possible write | sequence(comp(no observation, write), observe) | tuple expression containing comprehension then any call | strict |
| static local before first iterable | declaration pass updates containing function whole facts | function source walrus; CPython UnboundLocalError handled | strict |
| zero/lazy write remains possible | same outward summary irrespective iterations | existing empty/unconsumed generator cases | strict |
| arbitrary nested summary algebra | interpreter/theorem over abstract nodes | arbitrary nodes need not represent valid Python | model-only |
| general loop/callback execution | not added | existing Rust/CPython regression controls only | outside this extension |

Retain the current schema 1 and existing generator by keeping exactly one observable candidate site per source row. Add seven rows: first-iterable list source, list destination, set destination, dict destination, generator destination, static function source, and post-comprehension sibling. With the existing 11 rows this gives 18 rows and 8 positive cases. Expected booleans come from interpretation, not an adapter table. Preserve all prior routing sensitivity.

Useful proofs: changing only the current body's outward summary cannot change its first-iterable observations; an initially shadowed/static endpoint stays ineligible; publication cannot restore definitely-builtin knowledge. Sensitivity must reject publication before first-iterable evaluation, omission of zero/lazy publication, and loss of static declarations. Scope ownership remains derived from the existing routing model. Source validity, unique site identity and source-to-frame correspondence remain explicit adapter premises.

All Lean work, if executed, uses one guarded command at a time with 30 s wall, 2048 MiB RSS, 250 ms sampling, `-j1` and `-DElab.async=false`. Keep enumeration/corpus generation in the executable, not imported proofs. No guard invocation or model change has occurred during this documentation stage.

## Design self-reviews

1. Re-read `record_named_target`, `record_binding`, `visit_comprehension_expression` and ordered lookup at the worktree base. The defect is effect visibility after the complete index is built, so traversal order alone cannot fix it. Selected an explicit construct-completion boundary consistent with existing statement summaries.
2. Traced static function, module whole facts, global/nonlocal, lambda, nested produced expressions and outer-loop paths. Retained immediate non-ordered effects and backedges. Removed an invalid nested-walrus-in-iterable runtime subcase and replaced it with valid sibling/nested-body coverage.
3. Compared the proposed model with the actual current routing-only model and consumer. Separated static declaration collection from ordered publication, fixed the seven new rows and total/positive counts, and limited abstract nested expressions to model-only claims. This design does not claim precise arbitrary expression or callback execution order.

## Implementation correspondence

The private comprehension scope now carries the expression end. All four expression visitors pass that boundary; `record_named_target` takes the maximum while skipping comprehension parents and immediately records the routed binding. No resolver schema or public identity changed.

The public regression target covers eight first-iterable witnesses (four forms, both endpoints), CPython assertion failures for the call-only mutations, conservative scope/flow controls, three other builtin pairs and a real killed mutation. The generated oracle retains its original 11 rows and adds seven expression-order rows (18 total, eight positive). Every row has one `any` observation; the adapter checks valid execution, candidate count, byte span and stable identity.

The summary model separates declarations from ordered publication. Its `empty` constructor is the neutral summary needed when a comprehension has no candidate observation. Proofs establish independence of first-iterable observations at a fixed declaration environment, static-source suppression, and that joining a possible shadow cannot invent builtin knowledge. This is not a proof of Python execution or the Rust implementation. The source adapter and runtime controls test that correspondence on the stated finite fixtures. Arbitrary callbacks, general expression evaluation, generator scheduling and loop execution remain outside the model.
