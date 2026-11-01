# Issue 627 implementation plan

1. Commit design and plan before code. Add a production-walker counter regression
   with fixed selected content and 0/1000/5000 unrelated files. Observe current
   code failing to prune/avoid records, with a real broad-walk sensitivity control.
2. Extend ExactPathScope to source subtrees and rename it for its broader purpose.
   Index source keys and required ancestors; keep component-boundary semantics and
   native malformed-path handling. Keep normal/include walk roots and policies.
3. Test target correspondence against broad discovery for source unions/order,
   symbols, exact/line mixes, overlapping/root/file/normalized sources, ignore and
   include/exclude rules. Test native malformed names and platform case policy.
   Add public plan candidate equality and no-execution evidence.
4. Record three implementation and three test review passes. Document the scoped
   diagnostic boundary. Run related discovery/core/handler/public tests, full
   workspace, exact CI Clippy/fmt, and request independent root review. Use the
   assigned cache and clean local workspace crates when switching from Issue 629.
5. Commit implementation/evidence and publish through gh stack after review. Root
   owns CI polling, merging, and cleanup. Do not exceed two published lane issues.

## Plan reviews

1. Counter RED exercises the actual walk and collected map, not a simulated scope.
   A broad-discovery control grows with unrelated files; fixed scoped counts are
   the performance contract. No elapsed-time threshold or heap claim is needed.
2. Differential targets alone cannot prove pruning or unchanged diagnostics, so
   combine counters, full candidate-array equality, and selected/unselected invalid
   path fixtures. Inputs include non-Python files inside the scope as a control.
3. Shared-cache builds are sequential. No new Lean model is needed for this index
   extension; native-path tests plus actual walker observations target its risks.
   Core selector behavior remains the independent semantic oracle, not rewritten.
