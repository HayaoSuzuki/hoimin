# Iterator stop insertion validation

Scope: Windows, CPython3.14.9/3.15.1, Rust1.99.0; parent
`23837533e00a42445d7026147aa0452282e153c4` (loop break insertion).
No dependency/parser/schema changes.

## Design self-reviews

1. Restricted stop exceptions to synchronous __next__/async __anext__ methods.
2. Distinguished zero-item exhaustion from dropping a later item.
3. Chose one entry insertion after the docstring, retaining the original body.
4. Required builtin resolution and own-scope generator exclusion.
5. Covered source layout, selection/cancellation/budgets and opt-out defaults.

## Implementation self-reviews

1. Class/function/module context restores across nested definitions.
2. Own-scope yield inspection includes evaluated nested defaults, excludes bodies.
3. Stop exception names participate in binding uncertainty, outside the pair catalog.
4. Function-entry resolution sites belong to the method's scope.
5. Catalog/default/CLI/ranking fixtures and explicit saved-plan behavior checked.

## Validation self-reviews

1. Four new initial analyzer tests passed after missing-name RED.
2. Core/analyzer suites passed; generic parameters/late global bindings checked.
3.84 CLI-config,3opt-out/explicit/saved-plan and9valid-corpus tests passed.
4. Genuine sync/async iterator execution passed on CPython3.14.9 and3.15.1,
   initially LF/CRLF and then CR-only after the layout fixes.
5. One fresh reviewer found a global-declaration syntax error and two layout
   omissions. Three dedicated regressions failed before repair and passed after.

## Review-fix self-reviews

1. Exclude own-scope global declarations of the inserted name, including branches;
   later global declarations make earlier name use syntactically invalid.
2. Preserve docstrings sharing an indented line with the next statement.
3. Recognize universal newline boundaries, including CR-only EOF; also repaired
   the inherited loop-break helper's wrong CR-only indentation.
4. Represent lexical context explicitly as Module/Class/Function. Final typed
   context passed6iterator analyzer tests and84config/3selection tests.
5. Final all-target/all-feature Clippy passed. The obsolete workspace run was
   intentionally interrupted to apply review fixes; its partial results are not
   reported as a complete successful run. Corrected workspace run is pending.

## Other checks and resource scope

Documentation tests:21passed. Documentation site and source/link inventory passed,
with no missing or unreachable local references. Core/input/candidate payload
limits are retained; fuzz seed added, no local Windows ASAN campaign.

Build jobs2, incremental/debug artifacts disabled. Parent break worktree was
cleaned with cargo clean (5.6GiB reclaimed) before this phase completed. Final
workspace result, delivery reviews and cleanup are recorded at delivery.
