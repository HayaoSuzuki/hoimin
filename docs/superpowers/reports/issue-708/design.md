# Optional keyword deletion (#708)

Opt-in optional_keyword_delete indexes undecorated synchronous top-level defs without
variadic or type parameters. Names beginning with __ are excluded by the shared
resolver to avoid private-name ambiguity. A unique unconditional module binding must resolve at the
simple-name call site. Initially require source position after the definition's end;
this excludes forward/recursive source references. Direct class-namespace lookup is
excluded, while method bodies may resolve module names through normal lexical skipping.

Validate the entire original argument binding: positional arity, positional-only rules,
known unique keyword names, no duplicate position/keyword assignment, and all required
parameters supplied. Only an explicit keyword whose parameter has a default is removed.
Reject calls with expansion and removed values containing Named/Await/Yield/YieldFrom.
Retain other argument expressions once in original order; the removed value is not
computed and the already-created default object is used (not re-evaluated).

Reconstruct and parenthesize the complete call from its source callee spelling and parenthesized remaining
argument slices, preserving keyword spellings. Inter-argument formatting/comments are
normalized. Single-span replacement avoids comma/token surgery and handles sole keywords.

## Design self-review

1. Binding: reuse the unique-module identity logic established for enums, backed by the
   shared scope index. Reject import/conditional/repeated/local/nonlocal ambiguity.
2. Function identity: conservatively invalidate functions that escape a direct callee
   position, have relevant global declarations, or coexist with dynamic namespace access.
   This covers aliases and known defaults/attribute mutation; arbitrary external monkey
   patching or runtime reflection is outside the static guarantee.
3. Signature: map positional and keyword-only defaults separately; required and positional-
   only parameters never become deletion candidates. Validate even nondeleted arguments.
4. Source/evaluation: raw source spelling avoids Unicode normalization into keywords;
   parentheses preserve expressions. Default expressions execute only at definition time.
5. Roles/resources/proof: shared type/pattern/target guards; selected-only index, cancellation
   in scans/emission and max+1 local prefix. Lean proves abstract default binding and deleted
   argument effects, not Rust name resolution or Python source correspondence.
