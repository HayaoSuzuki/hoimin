# Private annotation imports (#611)

Import-dependent annotation pairs require both endpoints to retain import provenance. Python compiles class private names using a class-specific namespace key, while the collector currently tracks AST spellings. The issue demonstrates `__Alias` and `_C__Alias` referring to the same binding with different write histories.

## Chosen policy

Use conservative exclusion rather than normalize the whole `KnownImports` flow engine. Store an optional private class prefix on name scopes. Inherit it into functions, comprehensions and type-parameter scopes: compiler mangling persists when runtime lookup skips the class. Reset it at each class body using the class name with leading underscores removed; an all-underscore class resets it to none. Ordinary aliases and trailing-dunder names retain current behavior.

Reject a raw private annotation import root and its transformed spelling under the active compiler class context, for both source and destination. Qualified module aliases use the same root guard. Even an unmodified private import or explicit transformed alias inside its matching class context is excluded; full precision is not promised. Preserve private aliases at module scope and inside underscore-only classes where no transformation occurs.

Reverse-spelling writes need an additional safeguard: an import spelled `_C__Alias` can be overwritten by `__Alias`. When the builder sees a private binding whose transformed key matches an imported name, record that transformed key as conservatively unstable in the binding's actual destination scope, using the existing `usize::MAX` import-write sentinel. Track these bindings even when only the transformed spelling was imported. Add the transformed spelling to tracked global/nonlocal directives so writes reach the correct owner. Keep ordinary raw tracking intact. This does not reject names merely because an unrelated class elsewhere has a matching name. A private write followed by a clean explicit transformed import may remain excluded, which is an accepted precision loss.

The private-root rejection occurs before the function-local unevaluated fast path, because private spellings have no certified import identity even there. The prepared-namespace guard from #605 remains independent. No runtime operator behavior, source text, AST names, or full import-flow normalization changes.

Python reference: https://docs.python.org/3.14/reference/expressions.html#private-name-mangling . Local CPython probes confirm private globals write the mangled module key and nested underscore-only classes reset the outer prefix.

## Formal correspondence

Retain the issue's 36 cases: 3 class spellings (`C`, `_C`, `___`) × 3 aliases (`Alias`, `__Alias`, `__Alias__`) × 2 endpoints × 2 overwrite settings. Retain runtime key/identity/permission expectations unchanged. Add a static candidate-count projection that excludes transformed private aliases even without overwrite; prove this policy implies runtime permission. Preserve last-write theorem and broken raw-key/leading-underscore/trailing-dunder witnesses. Keep enumeration and serialization in a separate executable.

| Premise / observation | Lean | Public configuration / observation | Mode |
| --- | --- | --- | --- |
| Class / alias spellings | Strings over fixed domain | plain class and import-as source | strict |
| Namespace key | mangle | CPython class dictionary key | strict |
| Overwrite and identity | ordered environment, allowed | effective-key assignment and evaluated probe annotation | strict |
| Candidate permission / static count | allowed / eligible | target-line public plan pair and span | strict |
| False kill | two original overwrite fixtures | explicit baseline and public run | strict |
| Broken key rules | fixed witnesses | Lean sensitivity | model-only |

The public adapter's observation code stays outside the analyzed source. Reverse spellings, global/module routing, nested methods/functions, unrelated classes, qualified aliases and the other annotation operators get focused Rust/public regression controls. No metaclass, long identifiers, arbitrary dynamic writes, future-string evaluation or broad Python compiler model is claimed. Maximum two writes, 36 finite cases, no transitions or concurrency. Atomicity/idempotency are inapplicable.

## Gates

Design and plan committed before code; three actual self-reviews of design, plan, implementation and tests. Observe unit/public RED, then GREEN, full workspace, exact CI Clippy/fmt/Ruff and CI registry tests. Lean: one globally reserved process, external 20 seconds / 2048 MiB, local heartbeat 10000, no limit increase. Parent owns independent review/publication; preserve artifacts.
