# Insert a break at the end of a loop body

Add `loop_break_insert` as a default runtime operator, removable with
`--exclude-operators loop_break_insert` (opt-out). Preserve explicit operator
selections and saved plans. Support `for`, `async for`, and `while` statements.

Append one unconditional `break` at the end of the selected loop's body. It exits
that loop when execution reaches the end, preventing subsequent iterations and
its `else` suite. Existing conditional `continue` statements can bypass it.
Do not add `continue` insertion. Skip bodies with a direct unconditional
`break`, `continue`, `return`, or `raise`: their new tail would be unreachable.

Approaches: inserting before every statement would create many noisy candidates;
replacing the loop with an `if` would change iterator and binding semantics.
The user accepted appending `break` after the body. Use that approach, one
candidate per eligible loop. Handling source layout is an implementation choice:
retain indentation, comments, line endings and inline suites; reuse a nonempty
source anchor and the existing candidate admission, selection and payload limits.
Keep AST traversal and parser behavior unchanged. Verify generated Python with
both supported interpreters, including nested loops, loop else, and async for.

Use an isolated branch/worktree, two build jobs and no debug/incremental Rust
artifacts. Perform five distinct self-review passes at each stage, record evidence,
obtain one fresh final code review, and create a PR without merging it.
