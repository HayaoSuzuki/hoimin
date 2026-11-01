# Combined final-review fix report

Completed both final-review findings in commit `fcb11b946e2730f75c54d4408b5132ff6b57e41a` (`fix: reject mixed disk reports and classify pre-lease staging holds`), based on `4254936ca33240e2f69f6d612fbb408d92090c8a`. Scope: four product/test files. The controller owns the scoped re-review and final archive/publication work.

## Changes and correction to the historical Task 4 statement

`DiskRunSummary` now denies unknown fields at the shared Serde boundary. JSON and JSONL progress readers reject `stale_roots_reclaimed` beside current `janitor`, including explicit null. `required_janitor` still rejects omission and accepts explicit null. This enforces the existing published schema-9 contract without changing the schema version or 0.14.0 package floor.

The janitor retains the existing typed directory-age result. It records a Young hold only for a canonical staging candidate whose empty-directory inspection succeeded, whose existing lease metadata lookup returned NotFound, and whose age check returned Young. It uses the same capability and observations; it does not repeat filesystem probes. Non-Young age errors keep their diagnostic and original lease-error fallback. The stale-empty deletion path and its eligibility condition are equivalent to the previous implementation.

**R14 correction appendix:** the Task 4 report's statement that missing leases cannot be ordinary young holds was too broad. An empty staging directory from a pre-lease crash is an established lifecycle state, as the prior stale-empty reclamation test demonstrates. Its positively observed young form is preserved/young. Default presentation suppresses its path detail; verbose presentation retains it. Missing leases on nonempty staging or active roots, malformed lease entries, and failed inspection/age evidence remain unresolved. Historical Task 4 reports and their past receipts remain unchanged. Current general user guidance already describes ordinary holds and unresolved failures accurately, so this fix changes no published documentation or schema files.

## Verification

Fresh focused tests total **527 passed / 3 ignored**: core janitor/report policy/status 155; owned janitor 82; public progress 144; report handler 146. The three ignored tests are existing subprocess entrypoints exercised by their parent tests. The required commit hooks passed with no bypass. The native public staging probe also passed its four scenarios.

The table names matching `.json` command receipts, `.log` outputs and `.diff` tested snapshots under `/tmp/hoimin-priority-six-final-fix/`. Each receipt records exact argv, pre-command source HEAD, tested diff SHA-256, log SHA-256, elapsed time, exit status and bounded environment. The tests ran from base HEAD plus the recorded working-tree snapshots; I do not present them as postcommit reruns. The final test/hook snapshot hash `6ee975a1ef98d07defdbb1d4af23ef1b7be1d17556d04f0153c7f0e4f654c250` matches committed `4254936ca33240e2f69f6d612fbb408d92090c8a..fcb11b946e2730f75c54d4408b5132ff6b57e41a`. Earlier core/RED snapshots are retained in their own receipts.

