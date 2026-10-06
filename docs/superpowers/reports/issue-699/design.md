# Function body erasure (#699)

Opt-in function_body_erase targets synchronous non-dunder functions/methods without
own-scope value returns or yield/yield-from. Preserve the first string docstring,
signature, decorators and default arguments; replace from the first remaining
statement to the last statement's end with pass. Skip bodies consisting only of
string/None/ellipsis literals, pass, bare return and return None. No general equivalence claim.

A scope-aware scan skips nested function/class/lambda bodies but inspects their
immediately evaluated decorators, defaults and class bases: yield in a nested
default still makes the outer function a generator. Skip annotations/type aliases
as deferred non-runtime scopes. Poll cancellation while visiting statements/expressions.

Use add_candidate with the first erased statement as anchor, not the def line.
Line/changed selection must include that first statement; selecting only later
body lines does not select the whole-function mutation. Symbol selection includes
it normally. The descriptor's original text shows the entire erased span in preview.
Shared spool-record limit (2 MiB serialized) is enforced unchanged; oversize fails
through the existing diagnostic, not a truncated partial body edit.

## Design self-review

1. Scope: nested returns/yields belong to their own bodies, while nested default
   expressions execute in the outer function; scan these separately.
2. Preservation: docstring preceding semicolon and multiline suites remains outside
   edit; header/decorators and sibling definitions cannot overlap.
3. No-ops: exclude pass/return None/docstring-only bodies, async, generators and
   special-name methods; value-return functions remain out of initial scope.
4. Selection: anchor is first erased statement and applies existing single-line
   selection contract; explicit README explanation avoids surprising later-line selection.
5. Resources/proof: shared byte-size guard remains; Lean models eligibility and
   unchanged source prefix/suffix, not Python compilation or execution coverage.
