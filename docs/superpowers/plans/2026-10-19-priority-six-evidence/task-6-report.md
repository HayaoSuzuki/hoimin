# Task 6 (#1005): shared eager-import graph analysis

2026-10-19. Base: `faff6415d6a54f71ad881df97e521cc83ac19894`, after the independent Task 5 gate. The controller approved iterative Kosaraju and the user's R4 deterministic conservative depth rule. This record concerns Task 6 only; final integration and native Linux/Windows validation remain controller-owned.

## Delivered behavior

The exception index builds one graph of parsed source-path identities from normalized eager absolute dependencies. Only mapped destinations with parsed summaries become edges. Lazy/missing dependencies are still examined for cancellation but create no edge. Already dynamic modules remain graph vertices, and their existing flags are never cleared.

A private `exception_import_graph.rs` owns dense IDs, reverse adjacency, iterative Kosaraju, implicit condensation-DAG evaluation and exclusion application. Both searches mark vertices when pushing, so neither diamonds nor alias edges duplicate pending vertex expansion. Actual cycle members are rejected unconditionally. Other modules are rejected only when the approved SCC-weighted depth reaches 256:

```
H(C) = min(256, max(terminal(C), max(C -> D, w(C) + H(D))))
terminal(C) = w(C) for cyclic C, otherwise 0
```

Here w is the SCC's module count. Acyclic depth is longest-path edge count. Depth 255 remains eligible and 256 does not. A one-edge prefix into a 254-node cycle stays eligible; into a 255-node cycle it is excluded. The old traversal was name-order dependent on joined paths; the two isomorphic real-bound fixtures now both reject. A large branching cyclic component can conservatively exclude an additional prefix even when its simple paths are short. No prior module exclusion is relaxed. The separate class-ancestry limit stays unchanged.

Graph visits and temporary storage are O(V+E), with no per-start reachable set. Construction also scans D raw dependencies and uses existing BTree name/path resolution: ordered-map/string comparison costs remain outside the linear graph-visit claim. CLI file/source limits and all direct-API behavior remain unchanged; there is no new vertex cap. Scratch graph storage drops before class/visibility construction. Cancellation polls each dependency and traversal edge, each node/component and final application; a cancelled build returns no usable index. Allocation/free internals and linear scratch destruction are not forcibly interruptible.

