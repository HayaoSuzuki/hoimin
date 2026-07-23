# Final Rust Codebase Audit Report

## Executive summary

The audit reviewed every production Rust module across the six design areas and closed all
eight ledger entries: seven actionable semantic roots have GitHub issues and one lead was
rejected with a concrete rationale. The accepted set contains three confirmed bugs and four
high-risk designs; no maintainability-only candidate survived validation. Severity is three
P1 findings and four P2 findings, with no P0 or P3 finding.

The highest-risk roots are process containment and cleanup plus progress-decision
eligibility. Windows portable children can execute before Job Object assignment
([#23](https://github.com/tokyogas-tech/hoimin/issues/23)); termination failure can skip an
explicit root reap ([#24](https://github.com/tokyogas-tech/hoimin/issues/24)); and changing
candidate-ID sets can still produce an agent-facing saturation decision
([#25](https://github.com/tokyogas-tech/hoimin/issues/25)). Their P1 ranking reflects escaped
process-lifetime obligations or an incorrect terminal decision, even though Windows Job
Object execution itself could not be run on this host.

## Quality-gate baseline

The executable baseline was recorded on `2026-07-23` on macOS `15.7.7`
(`Darwin 24.6.0`, `arm64`). Formatting, Clippy with warnings denied, workspace
tests, core and CLI contract suites, Python tests, release build, wheel build,
wheel smoke, and the core dependency-purity assertion all completed with exit
status `0`. Complete local output is retained under
`.audit/rust-codebase/quality-gates/`; the exact command-to-log mapping is in
the audit README.

Sandboxed `uv` initially could not use `~/.cache/uv`, and the wheel smoke's
nested `uvx` initially could not write `~/.local/share/uv/tools`. Workspace
local cache reruns and an unrestricted wheel-smoke rerun passed; the initial
diagnostics remain in the evidence directory as execution-environment
limitations. Windows behavior and delegated Linux cgroup behavior were not
executed on this macOS host and remain `limited`, not passed.

The duplicate dependency report shows parallel versions in the random-number
and supporting dependency families. Baseline review did not establish a
concrete compatibility, binary-size, or security burden, so no finding was
opened from duplication alone.

## Findings by severity

| Severity | Confirmed bug | High-risk design | Maintainability | Total |
| --- | ---: | ---: | ---: | ---: |
| P0 | 0 | 0 | 0 | 0 |
| P1 | 1 | 2 | 0 | 3 |
| P2 | 2 | 2 | 0 | 4 |
| P3 | 0 | 0 | 0 | 0 |
| **Total** | **3** | **4** | **0** | **7** |

The P2 correctness issues cover reservation-ID exhaustion
([#26](https://github.com/tokyogas-tech/hoimin/issues/26)) and normalized plan configuration
validation ([#27](https://github.com/tokyogas-tech/hoimin/issues/27)). The remaining P2
design issues cover capability-relative workspace paths
([#28](https://github.com/tokyogas-tech/hoimin/issues/28)) and the unresolved meaning of
progress stall adjacency ([#29](https://github.com/tokyogas-tech/hoimin/issues/29)).

## Cross-cutting risk themes

- **Lifecycle guarantees must survive failure branches.** Pre-attach containment and
  post-termination reap are separate obligations; successful-path descendant tests do not
  establish either failure-path guarantee.
- **Validated values must remain validated after serialization boundaries.** Plan verification
  reconstructs normalized configuration without replaying constructor-owned invariants.
- **Decision tools need explicit eligibility checks and coherent public policy.** Progress
  comparison currently accepts mismatched candidate sets, while its internal stall-retention
  policy conflicts with the README's consecutive-stall language.
- **Check-then-use paths are not stable capabilities.** Workspace pathname validation rejects
  pre-existing symlinks but is not atomic against the constrained concurrent actors recorded
  in the finding.
- **Boundary arithmetic must preserve identity.** Saturating reservation-ID allocation turns
  exhaustion into silent active-map replacement.

## Areas with no actionable findings

- Core state transitions, target normalization, report scoring and exit precedence, resume
  compatibility, and ordinary budget grant/release paths produced no additional actionable
  root.
- Session schema migration, replacement, and lookup were reviewed. The only retained session
  concurrency lead was rejected because the lookup can linearize before completion and no
  stronger response-time finality contract exists.
- Fingerprint and Git inputs reject unsafe paths and pre-existing symlinks; hostile revisions
  and diff formatting are constrained by commit resolution, `--end-of-options`, and pinned
  diff arguments.
- Analyzer parsing, candidate selection, disk-backed spooling, record bounds, sequence checks,
  and replay offsets produced no actionable finding.
- JSON, JSONL, human reporting, diagnostic routing, streaming, and partial-write poisoning
  produced no actionable finding.
- CLI parsing, documented examples, Rust/Python metadata, maturin configuration, wheel smoke,
  and CI platform matrices were aligned. Duplicate dependencies and the repeated `run_e2e`
  invocation had no demonstrated compatibility, coverage, security, or material-cost impact.

## Coverage limitations

This audit ran on macOS `15.7.7` (`Darwin 24.6.0`, `arm64`). Windows Job Object behavior and
delegated Linux cgroup v2 hard enforcement were statically reviewed but not executed, so they
remain `limited`. The Windows reproduction required by
[#23](https://github.com/tokyogas-tech/hoimin/issues/23) must run on Windows; the
capability-relative filesystem design in
[#28](https://github.com/tokyogas-tech/hoimin/issues/28) requires explicit per-platform
verification.

Initial sandboxed `uv` and nested `uvx` attempts could not write their user cache/tool
directories. Workspace-local cache reruns and an unrestricted wheel-smoke rerun passed; those
diagnostics describe the audit environment, not product failures. Local ignored logs and
temporary failing-validation outputs are not committed. The report makes no claim about
absence of defects beyond the recorded static traces, focused tests, and quality gates.

## Remediation order

There are no true issue dependencies; each accepted root can be completed in one dedicated
worktree and pull request. The recommended priority waves are:

1. Correctness and cleanup prerequisites:
   [#23](https://github.com/tokyogas-tech/hoimin/issues/23),
   [#24](https://github.com/tokyogas-tech/hoimin/issues/24),
   [#25](https://github.com/tokyogas-tech/hoimin/issues/25),
   [#26](https://github.com/tokyogas-tech/hoimin/issues/26), and
   [#27](https://github.com/tokyogas-tech/hoimin/issues/27).
2. Shared-boundary refactor:
   [#28](https://github.com/tokyogas-tech/hoimin/issues/28).
3. Independent subsystem contract refactor:
   [#29](https://github.com/tokyogas-tech/hoimin/issues/29).
4. Maintainability-only follow-ups: none.

The exact future branch names and the explicit `none` dependency mapping are recorded in the
[issue map](issues.md).
