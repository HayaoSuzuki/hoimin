# Task 5: loaded-module hierarchy visibility (#995)

## Change and scope

`ExceptionIndex::build_visible` now starts with the importing module's existing
ordered `loaded` map. For each module binding it queries the exact imported
name, then the contiguous `imported.` descendant range, and looks up only those
names in the authoritative project `module_paths` map. The dotted prefix is
constructed once per binding. No public API, persistent index, or dependency
was added. Visibility accounting still decrements the shared 65,536-entry
ceiling for each retained class spelling, and binding/load positions still use
the later of the two offsets. Cancellation is checked for every visited loaded
module, including modules with no class members.

## Requirements review

1. Re-read issue #995 and its audit table: the target must contain real classes while unrelated helpers contain 32 external imports each. The regression uses that exact shape at N=32/64/128/256.
2. Checked the positive requirement separately from the cost claim: a package alias must expose an explicitly loaded child's class, and lexical spelling choice must survive the optimization.
3. Checked the existing import contracts: relative imports, aliases, configured roots, ambiguous module identities and unloaded descendants remain governed by existing resolution. The lookup still consults `module_paths`, so loaded names do not bypass ambiguity exclusions.
4. Checked resource requirements: preserve the shared 65,536 retained-spelling limit and cancellation; add a dedicated empty-target loop cancellation case rather than relying solely on class-member checks.
5. Checked exclusions: no timing threshold, blanket CPU/RSS claim, empty-class shortcut, CLI knob, additional permanent state or public API. Final workspace verification remains integration Task 8's responsibility.

## Design review

1. Chose the existing ordered loaded map as the search origin; project mappings remain the authority for valid unique module paths. This avoids duplicating state or changing callers.
2. Split exact lookup from the dotted descendant range. The dotted lower bound and prefix termination explicitly enforce component boundaries and keep similarly named loaded packages outside the descendant set.
3. Checked traversal equivalence: exact name sorts before every dotted descendant, and both existing maps order names lexically. Filtering loaded names before project lookup retains the old relative visitation order of valid modules.
4. Kept prefix construction and borrowed iteration local to the existing visibility builder. The test counter is thread-local and compiled only under `cfg(test)`, with no always-on metrics.
5. Added cancellation at the loaded-entry boundary, before checking for a valid project path or class members. Existing per-binding and per-member checks stay in place. Local software-design-philosophy score: 10/10 across the eight diagnostic rows; no failed row or new interface burden for callers.

## Implementation review

1. Compared old/new visitation sets: old accepted exactly imported-or-dotted names present in both maps; new exact-plus-prefix chain yields loaded names and rejects absent project paths by `module_paths.get`. No imported class can bypass that lookup.
2. Checked range termination and string slicing: all descendant names start with the dotted prefix, so slicing at `imported.len()` stays at a UTF-8 boundary and preserves the previous spelling construction. The exact case uses an empty suffix.
3. Checked lexical aliases and binding order: the same target binding keys, class resolution and final alias sort are used. Exact/descendant order is unchanged for names present in both maps.
4. Checked offsets, ceiling and errors: retained aliases still use `binding.end.max(loaded_end)`; both `checked_sub` sites and `HierarchyLimit` remain unchanged; cancellation now covers empty targets as well.
5. Read the scoped diff for leakage: the sole runtime change is inside visibility lookup plus a standard-library Bound import. Test hooks disappear outside test builds; the performance registry names the actual non-ignored test and makes only a module-visit claim.

## Tests review

1. Established behavioral RED on the production source-to-index path: visits [33792, 133120, 528384, 2105344] exceeded the independent linear ceilings. Initial compile/setup mistakes (Parsed vs ModModule, test filter, missing root) were corrected before collecting this RED and are not counted as regression evidence.
2. Confirmed the cost fixture retains two real visible target classes, so a global empty-class fast path cannot satisfy it. Table dimensions are helper count and module/member import form; no runtime timing assertion is used.
3. Literal ceilings [1024, 2048, 4096, 8192] allow further reductions in visited entries, while the member-import control requires zero. The counter increments at the actual visibility-module visit boundary, not a reconstructed source formula. Thread-local storage isolates parallel tests.
4. The positive control independently expects both `p.errors.Child` and `zz.Child`, the canonical candidate `p.errors.Child`, and no hierarchy diagnostic. It passed before and after the lookup change.
5. The empty-target cancellation test isolates the private visibility stage, checks cancellation on visit 16 of 128, and failed before implementation. Existing public suites exercise import roots, ambiguous identities, relative names, load positions, scopes and the 65,536-entry ceiling. Public plan replay independently compares complete candidate arrays, diagnostics and truncation.

