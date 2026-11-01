# Issues 1121, 1122 and 1124: bounded research protocol

## Scope and decision rule

Evaluate slice-bound omission/movement, true/floor division exchange,
and survivor-triggered selection on independent, specification-labelled small
examples. These are research tasks, not approval to build saved-format/scheduler
extensions in advance. Production Rust behavior stays unchanged in this evaluation.
The maintainer clarified during research that adopted operators should normally
be enabled by default and excluded with `--exclude-operators` when unwanted.
Assess whether to adopt a proposal before choosing that product implementation;
do not use permanent opt-in status as a substitute for an adoption decision.
Use the existing CLI to discover incumbent candidates and obtain its actual
line-diverse order. Python AST-based generation is an experimental adapter, not
a replacement for Ruff, source-span validation or static name resolution.

## Fixed conditions

- Compare at most two candidates per example and operator family, plus the
  existing full-profile prefix as a separate control. Report the
  complete pool size and unexecuted count too. An empty incumbent pool consumes
  zero slots; do not invent candidates to equalize actual execution counts.
- Use the same original source, Python executable, weak/strong checks and budget
  within each comparison. Run every original against both check sets first.
- Weak survivors become useful leads only if an independently stated requirement
  has a check that passes on the original and fails by assertion on the mutant.
  Count requirements separately from candidates. Runtime errors are separate.
- Selection compares the same saved candidate pool and weak test argv with
  equal execution-count budgets. Group by start line or exact mutation span;
  only `survived` blocks a group. Skipped IDs remain unexecuted, never killed or
  verified. Assess distinct requirements, not visited lines or raw survivors.
  Evaluate budget ceilings 4, 8 and 12 using a full direct-probe outcome census;
  replay the 12-candidate ceiling through the actual CLI. The adaptive replay
  makes each decision from the preceding CLI result and pays a fresh baseline
  per `verify --candidate`; smaller-budget tables are trace-prefix comparisons,
  not separately timed CLI runs.
- One worker, no third-party test framework or coverage dependency, no target
  imports during generation. Restrict proposed slices to load-context,
  one-dimensional, step-free slices with name/integer bounds. Record excluded
  calls, multidimensional slices, stores, deletes and annotations explicitly.
- Use disposable directories outside the repository and automatic cleanup.
  CLI limits: 8 GiB workspace, 10 GiB free reserve, one worker, 10-second test
  timeout and 60-second total run budget. Controlled direct probes have a
  5-second subprocess timeout. No arbitrary user-supplied target execution.
- Record Python/platform, source revision, binary hash, per-command elapsed
  time, candidate text, verdicts, policy traces and semantic controls in JSON.
  Timing is a single-run cost observation, not a performance benchmark.

## Steps and review gates

1. Read requirements and current implementation; freeze protocol and fixtures.
2. Implement the disposable experiment adapter and semantic controls.
3. Execute comparisons; audit verdicts, selection traces and adoption decisions.
4. Publish results, source catalog/OKF links and final verification.

Each step has five separate self-review passes in [self-review.md](self-review.md):
requirements, comparison validity, semantics, resources/security, and evidence.
Record findings and corrections, rather than asserting independent review.

## Acceptance boundary

Small examples can justify a limited follow-up or reject an unhelpful mechanism;
they cannot establish effectiveness across real projects. Do not call a bounded
domain equivalent case universally equivalent. Prefer an external-agent policy
when existing exact candidate selection supplies the same behavior. Any future
product adoption needs the contracts and validation listed in the final report.
