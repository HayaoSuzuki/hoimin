# Exception hierarchy: scope identity and analysis outcomes

Issue #692, branch `investigate/user-defined-exception-mutations`, base `97c5bcd`.
Implements the accepted follow-up from the post-repair design review.

## Analysis outcomes

Separate candidate production, intentional ineligibility and incomplete analysis.
`ExceptionIndex::collect` returns a bounded report instead of a skipped boolean.
Keep reason/count/first location per reason, not one entry per possible candidate.
Reasons distinguish unresolved or untrusted bindings, unsupported expressions and
disabled scopes from no related class, no visible destination and constructor policy.
No-related-class, constructor-policy and bare-raise exclusions do not warn that
analysis failed. Disabled scopes retain a reason at each visited mutation site.
Emit at most one existing `UnsupportedExceptionHierarchy` diagnostic per file,
summarizing skipped reasons and pointing to the earliest skipped site. Candidate
JSON fields, operator defaults, ranking and truncation semantics remain unchanged.

## Scoped write facts

Keep definitions and candidate spellings unchanged. Write-analysis names become
`BindingKey { scope, name }`, within one module summary; module identity remains
the owning summary's path. Scope 0 is the module. Lexical scopes get deterministic
preorder IDs. Normalize private names using the enclosing class before resolving.

Collect declarations before resolving references, so later assignments determine
function locals. Parameters and local imports belong to the function; global goes
to scope 0; nonlocal goes to the nearest enclosing function binding. Free variables
follow lexical parents, skipping class locals for methods. Class-body loads may
refer to the class namespace or outer binding; retain both where order is unknown.
Definition headers execute in the enclosing scope, separate from function/class
bodies. Lambda parameters and comprehension targets must not become module names;
comprehension first iterables are evaluated outside their implicit scope, and
assignment-expression targets bind in the containing non-comprehension scope.

Reuse the bounded reverse worklist over typed keys. Keep all possible assignment
edges across rebinding; do not infer call arguments/results or container aliases.
Only reached module bindings become module-level Unknown. Imported providers are
invalidated through reached import keys in any scope. Preserve existing eager versus
deferred dependency behavior and all input fingerprints.

The existing scope index has different flow-sensitive responsibilities. Avoid
coupling the new project summary to its full retained occurrence index; extract a
small declaration/resolution helper for this feature. Keep scope/local tables
bounded in addition to the existing 65536 edge/import/write limits. Resource errors
reject the index, rather than publishing partial facts.

## Verification and review

Public tests compare identical candidates after renaming an unrelated parameter,
and reject actual global/nonlocal/free-variable writes. Include class/method,
private names, headers, lambda and comprehension controls. Diagnostics test all
three outcomes, first location, aggregation and unchanged candidate limits.

Lean uses scoped keys for alias facts. Retain the path-coverage/provider-invalidation
invariants; add a scoped-name distinction property and public/extraction fixtures
for the lexical boundary. Compare complete keys rather than flattening scope away
in the adapter. Old public cases stay unchanged; old extraction expectations must
be regenerated for the intentional key representation change. A model theorem
still does not prove the parser or Rust implementation in general.

Design review 1: numbering scopes alone is insufficient; global/nonlocal/free
references must resolve before edges are connected. Design review 2: class reads
can fall back to outer names, while methods cannot close over class locals; model
that asymmetry explicitly. Design review 3: a missing replacement is not a failed
analysis; preserve the three outcomes before formatting a bounded diagnostic.
