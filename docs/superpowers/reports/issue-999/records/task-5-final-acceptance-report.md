# Task 5 final hosted evidence and documentation update

Base: `312be2c4759be00a26e2f04519258f1172057a67`. This follow-up changes only
documentation, OKF provenance and archived evidence. Production, fixture code,
proof sources, runner, tool pins, dependency locks and resource limits are unchanged.

## Source-specific hosted acceptance

Native Linux run `39197995202` used proof-source commit
`5033e1bb74c4b01c66dc86c3c9f70fc1b8bf11ee`. Attempt 1 completed the full workflow
successfully. Attempt 2 reran the proof job after the controller deleted only
its owned branch tool-image cache; it also concluded success with prior passing
jobs retained. The optional Linux cgroup-v2 hard job was skipped in both attempts,
not executed. The original metadata records every job disposition.

Warm job `117884864564`: `cache_hit=true`, setup-inclusive 247 seconds,
runner 156.09 seconds, generation/proof peak process-or-child RSS
1,514,396 / 273,676 KiB. Cold job `117888001628`: explicit `Cache not found`,
setup-inclusive 693 seconds, runner 105.023 seconds, generation/proof peak RSS
1,516,044 / 273,544 KiB and tool-build peak RSS 1,086,792 KiB. Cold completion
left 81,462,580 KiB disk and 14,552,420 KiB memory available. Each run generated
three targets independently, each 25 fresh Coma files / 59 proved goals /
25 strategies. Each produced 29,542 bytes of output and no source-span warnings.
The elapsed times fit the unchanged 30-minute setup-inclusive cap. Different
runner durations in separate jobs establish no warm/cold speed causation.

Historical Task 4 proof results at `41c888f5` and local ARM development results
remain separately sourced. Prior non-Linux run `39198000067` at `5033e1bb` failed
two native Windows readiness fixtures; it is preserved as failed evidence.

Native follow-up `39199825814` at `70ea57b2` failed the same two readiness cases:
761 passed, two failed, 23 ignored in 57.69 seconds. Thus the two-thread
scheduling mitigation was insufficient. The first scoped fix review found both
documentation Minors addressed and no new breakage; its then-pending native
acceptance finding is retained verbatim.

Diagnostic run `39201076428` at `dd2674dd` completed with eight successful jobs
and a failing Windows Rust job. Both six-second Job Object cases passed in that
job and its resource-only comparison. No observed diagnostic result establishes
the original intermittent cause. The failing lifecycle adapter already drained
stdout/stderr concurrently, but its post-premise exit wait omitted the documented
two-second shutdown grace and 250-ms binary telemetry flush following the
12-second run limit. `312be2c4` derives that wait from all three existing budgets,
leaving the separate eight-second premise wait and six-second Job Object bounds
unchanged. The round 2 report retains actual RED/GREEN and focused Clippy logs.

Native follow-up `39202749924` at `312be2c4` completed successfully with all nine
jobs successful. Metadata: `non-linux-final-312be-metadata.json`. Windows Rust
job `117899018511` reports 173 suite summaries / 2,886 passed / zero failed /
31 ignored, including both six-second Job Object tests, the three timeout-boundary
tests and real lifecycle correspondence. Its job duration was 20m18s within the
30-minute cap. macOS Rust job `117899018190` reports 172 summaries / 2,931 passed /
zero failed / 33 ignored. These are whole-job aggregates, including additional
commands beyond the workspace test. Windows/macOS wheels passed 1,155/1,192 tests
with 51/14 skips; Windows resource scope passed 37 tests in three summaries.

The round 2 scoped review found no new breakage and judged the composed guard
and local checks sound. Its only remaining gate at review time was native
acceptance, now satisfied. Its evidence limit is preserved: RED has an exact
command/result summary, not a raw terminal transcript. Final whole-branch review
remains pending. Bounded test-only diagnostics remain useful failure context;
this does not establish the original intermittent readiness cause. Their timing
perturbation and maintenance costs remain explicit for whole-branch review.

