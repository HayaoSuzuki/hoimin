# Typed shell completions

For maintainers reviewing [#828](https://github.com/HayaoSuzuki/hoimin/issues/828), this record explains the completion types and checks that preserve scheduler behavior. The baseline is main at `2ca030f`. This is an internal refactor with no new event bus, run-loop decomposition, public contract change, or performance claim. The package version floor stays at 0.4.0.

## Notification and ownership paths

The completion channel carries three notification kinds. Their payloads have different ownership obligations:

| Kind | Producers | Payload and normal acceptance | Shutdown drain |
| --- | --- | --- | --- |
| Serial | Effect preparation failures, budget observations, serial effects, final output acknowledgement, priority/stop events | A `RunEvent`; no spawned task or in-flight decrement | Not sent through the spawned-task channel in production |
| Process | `spawn_process` | Event, required worker ID, and whether it counts as a mutant; post-drain disk sample, task join, process accounting | Finish mutant metrics with `accepted = false`; baseline has no mutant metric |
| Blocking | `spawn_blocking_effect` | Success owns a boxed `BlockingEffectCompletion`; failure owns `EffectFailed`. Accept successful ownership before the state transition; join I/O and decrement its count | Return successful ownership even on expiry; a failed task has no ownership payload to accept |

`ShellCompletion` expresses these kinds as enum variants, including a `Result` payload for blocking success/failure. After ownership acceptance consumes the payload, a smaller `CompletionSource` retains task origin and process accounting data until transition and join finish. This descriptor replaces independent process/I/O flags and optional process metadata; it does not schedule work. The process mutant flag still describes the two accounting roles within a process notification, including events where the role cannot be inferred from a success event.

Blocking success no longer requires a cloned shadow event alongside its owned completion. Test-only output-acknowledgement observation still reads the event before ownership acceptance, preserving its existing observation point. Large blocking payloads stay boxed.

## Design self-review

1. Enumerated every production constructor and all normal, post-drain-stop, and shutdown-drain consumers. Serial events do not represent spawned tasks.
2. Compared a struct with a tagged metadata field against a completion enum. Chose the enum with a blocking `Result` so success cannot omit ownership and failure cannot carry unrelated ownership.
3. Traced event ordering: serial, priority, ready channel completion, and the biased stop/channel select remain in the same order. Disk sampling still applies only to process completions before classification.
4. Traced ownership: a successful blocking payload must be accepted during normal processing and drain. A failed blocking join preserves its effect identity without inventing a payload.
5. Checked scope and size: preserve boxing, existing joins, deadlines, metrics, and cleanup rules; no scheduler redesign or new public API. No change to the existing formal state machine is intended.

## Implementation plan self-review

1. Run existing CLI library tests before edits to establish a behavioral baseline; avoid a test that only mirrors enum variants.
2. Migrate serial constructors, process sender, and blocking sender together. Preserve event creation, worker identity, and mutant/baseline metadata.
3. Migrate normal acceptance and post-drain disk-stop replacement; retain the source descriptor only after consuming ownership.
4. Migrate drain acceptance and existing fixtures/assertions. Use observable ownership, metrics, failure, and cancellation tests for regression protection.
5. Run CLI library and shutdown/process/workspace/metrics integrations, formal-model correspondence tests, strict all-feature Clippy, formatting, and documentation checks. Use two build jobs/test threads and a dedicated target, then clean it. Finish with an independent read-only review.

## Implementation self-review

1. Compared all 14 serial constructor migrations and both spawned-task senders with main. Their event expressions, worker identity, and mutant/baseline roles are unchanged.
2. Traced post-drain disk failure: only process notifications trigger sampling; secondary process errors, task join, metrics, decrement, and replacement by a serial disk-stop event retain their order.
3. Traced normal acceptance: successful blocking ownership is accepted before the state transition; the source descriptor preserves the old counter, metrics, and join decisions without independent flags.
4. Traced shutdown drain: successful blocking ownership is returned, mutant metrics finish as unaccepted, and baseline/failed blocking results do not create mutant metrics or ownership. Expiry, task aborts, and final in-flight reset are unchanged.
5. Reviewed the whole diff for scope and memory: no public API, state machine, schema, dependency, deadline, or cleanup-safety changes. Large ownership payloads remain boxed. The workspace event accessor is test-only after removing the shadow-event clone; test acknowledgement observation remains before acceptance.

The added blocking-failure test runs an actual spawned operation that panics, then checks slot release, completed joins, and absence of ownership or metrics during drain. The existing process-metrics drain test also checks a baseline notification. These test observable accounting and ownership rather than reproducing the enum's variant list. The new test has branch-free Arrange/Act/Assert structure; it does not require randomized inputs.

## Testing self-review

1. Established the baseline with `cargo test -p hoimin-cli --lib`: 855 passed, 16 ignored. The first mechanical migration passed the same selection before adding the regression coverage.
2. Ran the final default-feature library selection: 856 passed, 16 ignored. Existing tests cover post-drain stop ordering and secondary failures, ownership return on expiry, responsive/overlapping blocking I/O, output acknowledgement, cancellation, and safe cleanup. The new test also passed separately after its assertion diagnostics were improved for strict lint.
3. Ran process, workspace, and session handlers and actual CLI integrations, checking counters, joins, isolation, output, metrics, and termination. Counts are listed in the table. Tests ran with two threads and build jobs.
4. Ran existing formal-model suites and shell tests with `contracts` enabled. The model's transitions and guarantees did not change, so no new model or proof was added. The shutdown suite's strict CLI correspondence cases are Unix-only and were not executed on Windows; its five local tests validate corpus/projection/error handling. The disk and result-lifecycle suites include their Rust correspondence checks. Lean proofs were not rebuilt.
5. Ran formatting, whitespace, and all-target/all-feature Clippy with warnings denied, followed by the strict documentation build. The first Clippy run flagged two empty-collection assertions in the new test; both were changed to `assert_eq!` without lint exclusions. Final Clippy and the focused regression passed. No Python code changed, so the Python suite was not rerun.

The default-feature regression command was:

```console
cargo test -p hoimin-cli --lib --test run_e2e --test disk_shutdown --test telemetry --test session_handler --test process_handler --test workspace_handler --test lean_shutdown_oracle --test lean_result_lifecycle_oracle --test lean_disk_shutdown_oracle
```

Results on Windows are:

| Selection | Passed | Ignored |
| --- | ---: | ---: |
| CLI library | 856 | 16 |
| Disk shutdown | 4 | 0 |
| Disk shutdown oracle | 2 | 0 |
| Result lifecycle oracle | 5 | 0 |
| Shutdown oracle (Windows subset) | 5 | 0 |
| Process handler | 26 | 0 |
| Actual CLI | 68 | 1 |
| Session handler | 33 | 1 |
| Telemetry | 5 | 0 |
| Workspace handler | 30 | 0 |
| Shell with contracts (separate run) | 106 | 2 |

The separate contracts command was `cargo test -p hoimin-cli --lib --features contracts shell::`. The final assertion check was `cargo test -p hoimin-cli --lib shutdown_drain_failed_blocking_task_releases_its_slot_without_ownership`. Strict lint used `cargo clippy --workspace --all-targets --all-features -- -D warnings`; formatting used `cargo fmt --all -- --check`; whitespace used `git diff --check`. Ignored tests, Unix/macOS behavior, and measured enum size or throughput are outside these execution claims.

## Independent review and compatibility

The fresh read-only reviewer compared the full diff with main `2ca030fee81cdbbfc0a7a17cb019e3e574a0bd23`, tracing producers, ordering, normal ownership/accounting, shutdown ownership/accounting, scope, and boxing. No Critical, Important, or Minor issue or blocking test gap was found. The reviewer also confirmed the final assertion changes, without independently reproducing builds or tests.

The scoped software-design-philosophy assessment passed all eight diagnostic rows (10/10), with no failed row: required payloads replace unrelated flags/options, and the source descriptor has a documented lifecycle purpose. This is an assessment of this completion boundary, not the whole scheduler or repository. The maintenance record and changed guide pass all ten technical-documentation diagnostic rows (10/10); no failed row or blocking factual issue remains.

Consulted `docs/compatibility.md`, `docs/okf-workflow.md`, and `docs/knowledge/design/runtime-lifecycle.md`. State transitions, deadlines, shutdown and cleanup guarantees, public CLI, defaults, saved formats, dependencies, and distribution requirements are unchanged. No command, reader, plan, session, or version-floor migration is needed. Existing knowledge concepts and formal models require no revision for this internal representation change; `docs/development.md` links the ownership explanation and current evidence here.

## Resource and documentation checks

Builds used the task-owned `.superpowers/issue-828/target`, two build jobs/test threads, and disabled incremental compilation and debug information. A check during verification reported about 3.7 GiB of free physical memory. After Rust checks completed, `cargo clean` removed 3,671 files (1.3 GiB) from the checked absolute target path. The C: drive then had 182,216,232,960 bytes free (about 170 GiB). The ordinary workspace target was not cleaned.

The strict site build validates the maintainer guide and its link to this repository-only record. This record remains outside the site publication allowlist. The final check uses `python -m tools.docs_site --output site` with the existing Python 3.14 environment.

## CI follow-up: abandoned staging fixture

[CI run 38231895577](https://github.com/HayaoSuzuki/hoimin/actions/runs/38231895577)
failed one Linux CLI library test: 842 passed, one failed and 18 ignored.
`stale_staging_root_is_reclaimed_only_with_expected_marker_contents` reported
zero reclaimed roots and one preserved root. The CI result job propagated that
failure; it was not a second independent defect.

The fixture dropped a locked lease and assumed that the lock was immediately
released. A concurrently forked child can retain the shared open-file
description until exec. Two adjacent reclamation fixtures already account for
this. The corrected staging fixture follows their pattern: retain a duplicated
lease handle on Unix and explicitly unlock before dropping the owner. The
duplicate makes omission of the unlock deterministic on Unix, without relying
on process timing. On Windows, no duplicate is retained and the owner still
closes before rename. No production cleanup or locking rule changes.

The CI log alone does not identify every possible preservation path. A separate
Linux kernel probe using `fcntl.flock` and `os.dup` confirmed that closing the
owner while retaining a duplicate leaves the lease busy, whereas explicit
unlocking makes it acquirable. This verifies the OS mechanism; it is not a local
Linux Rust test run.

### Follow-up self-reviews

Design:

1. Separated the primary Rust failure from the dependent aggregate failure.
2. Compared the failing fixture with the two existing retained-handle fixtures.
3. Traced the janitor's busy-lock preservation path and its empty diagnostics.
4. Kept production locking, marker, age and identity checks unchanged.
5. Chose deterministic fixture preparation over sleeps, retries or serialization.

Implementation plan:

1. Retain the existing reclaim assertion and the single Arrange/Act/Assert flow.
2. Duplicate the shared description only on Unix, matching existing fixtures.
3. Establish the unlocked premise explicitly before owner drop and rename.
4. Verify the kernel mechanism, owned-root tests, full CLI library and strict lint.
5. Obtain independent review, update this record and the existing PR, then clean
   the owned build target after verification.

Implementation:

1. Checked that the duplicated handle remains alive through reclamation.
2. Checked that unlock acts on the same shared description retained by the copy.
3. Checked that Windows still drops all owner marker handles before rename.
4. Kept the test free of runtime branches, timing assumptions and new helpers.
5. Compared the 15-line test-only diff with the neighboring established pattern;
   no public behavior, version, dependency or distribution change is introduced.

Testing:

1. Confirmed the original Linux CI failure and exact preservation report before
   changing the fixture. The deterministic retained-handle variant is Unix-only.
2. Reproduced the locked-after-close and unlocked-after-explicit-release states
   with Linux `fcntl.flock`/`os.dup`; no sleep or probabilistic fork was needed.
3. Ran Windows `cargo test --locked -p hoimin-cli --lib workspace::owned::tests::`:
   65 passed, three ignored. Live-root preservation and reclaim behavior remain
   covered by the existing owned-root selection.
4. Ran Windows `cargo test --locked -p hoimin-cli --lib`: 856 passed, 16 ignored;
   then `cargo clippy --locked -p hoimin-cli --lib --tests --all-features -- -D warnings`
   passed. Builds and tests used two jobs/threads and the existing owned target.
5. Formatting, whitespace and `python -m tools.docs_site --output site` passed.
   An initial documentation invocation with a scratch output path was rejected
   by the existing output-directory guard; the supported command succeeded.
   No Linux Rust execution is claimed by these local checks; hosted CI must
   confirm the retained-descriptor regression on Unix.

Independent source review found no blocking issue in the fixture delta and
confirmed its retained-handle lifetime, Windows ordering, branch-free AAA and
unchanged production guards. The scoped philosophy assessment passed 10/10;
this does not assess the entire owned-workspace module.

The final independent review also checked the appended record and test/lint
log tails without finding a blocking issue. After verification, `cargo clean`
on the checked task-owned target removed 3,341 files (856.0 MiB), leaving
181,908,611,072 bytes free on C:.
