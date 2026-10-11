# Issue #756: Linux Rust runner evaluation

Base: `00893465e0328204453e5008dc3884035b85c7bd`.
Branch: `ci/issue-756-nextest-evaluation`.
Status: comparison completed; evaluated configuration not adopted; final review pending.

## Bounded design

Evaluate cargo-nextest in the existing `rust` Linux job before replacing its
runner. Keep the workspace, default features, Rust 1.99.0 and Python 3.14 fixed.
Use a version-pinned upstream binary with a checked digest; record installation
time separately. The upstream release inspected on 2026-10-11 was 0.9.148;
installation and compatibility have not yet been verified.

First compare test identities, not just counts. Match package, target, test name
and ignored status from Cargo-built libtest binaries against nextest's JSON
list. Check Cargo metadata for custom harnesses and explicitly selected examples.
Keep `generate_cli_reference` example tests and the generated-reference check.
Run doctests separately with `cargo test --locked --workspace --doc`; they cannot
be counted as covered by a successful nextest run.

Measure build and warm execution separately in the same hosted job. Record the
initial target/cache state: `uv sync` already builds the package, so the next
Cargo build must not be described as a fully cold build. Reuse the same compiled
test artifacts for runtime comparisons. Start nextest at two concurrent tests,
with no retries. Compare the existing Cargo concurrency unchanged, recording
available CPUs and effective test-thread settings. Use a short, alternating
repeat on representative `run_e2e` and `process_handler` tests, then a full
workspace comparison if the probe remains healthy. Record wall time, peak
resource measurements and concurrent descendant process counts. GNU time's
maximum RSS is not aggregate process-tree RSS; label that distinction and the
sampling interval/limits explicitly.

Adoption requires matching test coverage, successful cleanup assertions and a
repeatable runtime improvement that remains useful after installation and any
extra doctest overhead. A speedup in one noisy sample is insufficient. Resource
regressions, hidden skips, unexplained failures or loss of diagnostic evidence
prevent adoption until resolved. If there is no improvement, commit the measured
decision and retain cargo test, without leaving repeated comparisons in CI.

Keep nightly shuffle, contracts, Linux best-effort/delegated-cgroup jobs and
non-Linux jobs on cargo test. Hosted Linux cannot establish delegated cgroup or
Windows Job Object behavior. Record capability-driven `SKIP:` output separately
from executed assertions; a nextest PASS can still contain such a skip. Exercise
existing interruption/descendant-reaping tests under both runners in the Linux
probe. Do not add new product inputs or a general benchmark service.

Before adoption, check intentional failure and bounded timeout behavior in a
small disposable test fixture, including nonzero status and retained output/JUnit.
Keep diagnostic uploads on `always()` and do not mask failure with retries or
`continue-on-error`. Preserve original exit status through measurement/logging.
Do not assume hard termination lets test code finish cleanup; verify the fixture
leaves no descendants and keep resource-sensitive unsupported cases on Cargo.

## Design self-reviews

1. Scope: checked the current triage against the original all-OS proposal.
   Restricted the first trial to the existing Linux Rust job; no new mandatory
   job, sharding or cross-platform migration is required.
2. Coverage: inspected both workspace manifests and the explicit example command.
   Test names alone can collide across binaries; compare qualified identities
   and ignored status, preserve example tests and run doctests separately.
3. State isolation: checked the existing nightly shuffle command and process
   tests. Per-test processes cannot substitute for same-process order testing;
   keep shuffle and distinguish capability skips from assertions.
4. Measurement: inspected the job's `uv sync` before `cargo test` and build-job
   limits. Corrected the cold-build assumption and separated installation,
   compilation, warm execution and doctest overhead in the comparison.
5. Resource/failure behavior: inspected cgroup probe skip paths and interruption
   tests. Concurrency two does not bound child-process counts; measure descendants
   and require cleanup/exit-status evidence before adopting any timeout policy.

## Execution sequence and plan self-reviews

1. Prepare temporary comparison changes in `.github/workflows/ci.yml` and any
   minimal repository-private helper needed for inventory/resource measurement.
   Review: keep paths/commands fixed and do not create a user-facing input format.
2. Verify pinned installation, inventory comparison and focused failure/timeout
   checks locally where supported; use hosted Linux for the actual decision.
   Review: Windows measurements are not evidence for Linux performance or cgroups.
3. Push only the work branch and open an attached draft PR for the existing Linux
   job's trial. Review: explicit push refspec prevents the branch's initial
   origin/main tracking from accidentally updating main.
4. Compare representative workloads, then full workspace coverage/results if
   viable. Review: alternate repeat order, preserve exit codes and record skips;
   stop the trial on resource/cleanup faults instead of collecting more samples.
5. Replace temporary trial changes with the measured adoption or non-adoption
   result; document reproduction and rollback in `docs/ci.md` and update its
   knowledge source hashes when changed. Review: run strict workflow checks and
   only relevant regressions, complete five implementation and five verification
   self-reviews, and obtain independent review before marking the PR ready.

CI/docs-only changes do not alter the distributed version floor. Preserve
`.idea/`, monitor free disk space and run `cargo clean` periodically.

