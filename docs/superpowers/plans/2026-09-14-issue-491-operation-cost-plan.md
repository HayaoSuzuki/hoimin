# Issue #491: Remaining acceptance implementation plan

1. Define minimal cost/semantic model, prove reuse of bounded-retention limit and cost decomposition, generate all event/source cases and finite broken witnesses. Run a deliberate wrong-bound RED before restoring proofs and generating corpus.
2. Implement real Rust instrumentation and strict corpus adapter, observe actual broken scan/clone/build rejection, and add isolated allocator preflight peak controls. Agent issue_476 owns rust.rs and workspace_preflight_heap.rs.
3. Extend bounded workload fixtures and registry while retaining 21 active gates. Parent owns tools/performance_shapes.py, tests/test_performance_shapes.py and shapes.json, and measures fresh baseline/candidate binaries.
4. Add serial guarded Lean build/freshness/sensitivity and actual deterministic gate references. Run new tests, all gates, meaningful regressions, Clippy/format/Python contracts; separate model proof, counter correspondence, allocator peak and release RSS evidence.
5. Record three actual reviews per stage, mode worksheet and exact limitations, update OKF sources/index hashes, commit/push and create PR.

Plan reviews: (1) Assigned files explicitly to prevent concurrent edits; runtime Python lane and Lean lane have separate owners and observations. (2) Required zero/one and tree boundaries plus duplicates/nonmonotone semantics, so a single large happy-path test is insufficient. (3) Retained real broken operations and numeric preconditions, and made release evidence depend on actual expanded fixtures rather than rerunning historical input shapes.
