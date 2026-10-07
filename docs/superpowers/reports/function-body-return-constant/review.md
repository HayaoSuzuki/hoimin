# Return constant review record

Design and plan each record five separate reviews. Worktree base: 1865f47.

## Implementation self-reviews

1. Registry/defaults: append one opt-in ID, all 70 -> 71, default50 unchanged. Behavioral ranking and existing IDs remain stable. Update README, README assertion, cross-producer inventory and ranking inventory together.
2. Annotation selection: direct names only; use annotation_resolution at the annotation offset. Confirm later/conditional/class/outer-scope writes, wildcard imports, type parameters and custom prepared class namespaces fail closed. Parameter names do not shadow the function's own annotation scope.
3. Own scope: shared visitor adds a value-return observation; prior reject-value-return and plain-synchronous behavior stay intact. Nested bodies are ignored, but eager defaults/decorators/bases are walked. Yield/await still rejects the owning body.
4. Range/literal behavior: skip the leading docstring; replace first remaining statement through last end, retaining headers and outer bytes. Skip only a sole exact-kind literal return, using cooked strings/AST integers. Bool/int and integer/float are not conflated.
5. Integration/resources: shared candidate creation supplies owning symbol, first erased line, profile filtering, bounded retention, encoded spans, 2-MiB record cap, cancellation and saved-plan identity. No external input features or new dependencies.

## Test self-reviews

1. RED/GREEN: first public test failed with unknown operator before implementation, then all three literal pairs passed. Existing function-body-erase (6) and optional-keyword-delete (6) tests remained green after shared visitor changes.
2. Python boundaries: compile positive and negative fixtures, including deferred/later bindings and Python3.14 type parameters. Observe exact counterpart suppression, nested/eager scope differences and both constants for each annotation.
3. Source/runtime: preserve leading docstrings with UTF-8/BOM/Latin-1 and LF/CRLF/CR, inline suites and tabs; check original byte span and execute all encoded mutants. Runtime trace proves retained header effects and removed body effects on the authored example.
4. Public behavior: weak/strong saved-plan tests execute both mutants in each family; unchanged candidate identities/source and exact killed/survived counts are asserted. Private analyzer boundary tests cover owning scope/first line, prefix truncation, focused profile, opt-in/exclude and cancellation; oversized records fail with the existing diagnostic.
5. Oracle/evidence: expected pairs originate in Lean; all32 strict rows are compared through public plan, all23 mutants compiled. Parser failure controls and CI freshness/sensitivity prevent silently skipped rows. Real-source discovery failures are recorded separately from mutant outcomes; no coverage or general-effectiveness claim.

## Final validation

Independent fresh-context review found no critical, important or minor findings.
Clippy detected that the statement collector exceeded its line limit; the new block
was extracted unchanged into `collect_function_body_return_constant`, then Clippy
passed. The full suite is repeated against that final refactoring before delivery.

Final checks (2026-10-07): `cargo test --offline --workspace` against the final
refactoring passed 2707 tests with 22 intentionally ignored. Workspace fmt and
`cargo clippy --offline --workspace --all-targets --all-features -- -D warnings`
passed. `pytest tests/test_ci_workflow.py -q`: 142 passed. OKF validation parsed
25 concept YAML headers, checked four reserved files, and verified the added
source link/hash. Guarded Lean freshness and all14 sensitivity controls passed
again. Nine dedicated public tests include all32 generated cases; analyzer
selection/profile/bound/cancellation test passed in the full suite.

Delivery preserves both PR worktrees. Cargo/Lean build outputs and task temporary
files are removed after PR creation; the root workspace's existing files remain.
