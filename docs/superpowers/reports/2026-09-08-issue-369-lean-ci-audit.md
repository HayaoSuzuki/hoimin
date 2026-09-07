# Issue #369: Lean CI audit

## Scope and design

This branch adds a dedicated Linux `lean-audit` job to the blocking CI graph.
The job installs elan 4.1.2 from its versioned release, selects the repository's
Lean 4.32.2 toolchain, and caches elan plus Lake output under a key derived from
the toolchain declaration, Lake configuration, and Lean sources.

The audit is deliberately serial. It first compiles the 88 library source
modules, the aggregate `HoiminOracle` module, and all 28 executable roots one at
a time. It requests each module's native object before the aggregate library
build so later executable links do not trigger concurrent cold compilation.
It then runs a freshness check for every `lakefile.toml` generator and the
sensitivity mode exposed by 25 of those 28 generators. The generated corpora
and Lean model are unchanged.

Every build, freshness check, and sensitivity command is wrapped independently
by `tools/lean_resource_guard.py`. The draft uses a 20-second timeout, 768 MiB
aggregate RSS limit, and 250 ms sampling. Lake adds `-j1` and
`-DElab.async=false` to every Lean invocation. Per-command resource statistics
are uploaded with `if: always()` so a failed gate retains its reason and peak
measurement.

## Contract evidence

The workflow contract test executes the actual audit Bash with a recording
`python3` substitute. It verifies the working directory and every guard
argument, rejects duplicate statistics paths, and checks fail-fast behavior by
making the fourth guarded call fail and observing that no fifth call starts.

The same behavioral test derives the expected package module set from the Lean
sources and executable roots. It requires all 117 targets exactly once and
checks that every local import precedes its consumer. It also derives executable
order from `lakefile.toml` and compares the complete command stream against all
28 corpus files and all 25 supported sensitivity gates. The only root Lean file
outside that package set is `DiskGuardBrokenConsumer.lean`, an intentionally
broken proof-consumer fixture rather than a library or executable target.

TDD started with three errors because `lean-audit` did not exist. After adding
the job, all three behavioral and configuration tests passed. The combined CI
workflow and resource-guard suite passed 27 tests outside the macOS sandbox;
the sandboxed attempt returned exit 126 in the four tests that require `ps`
process-group inspection.

## Incomplete resource validation

The current 768 MiB draft is known not to complete a cold audit and is not ready
to merge. Initial aggregate compilation stopped at 804,000 KiB. Adding serial
Lean elaboration allowed `HoiminOracle.ShutdownModel` to complete on macOS at
769,248 KiB, but `HoiminOracle.ShutdownProofs` later stopped at 820,624 KiB.
In an isolated one-CPU Linux container, `HoiminOracle.ShutdownModel` stopped at
791,580 KiB after 5.706 seconds. Each stop was the guard's expected RSS-limit
exit 125; no limit was raised and no corpus or proof was changed.

These measurements establish that 768 MiB is not a viable CI bound on either
tested platform. A higher bound requires explicit approval and a complete cold
run of the exact workflow sequence. Until that succeeds, this branch provides a
reviewable workflow and tested command inventory, not evidence that the Lean CI
audit passes.

## Verification recorded at this checkpoint

| Command | Result |
| --- | --- |
| `python3 -m unittest tests.test_ci_workflow.LeanAuditWorkflowContractTests -v` | 3 passed |
| `python3 -m unittest tests.test_ci_workflow tests.test_lean_resource_guard -v` | 27 passed outside the sandbox |
| `actionlint .github/workflows/ci.yml` | Exit 1: existing custom runner label `cgroup-v2-delegated` is unknown |
| `actionlint -ignore 'label "cgroup-v2-delegated" is unknown' .github/workflows/ci.yml` | Exit 0 |

No GitHub Actions run was started. Linux validation used an isolated local
container and does not establish hosted-runner behavior.