| Receipt/log stem | Command | Exit | Result | Log SHA-256 |
| --- | --- | --- | --- | --- |
| `red-core` | `cargo test -p hoimin-core --test janitor_report` | 101 | 3 passed / 1 expected RED | `affffc11f49f4512dd63dbd822e7f3f1df8b58f8ed3d0e8b14f95bbdf51db261` |
| `red-public` | `cargo test -p hoimin-cli --test progress disk_` | 101 | 5 passed / 2 expected RED | `bcb2e2802ed8c424e2880cf2beb6c3827b2cac61c8628af4e8698a65be9b1dc9` |
| `red-owned` | `cargo test -p hoimin-cli --lib pre_lease` | 101 | 1 passed / 2 expected RED | `f5a193eec5dadf7c6437f45f38cac877c119fe99acdf0b594cc47fe5d6f57d50` |
| `green-core` | `cargo test -p hoimin-core --test janitor_report --test report_status --test report_policy` | 0 | 155 passed | `555829ed0809c5031d0f2a3a69f64d3e3c28fc704e4d3bd15a022e69abb73fb8` |
| `green-owned` | `cargo test -p hoimin-cli --lib workspace::owned::` | 101 | 80 passed / 1 fixture failure / 3 ignored | `19ce18507f24e94195dbf3a2c93aff96bde9c4d28ba3e51a06435246abcc6251` |
| `green-owned-corrected` | `cargo test -p hoimin-cli --lib workspace::owned::` | 0 | 82 passed / 3 ignored | `55d0c98bb282747924075b30ae065fa41ab867c04848a9bd24385b4716a81b81` |
| `green-public-report` | `cargo test -p hoimin-cli --test progress --test report_handler` | 0 | 144 progress + 146 report-handler passed | `6e84e33c1ac4d96ce1ada1580bbbb387df81250133404f2e08819f85ebd1aa39` |
| `public-staging` | `python3 /tmp/hoimin-priority-six-final-fix/staging_probe.py` | 0 | 4 CLI scenarios passed; each exit 1 | `7b01e0592b2600fd8119a0be735635e7a6ae44f1c845ed49db2325540e56cdd2` |
| `commit-hooks` | `git commit -m fix: reject mixed disk reports and classify pre-lease staging holds` | 0 | 4 mandatory hooks passed; commit created | `1f39002dde7315aba794919b111b38a2314a6b7235e7cb36377898c8e662d6d4` |

The initial `green-owned` failure was `empty_staging_with_unreadable_age_is_unresolved`: chmod 0600 prevented candidate acquisition, so its asserted AgeUnverifiable detail was incorrect. The report correctly contained CandidateInspectionFailed. I renamed/corrected that control and added `failed_staging_age_inspection_retains_typed_failure`, which removes search permission only after acquiring the capability. The final owned run passes both controls. This was test-fixture correction, not an additional production fix, and the failed log remains in the receipts.

`staging/receipt.json` records exact public argv, isolated TMPDIR, report/log hashes, observed directory mtimes and the binary SHA-256 `e9808acbf8114c114c2f4d43e62883491c039cc739caf9ea49386e432dde02f0`. The young default report has preserved/young=1, unresolved=0, no details and one suppressed ordinary detail; verbose has the same counts and one Young path detail. Both leave the empty directory and mtime unchanged. Setting its mtime 25 hours earlier reclaims it. All four runs exit 1 because `/usr/bin/true` survives the mutation. The checked executable was rebuilt by the CLI test command at the final product snapshot.

## Five requirements/design self-review passes

1. Wire ownership: DiskRunSummary is shared by JSON summary and JSONL run_finished. deny_unknown_fields matches the already-published object schema; retain required_janitor to distinguish null from absence. No schema/version changes.
2. Lifecycle: R14 applies only after canonical staging selection, successful directory open/identity, successful empty probe, positive age evidence and observed lease NotFound. Nonempty and active roots must still reach LeaseInvalid; stale empty staging retains its original deletion path.
3. Operation order/races: reuse the already-obtained lease metadata and one existing empty/mtime probe; preserve their order and all fallback observations. Retain typed Result from age inspection, including failure details; never infer age from strings or probe again.
4. Presentation/accounting: Young enters the existing ledger hold path, so counts and verbose/default projection share ownership. Test independent projections plus unchanged filesystem and zero deletions. No execution exit-policy authority is added.
5. Validation/resources: meaningful RED for core mixed field, both actual public readers and both young projections; valid/null/missing, nonempty, invalid, unexpected lease controls and existing stale test. Bounded Cargo (2 jobs/2 threads), sole owner, required hooks and native macOS-only claims. Historical reports remain unchanged; R14 correction recorded in final report.

## Five implementation self-review passes

