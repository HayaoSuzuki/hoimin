# Issue 616 implementation and review

Design/plan86b8116 preceded production changes, with three review passes each.

## Implementation self-review passes

1. Selection and allocation: inspected normal/include walker differences and recorded only explicitly whitelisted directories in the include pass. File ancestors are included for reachability. Replaced a postpass that cloned every ancestor path with incremental set insertion that stops at an already-recorded parent; no temporary path vector proportional to file-count×depth remains. File-only entries()/logical-byte accounting stays compatible.
2. Restoration and ownership: reviewed reverse-depth deletion of stray directories, file/link replacements, and recreation via retained-root no-follow parent handles. The synthetic child used by ensure_directory is never created. Pristine snapshot and fresh workers get directories, and reset/original-integrity comparisons include directory inventories. Existing workspace lifecycle, retained-root, depth and heap tests were run. No claim of preserving directory permissions is introduced.
3. Integration and independent reviews: two independent reviewers found no production blockers. Clarified children-only glob behavior in README and assertions: fixtures/** may leave parent fixtures, whereas fixtures excludes the tree. Narrowed the Lean design claim to presence/exclusion/reset; ancestor closure stays a Rust public-test responsibility. The first full suite exposed an import-root test relying on implicit parent removal; changed its negative premise to exclude the root itself and added the complementary positive test.

## Test self-review passes

1. Public RED: two tests failed because selected empty directories were absent. GREEN checks zero-file/zero-byte plans, nested directories, ignored include targets, unrelated ignored directories, and reset after replacing an ancestor with a file. Public CLI baseline plus two mutants remove the fixture directory during each test execution and still succeed after reset; original source directory remains.
2. Failure/policy boundaries: add a symlink replacement with an outside sentinel (Unix), default/literal exclusions despite include**, exact parent exclusion, child-only exclusion and original-directory deletion detection. The second independent review requested a generated file inside a retained empty directory; added an explicit reset-removes-child/keeps-parent assertion.
3. Executable oracle and contracts:24 versioned strict cases use real public WorkspacePlan/WorkerWorkspace create/reset operations, unique tuple IDs, selected/excluded combinations, absent/file/directory starting entries and optional extra directory, checking two resets. Run targeted contracts builds to exercise directory-set postconditions, plus existing heap/workspace tests. Lean sensitivity detects lost empty directories, ignored exclusions and retained extras. The model does not represent glob parsing, ancestor closure, filesystem races or directory permissions.

## Lean correspondence

| Model input/output | Public implementation |
| --- | --- |
| selected Bool | create or omit source fixture/empty |
| excluded Bool | CopyOptions excludes fixture/** or empty list |
| entry absent/file/directory | mutate the created worker before reset |
| extra Bool | create worker extra directory |
| expected directory/extra_after | filesystem metadata after create and two resets |

Bounds:2×2×3×2=24 strict cases, no trace search. New proofs use10,000 heartbeats; each model/main/native/freshness/sensitivity command runs under20s/2GiB. General idempotence and exclusion theorems apply to the deliberately small state model. Resource telemetry is committed beside this report.

## Validation

Results are appended after final verification. Initial focused public2 tests and later public/oracle/symlink/CLI5 tests passed. Existing workspace/disk/heap/Lean workspace groups passed; the first full run's import-root compatibility assertion was diagnosed and corrected with a positive control, not hidden.

Full workspace after import-root controls: 2383 passed, 0 failed, 22 ignored across 106 groups. Both CI Clippy/fmt gates and Python CI registry40 passed. Contracts-enabled empty-directory/workspace tests passed; final targeted rerun includes the generated-child assertion and updated registry placement.
