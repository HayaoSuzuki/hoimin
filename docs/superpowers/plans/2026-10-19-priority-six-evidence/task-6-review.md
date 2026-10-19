### Spec Compliance

- ✅ Spec compliant for Task 6 (#1005), base `faff6415d6a54f71ad881df97e521cc83ac19894`, head `3aaf82b0d5b695a548dda33ea248ceaa1dd68219`. The one-time path-identity graph replaces the per-start search; the approved R4 SCC-weighted depth rule, cycle membership, conservative existing flags and cancellation are implemented in `crates/hoimin-python/src/analyzer/exception_import_graph.rs:23`, `:61`, `:167`.
- ✅ Scope matches the brief: one private production graph module, focused source/property tests, development measurement tools/evidence and guidance. The package changes no schema, public candidate identity, class-ancestry loop, visibility implementation, line selection, production Creusot integration or unrelated feature. The hierarchy integration is at `crates/hoimin-python/src/analyzer/exception_hierarchy.rs:1077`; user-visible additional conservative exclusions are documented at `docs/usage.md:728` and `docs/development.md:2012`.
- ✅ Literal 255/256 chains and prefix→cycle(254/255), joined DAG order, DAG/diamond/cycle/lazy/dynamic/missing/unparsed/relative/alias semantics, candidate/diagnostic consequences and deterministic cancellation have behavioral tests: `crates/hoimin-python/src/analyzer/exception_import_graph_tests.rs:121`, `:145`, `:374`, `:404`, `:501`, `:557`; independent graph and containment properties are at `crates/hoimin-python/src/analyzer/exception_import_graph_properties.rs:17`, `:85`, `:124`, `:232`.
- ✅ Five distinct design, implementation and validation self-review lenses are recorded with concrete checks in `docs/maintenance/2026-10-19-issue-1005/report.md:120`, `:128`, `:136`. Recorded original-Rust RED and final focused validation support the change (`red.log:31`, `green-final-python.log:95`, `cli-focused.log:30`, `:40`, `:49`, `release-counts.log:44`, `runner-tests-final.log:2`, all under that evidence directory).
- ⚠️ Native Linux/Windows execution and combined metadata/catalog/final integration consistency remain controller/CI work, as explicitly scoped by the brief and `docs/maintenance/2026-10-19-issue-1005/report.md:3`. This is a Task 6 gate, not whole-branch approval.

### Strengths

- `crates/hoimin-python/src/analyzer/exception_import_graph.rs:96`, `:131`, `:167`: iterative finishing-order DFS and reverse DFS mark vertices at push, so stacks contain no repeated pending vertices. Kosaraju assigns source-to-sink component IDs; reverse component evaluation therefore reads completed destination heights. Cyclic terminal weight, saturated outgoing weight and unconditional cyclic rejection implement the exact approved formula, including finite prefixes rather than blanket ancestor exclusion.
- `crates/hoimin-python/src/analyzer/exception_import_graph.rs:61`: edge construction preserves mapped parsed path identity and eager absolute semantics, includes dynamic vertices, and polls even skipped dependencies. Parallel alias edges do not invent cycles. The final `|=` at `:32` cannot restore existing dynamic modules.
- `crates/hoimin-python/src/analyzer/exception_import_graph.rs:28`: graph scratch drops at the expression boundary before application and downstream class/visibility construction. Two adjacency directions, component vectors, stacks and result vectors consume O(V+E) auxiliary storage. No per-start reachable set or graph-proportional recursion remains; ordered-map/string lookup costs and raw dependency scans are correctly excluded from the linear graph-visit claim (`docs/development.md:2024`).
- `crates/hoimin-python/src/analyzer/exception_import_graph_properties.rs:17`: the reference uses positive transitive closure, equivalence classes and explicit quotient-path enumeration instead of duplicating Kosaraju or its dynamic program. The separate old traversal model tests containment. Generated cases and literal real-limit tests complement each other; the extra star-SCC exclusion is explicitly tested at `:271`. New tests use AAA with one fixture/property case per invocation.
- `crates/hoimin-python/src/analyzer/exception_import_graph_tests.rs:55`: operation assertions count actual loop items and verify separate dependency, retained-edge and traversal quantities. The ceiling is input-derived rather than wall-clock based. The reported `10V+C+D+4E` follows from the production checkpoint inventory; old pop counts are not presented as homogeneous new operations (`docs/maintenance/2026-10-19-issue-1005/report.md:45`).
- `docs/maintenance/2026-10-19-issue-1005/heap_probe.rs:46`, `measure.py:106`, `report.md:58`, `:86`: normal CLI timing, instrumented pure-index requested allocation, and sampled RSS have clearly separated scopes. Peak requested heap is honestly reported unchanged; allocation traffic decreases. Allocation/reallocation accounting preserves live ownership on failure, snapshots precede output, and the post-drop observation is not overstated as a universal leak proof.

### Checks Performed

- Read the binding brief, depth ruling/facts, implementer report and supplied diff. Read source/test/tool/doc hunks from the package; recovered only the property section truncated by tool output. Compared the durable report with the SDD report programmatically: the durable content is an exact prefix, followed by the separate post-commit handoff.
- Named outside-diff risk: cancellation during final application could otherwise expose a partially modified index, and graph lifetime could extend into later stages. Checked only the unchanged caller around `crates/hoimin-python/src/analyzer/exception_hierarchy.rs:866`: normalization and prior exclusions precede this stage; `exclude_import_cycles(cancelled)?` immediately propagates failure before class construction. The stage owns its scratch and cannot publish a successful partial index.
- Programmatically verified `provenance.json:11` product/test hashes, all retained-tool hashes, final matrix runner hash against `measured-runner.py.txt`, probe and fixture-manifest hashes, and all four saved binary hashes. Checked preserved baseline revision/status markers and successful baseline release log under `/tmp/hoimin-priority-six-task6/`. Final timing belongs to the saved pipe runner; later type/format cleanup is separately hashed. Superseded polling measurements were not used.
- Verified all eight manifests and every retained fixture file's bytes/hash; all 96 normal time/heap observations and their raw plan outputs preserve one identical complete candidate-array hash, no diagnostics and `truncated=false`. Maximum stdout is 37,292 bytes; stderr is empty. Recomputed all 16 five-sample timing medians/ranges and every heap median: all match the tables and JSON. All observed heap post-drop live values equal preloaded-input live values.
- Verified 96 RSS observations, successful child exits, 10 ms sampling, and all 16 five-sample RSS medians: all match `docs/maintenance/2026-10-19-issue-1005/report.md:107`. RSS remains a sampled lower bound, not an exact heap/peak claim. N=128 DAG normal medians independently recompute to 918.306 ms baseline and 80.354 ms candidate; missing-import controls and unchanged requested peaks limit the claim appropriately.
- Inspected recorded final test/lint outputs and `/tmp/hoimin-priority-six-task6/commit.log`: all four mandatory hooks passed for `3aaf82b0`. Final logs contain no unresolved warnings/errors; RED failures and earlier tooling/style failures are explicitly distinguished. No suite, benchmark, build, git command or subagent was run during review; checks were read-only artifact validation, with this review as the sole written artifact.

### Issues

#### Critical (Must Fix)

- None found.

#### Important (Should Fix)

- None found.

#### Minor (Nice to Have)

- None found.

### Assessment

**Task quality:** Approved.

**Reasoning:** The implementation is a bounded-stack shared SCC analysis with a small private interface and explicit conservative depth policy. Independent references, meaningful original-behavior RED, source-level boundaries, actual work counts and verified release/heap provenance support this task's correctness and measured improvement without extending the evidence to unrelated stages or platforms.