## Validation and evidence

All Cargo commands and commit hooks used this environment; commands were
sequential:

```sh
export CARGO_TARGET_DIR=/tmp/hoimin-issues-992-998/target
export CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export RUST_TEST_THREADS=2 PREK_CONCURRENCY=1 CARGO_NET_OFFLINE=true
```

- RED: `cargo test -p hoimin-python --lib visibility_tests -- --nocapture`: one semantic control passed, cost case failed with visits [33792, 133120, 528384, 2105344], against ceilings [1024, 2048, 4096, 8192]. The initial test used equality; final test accepts any smaller count. Log: `/tmp/hoimin-issues-992-998/task-5/red.log`.
- Cancellation RED: `cargo test -p hoimin-python --lib visibility_cancels -- --nocapture`: expected `AnalysisError::Cancelled`, but the old empty-target loop returned success. Log: `task-5/cancel-red.log` under the same evidence root.
- GREEN: `cargo test -p hoimin-python --lib visibility_tests -- --nocapture`: all 3 passed. Module-import visits [1024, 2048, 4096, 8192]; member-import visits [0, 0, 0, 0]. Log: `task-5/green.log`.
- `cargo test -p hoimin-python --lib`: 366 passed, 3 existing ignored, including hierarchy parsing, alias extraction oracle, relative imports and cancellation. Log: `task-5/python-lib.log`.
- `cargo test -p hoimin-cli --test exception_hierarchy --test exception_hierarchy_scopes --test lean_exception_hierarchy_oracle -- --nocapture`: 22 + 5 + 2 tests passed; strict public-plan oracle checked 159 non-error cases plus resource-error fixtures. Log: `task-5/hierarchy-cli.log`.
- Registered gate command: `cargo test -p hoimin-python --lib rust::exception_hierarchy::visibility_tests::external_import_visibility_work_tracks_imports_not_project_mappings -- --exact --nocapture`: 1 passed after the final ceiling assertion change. Log: `task-5/gate.log`.
- `cargo build -p hoimin-cli --bin hoimin` succeeded. The task-owned `/tmp/hoimin-issues-992-998/task-5/probe.py` replayed the original audit's module/member helper fixtures at N=32/64/128/256 before and after the change. Eight public `plan` results were identical for complete ordered candidate arrays, diagnostics and truncation. Each has 1 candidate, no diagnostics, `truncated=false`; canonical candidate JSON SHA-256 remains `2511d0ca186bb4df8e42cc7cb3ee295576ffe4b7b7f2407b46b19b59f2a9bf11`, matching the original audit. Aggregate row SHA-256 before/after: `73bf728b2061dd6d48e9368c578c30a4c0e666e68d1bff26035a39d6f8599bc3`.
- Probe command per fixture: `/tmp/hoimin-issues-992-998/target/debug/hoimin plan --root <fixture> --file target.py --operators exception_hierarchy --analyzer-timeout 3s --allow-best-effort-memory -- true`. Original fixtures and audit evidence at `/tmp/hoimin-audit-20261019-8fqnvm9k/analyzer-probes/` were read only; the adapted probe and before/after outputs belong to Task 5.
- `python3 tools/performance_shapes.py check`: validated 29 shapes and the gate registry. `cargo fmt --all` and `git diff --check` succeeded.

The operation gate measures visited module entries in visibility construction;
ordered-map comparisons, allocation, parsing and other hierarchy phases are
outside its count. This task makes no wall-time or whole-analysis bound claim.
Repository commit hooks run with the same resource settings; their result is
recorded in the task report.
