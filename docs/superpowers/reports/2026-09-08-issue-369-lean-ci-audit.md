# Issue #369: Lean CI audit

## Scope and design

This branch adds a dedicated Linux `lean-audit` job to the blocking CI graph.
The job installs elan 4.1.2 from its versioned release, selects the repository's
Lean 4.32.2 toolchain, and caches downloaded toolchains plus Lake output under a key derived from
the toolchain declaration, Lake configuration, and Lean sources.
The cache excludes elan binaries so an old cached installer cannot bypass a
future installer-version update.

The audit is deliberately serial. It first compiles the 88 library source
modules, the aggregate `HoiminOracle` module, and all 28 executable roots one at
a time. It requests each module's native object before the aggregate library
build so later executable links do not trigger concurrent cold compilation.
It then runs a freshness check for every `lakefile.toml` generator and the
sensitivity mode exposed by 25 of those 28 generators. The generated corpora
and Lean model are unchanged.

Every build, freshness check, and sensitivity command is wrapped independently
by `tools/lean_resource_guard.py`. The configuration uses a 20-second timeout, 2 GiB
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

## Initial resource validation

The initial 768 MiB draft could not complete a cold audit.
Initial aggregate compilation stopped at 804,000 KiB. Adding serial
Lean elaboration allowed `HoiminOracle.ShutdownModel` to complete on macOS at
769,248 KiB, but `HoiminOracle.ShutdownProofs` later stopped at 820,624 KiB.
In an isolated one-CPU Linux container, `HoiminOracle.ShutdownModel` stopped at
791,580 KiB after 5.706 seconds. Each stop was the guard's expected RSS-limit
exit 125; no limit was raised and no corpus or proof was changed.

These measurements establish that 768 MiB is not a viable CI bound on either
tested platform. Follow-up validation uses a 2 GiB limit with the same 20-second
deadline. The Linux container also enforces 2 GiB with no swap and one CPU;
the repository sources are read-only and the build cache starts empty.

## 2 GiB validation and remaining blocker

The exact workflow Bash ran in `rust:1.98-bookworm` on Linux aarch64 with
`--memory=2g --memory-swap=2g --cpus=1 --pids-limit=128`. All 117 native module
builds and the aggregate library build passed from an empty build cache.
`ShutdownProofs` took 12.424 seconds and peaked at 943,656 KiB.
The state-machine corpus check also passed.

The next command, `lake exe generate_budget -- --check corpus/budget-cleanup.jsonl`,
stopped after 20.242 seconds with exit 124 and peak RSS 1,054,276 KiB.
Of 120 recorded commands, 119 passed and one timed out. The remaining 26
freshness checks and 25 sensitivity gates did not run because the job is
fail-fast.

A reduced-work probe omitted Lake and executable linking:

```sh
python3 tools/lean_resource_guard.py \
  --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 250 \
  --stats /stats/budget-runtime-only.json \
  -- .lake/build/bin/generate_budget --check corpus/budget-cleanup.jsonl
```

Under the same container limits, this also stopped with exit 124 after 20.184
seconds, at 89,632 KiB peak RSS. Separating linking from execution is therefore
not sufficient. Generated `BudgetModel.c` initializes `reachableStateCount`,
`checkedTransitionCount`, and `boundedAuditPasses` at module startup. These
constants evaluate the depth-six exhaustive exploration even though the corpus
checker does not consume them; `BudgetMain.lean` uses them only in `--stats`.

This is an infrastructure timeout, not a corpus mismatch. No Lean source,
proof, search depth, or generated corpus was changed. The complete audit and
PR remain blocked on separating statistics evaluation from checker startup;
no longer timeout was attempted.

## Verification recorded at this checkpoint

| Command | Result |
| --- | --- |
| `python3 -m unittest tests.test_ci_workflow.LeanAuditWorkflowContractTests -v` | 3 passed |
| `python3 -m unittest tests.test_ci_workflow tests.test_lean_resource_guard -v` | 27 passed outside the sandbox |
| `actionlint .github/workflows/ci.yml` | Exit 1: existing custom runner label `cgroup-v2-delegated` is unknown |
| `actionlint -ignore 'label "cgroup-v2-delegated" is unknown' .github/workflows/ci.yml` | Exit 0 |
| `cargo fmt --all -- --check` | Exit 0 |
| All eight `hoimin-core` `lean_*` integration-test binaries | 31 tests passed |
| All nineteen `hoimin-cli` `lean_*` integration-test binaries | 72 tests passed |
| `git diff --check` | Exit 0 |

The 2 GiB guard change was tested red-to-green against the actual workflow
command stream. The combined Python suite was rerun and passed all 27 tests.
The CLI integration tests required the repository Python environment at the
worktree-relative `.venv` path; a temporary symlink supplied it for the passing
run and was removed afterwards. An earlier run without that environment failed
the candidate-ranking adapter's command-launch assertion.

No GitHub Actions run was started. Linux validation used an isolated local
container and does not establish hosted-runner behavior.