Proof inputs are unchanged from `5033e1bb` through `312be2c4`: production math,
root Cargo manifests/lock, formal proof sources/strategies/configuration, runner
and environment tools. Within those paths only `formal/creusot/README.md` changed.
Native warm/cold results retain their actual source SHA; the final PR runs its
ordinary fresh proof CI without an unnecessary additional cold tool build.

## Archive and provenance

`docs/superpowers/reports/issue-999/evidence/task-5-final-ci.tar.gz` preserves the
native CI directory under `task-5-final-ci/`, including actual generated proof
artifacts, setup/resource records, full logs and metadata. Earlier archives and
failed attempts remain unchanged. The latest progress ledger preserves all seven
`Ruling:` decisions and their costs; both scoped fix reviews are copied verbatim
apart from terminal-newline normalization. The SDD workspace is not deleted.

The guide, adoption report, spec/plan status, review record and relevant OKF
concept are updated. Only the four changed source references receive new hashes
and base revision `312be2c4` with `working_tree: modified`; unrelated historical
metadata stays intact. All licensing and package conclusions retain their
original audited source and scope. This evidence-only update claims no additional
legal conclusion or fresh package execution.

## Focused validation commands

Run from the issue worktree with the task-owned Python environment. No broad
suite is repeated for this evidence/documentation update.

```sh
python3 /tmp/hoimin-issue999-finalize-evidence.py
/tmp/hoimin-issue999-task5-venv/bin/python /tmp/hoimin-issue999-validate-okf-final.py
/tmp/hoimin-issue999-task5-venv/bin/python -m pytest tests/test_docs_site.py -q
python3 /tmp/hoimin-issue999-validate-final-archive.py
cargo fmt --all -- --check
git diff --check
```

Archive SHA-256 validation compares every regular archived file against
`SHA256SUMS`; the copied progress ledger is checked against the source ledger's
complete `Ruling:` lines. Final run metadata and proof summaries are checked
against the exact source commits and measurements above.

## Five final implementation and evidence self-reviews

1. **Scope:** inspected changed files; no runtime, fixture, workflow, proof or
   dependency change accompanies this final evidence update. Earlier code
   checkpoints retain their separate review and native validation evidence.
2. **Provenance:** separate `5033e1bb` proof/full native CI, `70ea57b2` failed scheduling follow-up, `dd2674dd` diagnostics,
   `312be2c4` accepted native follow-up, historical `41c888f5`, and local ARM/package
   sources. Prior failed and skipped runs remain classified accurately.
3. **Measurement:** retained setup-inclusive versus runner duration, process RSS
   versus aggregate memory, owned-cache deletion evidence and exact target counts.
   No causal speed claim or relaxed cap is inferred.
4. **Review and decisions:** preserve scoped review's original pending disposition,
   all seven Rulings and their costs, failed logs and earlier snapshots. Final
   whole-branch review remains a controller gate.
5. **Documentation validation:** safe-parse OKF, verify only relevant fingerprints,
   compare manifest/archive contents and source records, check docs tests and
   whitespace on final contents. Existing package/audit evidence is
   reused at its actual source revision, not described as rerun.

## Final focused results

- Documentation-site tests: 16 passed in 0.27 seconds.
- OKF: 46 concepts safe-parsed, four reserved files checked, 23 relevant current
  source fingerprints verified; no missing changed-page file links. Historical
  hashes remain unchanged. Remote URLs and Markdown fragment anchors were not
  fetched or validated by this focused script.
- Workspace formatting and `git diff --check`: passed.
- Archive verification: 40 archived files covered by SHA-256, 93 native evidence
  files byte-identical to the retained source collection, all seven controller
  Rulings preserved, both scoped review copies preserved, and final metadata
  confirming all nine native jobs successful at the exact `312be2c4` source.

`evidence/task-5-final-validation.tar.gz` preserves the OKF/archival preparation
and validation scripts, actual OKF output, docs-test log and format-result record.
The archive verifier is run after the final manifest is refreshed; its counts
above are recorded in this report rather than inserted recursively into the
archive whose hash it verifies. The earlier Task 5 and fix-round reports retain
their original source-specific checks. No broad test/build was repeated.

Task 5's implementation and native acceptance are complete. The controller's
final whole-branch review and PR publication are still outstanding. No merge,
release, proof-image publication, or deletion of SDD scratch occurred.