No protocol, candidate ID, fingerprint, line-selection (#1006), visibility-index (#1008), #806 or production Creusot changes were made. Usage, development guidance and the existing analyzer OKF concept describe the new conservative rule and evidence limits.

## Baseline and measurement provenance

Before any Task 6 product/instrumentation edit, built the clean Task 5 HEAD in the shared target, preserved `baseline-hoimin`, built the pure library, selected exact `.rlib` artifacts from Cargo JSON, and linked/preserved `baseline-heap`. The recorded product status was empty. The issue-exact DAG/control source fixtures were generated, compiled for syntax without executing Python, hashed, and sanity-run at all four N before edits. Both baseline executables remained outside the target while candidate artifacts replaced shared build outputs. No second checkout or large target was created.

All Cargo commands, including the commit hook invocation, use `CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=/tmp/hoimin-priority-six-target`; process/filesystem tests use two test threads. Native environment: macOS 15.7.7 arm64, Python 3.12.6, rustc 1.99.0 (`b940084d7`, 2026-09-28), release optimization with thin LTO. The standalone allocator probe uses the same source before/after, `rustc -O -C lto=thin`, and exact release `hoimin_python`/`camino` artifacts. Initial probe linkage without `-C lto=thin` failed because Apple LLVM 17 could not read Rust LLVM 23 bitcode; using Rust's thin-LTO path resolved it before baseline preservation completed. This was tooling setup, not a product failure.

`provenance.json` records source snapshot hashes and compiler/profile settings. `baseline-candidate-measurements.json` records executable, probe, runner and fixture-manifest hashes, all runs and medians. `fixtures.json` preserves every path/content hash. The baseline/candidate normal and heap executables are task artifacts under `/tmp/hoimin-priority-six-task6/`, not committed files. Repeated full plan stdout/stderr also remain there.

## RED and focused validation

`red.log` ran the old Rust traversal with only a test-only pop counter and new public-index regression tests: **5 failed, 5 passed**. The joined DAG with the short lexical branch first was incorrectly accepted. All four DAG work ceilings failed. Actual old pending-pop counts matched the issue's independently stated formula `N^3 + 2N^2 + 3N + 1`: 4,657 / 34,913 / 270,529 / 2,130,305. The long-branch-first and missing-import controls passed. This is running old-behavior RED evidence, not a compile failure or Python-only model.

Final hierarchy tests include an independent positive transitive-closure SCC reference followed by explicit whole-component-path enumeration; a separate legacy-order model checks old-exclusion containment. Four properties each use 256 cases, seed `PROPTEST_RNG_SEED=1005`: reference equality, old containment, permutation/reversed-edge invariance, and independently enumerated DAG depth. Literal real-256 cases and source-to-candidate/diagnostic checks supplement the reduced-limit properties. All new tests use Arrange/Act/Assert and one fixture or generated property/parameter case per invocation.

Executed commands and outcomes (all exit 0 unless identified RED):

- `cargo test -p hoimin-python exception_hierarchy -- --test-threads=2 --nocapture`: **77 passed**; `green-final-python.log`. Includes graph properties, cycle/prefix/depth boundaries, normalized relative imports, aliases, lazy/missing/dynamic vertices, deterministic cancellation and operation counts.
- `cargo test -p hoimin-cli --test exception_hierarchy --test exception_hierarchy_scopes --test lean_exception_hierarchy_oracle -- --test-threads=2`: **22 + 5 + 2 passed**; `cli-focused.log`. Existing oracle corpus was consumed unchanged; no new formal proof is claimed.
- `cargo test --release -p hoimin-python layered_ -- --test-threads=2 --nocapture`: **8 passed**; `release-counts.log`.
- `PATH=/tmp/hoimin-priority-six-venv/bin:$PATH python -m pytest -q docs/maintenance/2026-10-19-issue-1005/test_measure.py`: **8 passed**; `runner-tests.log` (initial) and `runner-tests-final.log` (after maintenance-tool lint/type cleanup). Literal fixture counts, control source names, semantic output rejection and manifest change detection are covered.

An initial focused clippy run reported six style findings in new tests (range loops, formatted-string collection and empty-string construction). They were corrected; final focused lint passed as recorded below; mandatory commit-hook output is retained in the task handoff log. The initial failing lint log remains under the task temp directory and is not presented as a passing check.

## Operation counts

Counters exist only under `cfg(test)`, follow thread-local existing test conventions, and increment at actual loop items, including skipped dependencies/visited nodes/intra-SCC edges. No permanent runtime counters or measurement API were added. New total work is exactly `10V + C + D + 4E` for this implementation, where C is SCC count; the regression ceiling `16V + D + 4E + 1` is a fixed conservative input-derived bound. Separate assertions establish D dependency examinations, E retained-edge operations, V+E forward frame/edge events, and E reverse/depth edge scans.

| N | V | DAG E | Old DAG pending pops | New DAG all-phase events | New missing-import all-phase events |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 16 | 49 | 512 | 4,657 | 3,099 | 1,051 |
| 32 | 97 | 2,048 | 34,913 | 11,307 | 3,115 |
| 64 | 193 | 8,192 | 270,529 | 43,083 | 10,315 |
| 128 | 385 | 32,768 | 2,130,305 | 168,075 | 37,003 |

The old and new columns count different events: old stack pops versus all new graph-stage loop events. They establish the removed repeated-work shape and the new bound, not a homogeneous-operation speed ratio. Parsing, name comparisons, visibility and allocations are not counted by these counters. Debug and release new counts agree.

## Final release time and semantic controls

Each fixture has exactly 3N+1 files and issue-exact bytes 17,695 / 70,127 / 279,439 / 1,115,855. Controls keep the files/bytes but replace m/l import-source prefixes with absent x/y names. Their target is identical. Normal CLI command:

```
BINARY plan --root FIXTURE --file target.py --operators exception_hierarchy --max-candidates 1 --analyzer-timeout 5s --allow-best-effort-memory -- true
```

One warm-up and five retained process runs per binary/shape/size, alternating binary and shape order; no concurrent build or measurement. The final matrix has 96 observations including 16 warm-ups. Every run exited 0 with one candidate, no diagnostics and `truncated=false`. Complete canonical candidate arrays have the same SHA-256 across both versions/all sizes/both shapes:
`8af7ea6a8250430ea07dc972610146ffb6e9469206cee6ad4090dca0dc6054d7`.

The final wall timer surrounds `subprocess.run(capture_output=True, timeout=30)` and includes process startup, CLI work and parent pipe communication. It excludes later artifact-file writes. Fixed inputs produced at most 37,292 stdout bytes and zero stderr bytes; the runner also asserts at most 8 MiB per captured stream after completion (this assertion is not a streaming allocation cap). The timer is end-to-end, not an isolated graph timer. All samples and semantic hashes are retained. No timing/RSS threshold is a regression assertion.

An earlier regular-file-redirection/timed-wait matrix showed polling quantization on short runs. It is **superseded**, retained as `/tmp/hoimin-priority-six-task6/polling-baseline-candidate-measurements.json`, and contributes no final timing claim. The final pipe runner was rerun once on the same binaries/fixtures; no product change occurred between matrices.

Milliseconds, median [minimum, maximum] of five measured runs:

| N | Shape | Baseline | Candidate |
| ---: | --- | ---: | ---: |
| 16 | dag | 9.723 [9.448, 10.310] | 8.699 [8.649, 9.683] |
| 16 | control | 8.216 [8.045, 8.917] | 8.012 [7.423, 8.251] |
| 32 | dag | 22.735 [21.758, 24.442] | 12.499 [11.730, 12.653] |
| 32 | control | 11.553 [11.082, 14.032] | 12.139 [11.383, 12.606] |
| 64 | dag | 119.386 [118.199, 126.602] | 28.022 [27.750, 29.485] |
| 64 | control | 24.530 [24.107, 25.979] | 24.848 [23.156, 26.621] |
| 128 | dag | 918.306 [871.436, 951.913] | 80.354 [79.708, 99.510] |
| 128 | control | 66.716 [64.664, 69.926] | 68.149 [65.280, 91.973] |

## Requested heap and sampled RSS

The standalone probe preloads/sorts decoded source strings and root arguments before measurement, then measures exactly `ExceptionIndex::from_sources_with_inputs`. Its `System` allocator wrapper tracks successful allocation/zeroed allocation/reallocation requests and deallocations without allocating inside accounting. Failed realloc does not change ownership. A known allocate/reallocate/free sequence checks live-byte accounting before each probe. Counters are reset for the interval while live bytes remain correctly tracked from process start; high-water starts at the preloaded-input baseline. Index retention and post-drop snapshots precede JSON output.

Five fresh-process probes per version/shape/size plus warm-ups use the same source/probe hashes. This covers the whole pure-index build (parsing, summaries, imports and visibility), not graph scratch alone or filesystem acquisition. Requested Rust bytes exclude allocator metadata/fragmentation and native allocations bypassing the global allocator. Probe timing is instrumented and is not substituted for normal CLI timing. The exact requested peak above preloaded inputs was unchanged by this optimization; allocation traffic fell. Post-index-drop live bytes returned to the input baseline in every observed probe; this is an observed scope result, not a proof that every process lifetime is leak free.

Values below are median counts/bytes; A/B means baseline/candidate. Full initial, absolute peak, retained and post-drop values remain in raw JSON.

| N | Shape | Peak above inputs A/B (bytes) | Allocation calls A/B | Cumulative requested bytes A/B |
| ---: | --- | ---: | ---: | ---: |
| 16 | dag | 223,953 / 223,953 | 20,598 / 11,243 | 1,217,574 / 1,086,408 |
| 16 | control | 217,200 / 217,200 | 10,654 / 10,531 | 1,039,518 / 1,033,536 |
| 32 | dag | 703,953 / 703,953 | 110,938 / 40,793 | 4,572,542 / 3,806,144 |
| 32 | control | 684,034 / 684,034 | 38,484 / 38,227 | 3,602,198 / 3,590,744 |
| 64 | dag | 2,592,529 / 2,592,529 | 697,832 / 155,287 | 19,428,950 / 14,542,808 |
| 64 | control | 2,553,986 / 2,553,986 | 146,338 / 145,809 | 13,691,582 / 13,668,416 |
| 128 | dag | 10,401,809 / 10,401,809 | 4,873,295 / 605,599 | 92,120,478 / 57,770,272 |
| 128 | control | 10,324,930 / 10,324,930 | 570,833 / 569,761 | 54,299,062 / 54,252,856 |

RSS was measured in a separate matrix by the existing `lean_resource_guard.py`, with 10 ms process-tree samples, a 30-second containment timeout and 2,048 MiB containment guard. Read-only `ps` sampling required and received sandbox escalation. All 96 guarded observations (including warm-ups) ended with `reason=child_exit` and the same successful candidate hash. Monitor elapsed times are not compared with unguarded CLI times. These are sampled lower bounds on process-tree resident peaks, not exact peaks or heap bytes; short executions can finish between samples, so apparent small-size RSS reductions are especially uninformative.

Median sampled tree RSS, KiB, five non-warm-up runs:

| N | Shape | Baseline | Candidate |
| ---: | --- | ---: | ---: |
| 16 | dag | 6,256 | 4,944 |
| 16 | control | 4,816 | 5,008 |
| 32 | dag | 4,944 | 3,744 |
| 32 | control | 3,840 | 4,224 |
| 64 | dag | 11,328 | 3,872 |
| 64 | control | 4,320 | 3,104 |
| 128 | dag | 23,040 | 22,944 |
| 128 | control | 22,576 | 22,096 |

## Five design self-reviews

1. **Identity/cycle membership:** inspected parsed-path versus name identity and mapped-but-unparsed destinations. Chose path IDs and literal self/mutual/disconnected/alias cases; rejected unconditional ancestor-of-cycle invalidation.
2. **Depth policy:** traced the old pre-visited depth check, literal 255/256 and cycle-prefix 254/255 boundaries and both isomorphic joined DAGs. Applied already approved R4 without a new ruling; cyclic outgoing edges contribute H rather than terminating depth at cycle entry.
3. **Lazy/dynamic/consumer semantics:** confirmed normalized eager dependencies after existing exclusion stages, retained dynamic vertices, unchanged visibility/class-resolution order, and separated local-prefix eligibility from imported untrusted-class resolution.
4. **Cancellation:** identified hidden scans in filtered lazy iterators and high-degree nodes. Required per-raw-dependency/per-edge polling, zero-degree nodes and empty input, with deterministic stage counters rather than sleeps; recorded cleanup latency limitations.
5. **Complexity/interface/evidence:** compared iterative Kosaraju with Tarjan and per-start memoization, selected one private deep module with no exposed SCC IDs. Distinguished linear graph visits from BTree/string costs and CLI limits from direct-API inputs. Required clean baseline CLI+heap preservation before instrumentation and separate time/heap/RSS scopes.

## Five implementation self-reviews

1. **Node/edge identity:** checked `from_modules` retains the existing level/eager/mapped/present predicate, borrows path keys for integer lookup, counts/skips every dependency, and keeps dynamic vertices. Parallel aliases remain harmless; source relative-cycle and alias-identity tests exercise different stages.
2. **SCC correctness:** checked both traversals mark at push, finish only exhausted forward frames, visit reverse finish order and assign every member once. Component vectors cannot be empty; cross-component edges target later IDs, asserted in debug and tested against independent closure/path enumeration. No module-proportional recursion exists.
3. **Depth/fail-closed behavior:** inspected finite cyclic terminal weights, all outgoing edges and saturating addition; acyclic leaves initialize to zero. Real source boundary tests verify candidate/diagnostic effects. Dynamic assignment is OR-only; cancelled construction cannot publish an index. Existing ancestry resolution was unchanged.
4. **Cancellation/work inventory:** checked each Init/Dependency/EdgeBuild/ForwardRoot/Forward/ReverseRoot/ReverseNode/ReverseEdge/DepthComponent/DepthNode/DepthEdge/Classify/Apply loop and actual test hooks. Derived `10V+C+D+4E`, not a measured timing coefficient. Tests cancel at an exact work boundary through construction, both searches, DAG depth and application, including lazy and zero-edge graphs.
5. **Allocation/lifetime/design:** confirmed two adjacency directions plus O(V) state, marked stacks bounded by vertices, no per-start sets or path clones per edge, and temporary graph release before downstream stages. Test counters compile out of production; the allocator wrapper is a separate development probe. The private exclusion operation hides representation and policy together; all eight software-design diagnostics pass (10/10), with no extra public interface or compatibility wrapper.

## Five validation self-reviews

1. **Reference independence/sensitivity:** the oracle uses positive transitive closure and enumeration of complete quotient paths, not production SCCs or topological DP. The legacy model is only a containment reference. Meaningful original-Rust RED captured the order-sensitive false acceptance and all four excessive-work cases; independent properties and literal cases complement it.
2. **No false cycle prefix:** exact cycle-member literals and actual source candidate collection retain short prefixes; imports from a cyclic destination remain unresolved. Relative aliases converge onto the actual two cycle members, while a separate eligible importing module survives. Missing/unparsed and lazy-only edges do not create cycles.
3. **Depth limits/conservatism:** verified real 255/256 chains, real prefix→cycle(254/255), both lexical DAG orders and a documented additional star-SCC exclusion. Four properties with recorded seed/case counts test weighted H, DAG depth, renaming and old-exclusion containment. No whole-Rust formal proof or unchanged historical audit is claimed.
4. **Measurement method:** verified clean baseline preservation, exact artifacts/thin-LTO linkage, identical immutable fixture hashes, binary hashes and probe source, all exits/candidate hashes, output sizes and warm-up exclusion. Detected and superseded Python timed-wait quantization once. Kept raw per-run final records and separate sampled RSS; observed unchanged exact requested heap peak instead of inventing a memory-peak improvement.
5. **Scope/nonclaims:** rechecked no input-cap increase, no ancestry/visibility/line-selection/schema changes, and no production telemetry. CLI wall time includes startup/communication and all analysis stages; heap covers pure index build; counters exclude ordered-map/string costs. Five local release samples and native macOS focused tests establish neither universal speedups nor native Linux/Windows success. Existing formal corpora/proofs are unchanged.

## Reproduction and retained evidence

Run from the repository root with the task venv on PATH. `measure.py /tmp/hoimin-priority-six-task6 --fixtures` recreates issue-exact sources/manifest; `link_probe.py baseline` or `candidate` reads the corresponding Cargo JSON in that task directory and records exact rustc argv. Preserve clean baseline binaries first using the documented build commands. Then `measure.py /tmp/hoimin-priority-six-task6 --repeats 5` produces the normal/heap comparison; `rss.py` separately invokes the existing resource monitor and may require read-only process-table permission. Only immutable fixtures, saved executables and task-local artifact paths are used. The retained probe/runner sources are development measurement tools, not new hoimin commands. After measurement, the maintenance Python sources received formatting/type annotations and equivalent literal/assertion cleanup; `measured-runner.py.txt` preserves the exact executed runner bytes whose SHA-256 is in the final matrix. The immutable fixture, probe and product/test source hashes were rechecked against the archive. The tidy runner's hashes are separately labeled in `provenance.json`; no timings are falsely attributed to that later formatting snapshot.

Durable evidence in `docs/maintenance/2026-10-19-issue-1005/` (the ignored SDD report points here): final per-run time/heap JSON, RSS JSON, complete fixture hash manifest, source/environment provenance, both exact probe-link argv files, runner/probe sources, exact `measured-runner.py.txt` snapshot, focused runner tests and meaningful RED/GREEN logs. Binaries, generated fixture trees, Cargo's full artifact JSON, intermediate failures and repeated plan output remain under `/tmp/hoimin-priority-six-task6/` for the final integration archive; do not delete them before that verification. No historical evidence was relabeled as rerun.

## Final pre-commit verification

- `cargo clippy -p hoimin-python --all-targets --all-features -- -D warnings`: exit 0, no warnings; `clippy-final.log`.
- `cargo fmt --all` completed; the mandatory commit hook checks workspace/vendor formatting again.
- `git diff --check`: exit 0.
- Real PyYAML parsing checked the updated analyzer concept's frontmatter/type, all four new source paths/footnotes and every new declared SHA-256; `okf-check.log`. Existing unknown/historical metadata was preserved. This is focused source/format checking, not a fresh attestation of the entire catalog.

The mandatory commit hooks run with the prescribed target/jobs and task venv PATH; their observed outcome and commit SHA are recorded in the SDD handoff after the commit finishes. Full integration and final task-independent review remain controller-owned.

Final maintenance-tool checks explicitly override the repository's default docs exclusion, so these are nonempty checks:

- `ruff format --check --no-force-exclude docs/maintenance/2026-10-19-issue-1005/*.py`: **4 files already formatted**, exit 0; `ruff-format-final.log`.
- `ruff check --no-force-exclude docs/maintenance/2026-10-19-issue-1005/*.py`: **all checks passed**, exit 0; `ruff-final.log`. Standalone-script package markers and trusted task-owned subprocess/temp paths have narrow explained exemptions; no repository lint configuration changed.
- `ty check --config 'src.include = ["docs/maintenance/2026-10-19-issue-1005"]' docs/maintenance/2026-10-19-issue-1005`: **all checks passed**, exit 0; `ty-final.log`.
- The focused runner pytest command reran after those changes: **8 passed**; `runner-tests-final.log`. The final timing matrix was not rerun for formatting/type-only maintenance; its exact executed-source snapshot is preserved.

## Completed commit and handoff

Committed as `3aaf82b0d5b695a548dda33ea248ceaa1dd68219` (base `faff6415d6a54f71ad881df97e521cc83ac19894`). Mandatory commit hooks all **Passed**: workspace cargo fmt, vendored parser cargo fmt, workspace all-target/all-feature cargo clippy, and vendored parser cargo clippy. Exact commit/hook output: `/tmp/hoimin-priority-six-task6/commit.log`; the command carried `PATH=/tmp/hoimin-priority-six-venv/bin:$PATH CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=/tmp/hoimin-priority-six-target`. Final `git status --porcelain` was empty. No hook was skipped.

Canonical durable evidence/report: `docs/maintenance/2026-10-19-issue-1005/report.md` and adjacent named artifacts. This ignored SDD report adds the post-commit hook/SHA outcome without modifying committed product/evidence snapshots. The controller owns the independent Task 6 review and any Task 7 implementation gate. Cargo slot released. No unresolved product concern; observed performance/heap/RSS limits remain as reported.