1. Shared wire boundary: one deny_unknown_fields attribute applies to DiskRunSummary in both formats. required_janitor is unchanged; all current fields/null representations remain accepted. Existing schema already says additionalProperties false.
2. Deletion equivalence: Some(Ok(())) replaces the same old successful boolean condition; stale empty removal body is untouched. The new branch only emits hold and continues, after the original reclaim path. It cannot widen deletion eligibility.
3. Typed provenance and order: observed lease_metadata is reused; staging && existing empty probe remains short-circuiting; age is called once. Result is retained without parsing cause text. Existing non-Young age diagnostics are preserved, followed by the original lease error path.
4. Failure visibility: unknown/missing lease cases only use Young when the lease lookup was NotFound, staging empty probe succeeded and age specifically returned Young. Active/nonempty/malformed roots and permission failures remain unresolved. No extra mutable summary fields or new public API.
5. Diff/test maintainability: only two product boundaries and three test files changed; generated cases isolate one format/defect or one projection/state. Caught a permission fixture that failed at acquisition instead of mtime; split it into acquisition and held-capability mtime checks. No new production hooks or filesystem race probes were introduced.

## Five validation self-review passes

1. Regression sensitivity: observed the core retired-member decode test fail by accepting the object, both real JSON/JSONL processes return 0 instead of 2, and both young-staging projections report unresolved/lease_invalid. These five intended failures occurred before product edits; controls remained green.
2. Wire cases and isolation: final progress tests invoke the executable separately for JSON/JSONL current null, missing janitor and retired field. Golden event ordering includes mutant_started. Core/report suites retain status consistency and null/absence tests; report-handler schema tests pass. No new schema bump or fixture migration.
3. Cleanup evidence and projections: 82 owned tests pass with 3 existing helper entrypoints ignored. New cases cover both young projections, young/stale nonempty roots, empty active root, directory lease, candidate acquisition failure, empty inspection failure and typed age failure. Existing stale-empty reclamation and deletion-resume controls pass. Public CLI probe preserves the young directory mtime/contents in both modes, removes it after aging 25 hours, and retains exit 1 for the surviving mutant throughout.
4. Test assumptions and limits: the first owned run exposed my permission-fixture assumption: 0600 failed candidate acquisition before age lookup. I retained that diagnostic in green-owned.log, corrected its expected boundary and added a separate already-opened capability test for age failure. Unix permission tests ran as the native non-root macOS user. Native Linux/Windows behavior remains for CI; no new formal-proof or broad workspace-run claim.
5. Resource and completion evidence: Cargo was sequential with two build jobs and two test threads. Required commit hooks passed (workspace/vendor fmt, workspace all-target/all-feature clippy, vendor clippy). PREK_NO_CONCURRENCY=1 serialized hooks. Disk check reported 75 GiB available. Final diff matches the recorded final precommit snapshot; clean tracked worktree and diff --check verified after commit. No target cleanup, push or PR action.

## Resources, remaining work and limits

I remained sole product implementer and Cargo owner. Cargo/test commands and git commit used `CARGO_BUILD_JOBS=2`, `CARGO_TARGET_DIR=/tmp/hoimin-priority-six-target`, `RUST_TEST_THREADS=2`, `UV_PROJECT_ENVIRONMENT=/tmp/hoimin-priority-six-venv`, `UV_CACHE_DIR=/tmp/hoimin-priority-six-uv-cache` and the venv PATH prefix. Commit also used `PREK_NO_CONCURRENCY=1`; all four configured hooks ran. Git staging/commit used escalation. I did not push, open a PR, clean the shared target or change other worktrees.

No known product concern remains for these two findings. The controller still needs the scoped final-fix review and archive, followed by the already-planned publication/CI work. These receipts establish native macOS behavior only. Unix permission fixtures assume a non-root user; they do not establish native Windows error behavior. This wave did not repeat the full workspace suite or execute proof tools, per the focused-validation contract. `final-state.json` records the final clean tracked-worktree observation and diff check; the plan-owned report is intentionally outside the product commit for Task 7's archive step.