## Temporary comparison implementation self-reviews

1. Installation is pinned to 0.9.148 and the GitHub release asset SHA-256;
   credentials and write permissions are not added to the Rust job.
2. Inventory checks include all compiled test binary paths and qualified test
   identities with ignored status. Unsupported harness output fails the probe.
3. Resource sampling follows only the measured command's descendants, includes
   its time/Cargo wrappers and records its 200 ms sampling interval. Fast exits
   and reparented children can escape samples; existing cleanup assertions remain
   the behavioral evidence rather than the sampler claiming to prove cleanup.
4. Cargo and nextest alternate on the same timeout-focused integration targets;
   workspace runs follow, with doctests measured separately. Other CI jobs and
   explicit example/reference commands remain unchanged.
5. Commands use argv without a shell, preserve failed exit status, stop further
   trials on failure and retain logs/time files with `always()` upload. A 900 s
   command limit terminates the probe process group; this is an evaluation abort,
   not an adopted test timeout policy.

## Temporary comparison verification

The initial absent helper caused collection failure; with stubs in place the
intended regressions failed (four failures, one pass), then all five passed after
implementation. They reject a missing test, a changed target kind and a changed
ignore flag, and check recursive process accounting excluding siblings.
Repository Ruff format/check and ty passed. Strict actionlint/ShellCheck/zizmor
passed (the existing three Python-shell warnings remain). Cargo clean removed
zero files; disk free space was 175.79 GiB before the trial.

Ruling: use this review record as the execution ledger, since the approved
bounded design already contains the sequence. Do not create a second speculative
implementation-plan document or a persistent benchmark interface. The temporary
comparison/helper/tests were removed after the measured decision.

## Final implementation self-reviews

1. Compared restored workflow and production files byte-for-byte through Git
   against the base: no product, workflow, test or package-version change remains.
2. Preserved measured commands, GNU time output, sampling metadata and log hashes
   in Markdown evidence, so this documentation-only result uses existing CI
   selection without broadening its path classification.
3. Kept the successful comparison job separate from the failed overall trial.
   The two existing workflow assertions remain unchanged and are rerun after
   restoration; they are not waived to make the experiment green.
4. Reported the memory benefit together with separate doctest overhead and the
   unchanged shared-build peak; the decision does not discard favorable data.
5. Recorded non-adoption only for the evaluated Linux two-thread setting.
   The archived probe is reproducible by commit, with unsupported platforms,
   concurrency settings and adoption-only failure/timeout checks explicit.

## Verification self-reviews

1. Verified actual independent inventory equality: 2,785 qualified identities,
   145 binaries and 24 ignores. Nextest JUnit has 2,761 cases and no failures;
   excluded two nested bootstrap summaries from naive Cargo pass-count sums.
2. Inspected both logs for eight capability skips; no delegated-cgroup success
   is inferred from PASS. The timeout probe similarly has one capability skip.
3. Verified alternating probe repeats (12 selected cases each) and successful
   full workspace/doctest runs. Existing interruption/reaping checks executed on
   supported Linux paths; examples/reference checks also succeeded afterward.
4. Reviewed resource arithmetic and limits: 200 ms samples include wrappers and
   may miss fast/reparented processes; RSS sums duplicate shared pages. A single
   full-workspace pair does not establish a general speed difference.
5. Checked that retaining Cargo follows the agreed improvement gate and avoids
   permanent benchmark/diagnostic machinery. Final strict lint, links/source
   hashes, unchanged workflow contracts and independent review are recorded below
   as their results arrive, without claiming unrun checks succeeded.

## Hosted result

[Rust comparison job](https://github.com/HayaoSuzuki/hoimin/actions/runs/38113472996/job/114393883068)
succeeded. Cargo workspace took 146.12 s, nextest 151.10 s, installation 1 s and
separate doctests 0.83 s. Timeout probe repeats were Cargo 15.97/14.32 s versus
nextest 20.32/20.35 s. No runtime improvement justified adoption; the report
records reduced execution RSS and its build/doctest limits as well.
See the [result](../performance/nextest/2026-10-11-linux.md) and
[preserved evidence](../performance/nextest/2026-10-11-linux-evidence.md).

Ruling: do not add intentional failure/forced-timeout diagnostic infrastructure
after the probe failed the adoption gate. Those checks remain prerequisites for
any future adoption, as agreed in the design. This costs no current Cargo
coverage; it leaves nextest timeout behavior unverified, explicitly documented.

## Sources inspected

- [Issue #756](https://github.com/HayaoSuzuki/hoimin/issues/756), current triage.
- [Existing CI](../../.github/workflows/ci.yml),
  [CI guide](../ci.md), [CI contract](../knowledge/ci-validation.md).
- [Nextest execution model](https://nexte.st/docs/design/how-it-works/).
- [Test and binary lists](https://nexte.st/docs/machine-readable/list/).
- [Running tests and concurrency](https://nexte.st/docs/running/).
- [Slow tests and timeouts](https://nexte.st/docs/features/slow-tests/).
- [Versioned binary URLs](https://nexte.st/docs/installation/release-urls/).
