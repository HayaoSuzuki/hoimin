# Local rstest_macros patch

Upstream: rstest_macros 0.28.0, https://github.com/la10736/rstest.
The published crate and its MIT/Apache-2.0 licenses are preserved here.

`src/render/apply_arguments.rs` saves the original function body's brace token
before wrapping awaited arguments, then restores that brace token afterward.
These two added statements preserve the original source span; generated Rust
tokens and runtime behavior are unchanged.

Without this patch, rustc's LLVM coverage mapping omits parameterized test bodies
because the replacement outer braces have a generated span. This also prevents
unexecuted test bodies from appearing as uncovered. The patch restores their
normal instrumentation without excluding source or marking it covered.

Isolated controls with rstest 0.28.0 and nightly-2026-10-08 verified:

- Plain and parameterized test bodies appear in the coverage export.
- An untaken conditional expression remains uncovered.
- An ignored parameterized body appears with execution count zero.
- Injected fixtures and automatically awaited future arguments pass and retain
  their body/async-closure instrumentation.
- Expanded Rust tokens before and after the patch are byte-identical.

Keep this patch limited to span preservation. Remove the local override when an
upstream release preserves these body spans.
