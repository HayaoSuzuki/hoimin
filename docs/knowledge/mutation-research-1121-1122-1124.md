---
type: Decision
title: Slice, division and survivor-directed selection research
description: Bounded comparisons, opt-out adoption policy and limits of survivor-triggered deferral.
status: draft
sources:
- id: report
  resource: ../superpowers/reports/issues-1121-1122-1124/README.md
  revision: 7c2a79168c8698f85cc90d7efd6bf16a183e08b3
  working_tree: untracked
  sha256: f281b031caa75045a23795c2dcf9df821b5a90f24c035a82fdf333df877f1dbd
- id: experiment
  resource: ../superpowers/reports/issues-1121-1122-1124/experiment.py
  revision: 7c2a79168c8698f85cc90d7efd6bf16a183e08b3
  working_tree: untracked
  sha256: ebb13e5af121930bf84aec51ebfa368807cff315aac1fcffc1547bad0d8734c8
- id: python314
  resource: ../superpowers/reports/issues-1121-1122-1124/results-python314.json
  revision: 7c2a79168c8698f85cc90d7efd6bf16a183e08b3
  working_tree: untracked
  sha256: 44c94771fa5790cd376a94d6ac743e90dd848e835a87edad5111024730c6cf41
- id: python315
  resource: ../superpowers/reports/issues-1121-1122-1124/results-python315.json
  revision: 7c2a79168c8698f85cc90d7efd6bf16a183e08b3
  working_tree: untracked
  sha256: 54c4047b3b930d529fffcd843a821850bab11f63ebba8f7aa80ab56cf156f698
---

# Decisions and product policy

The 2026-10-28 evaluation recommends narrowly scoped follow-ups for slice-bound
omission/movement (#1121) and true/floor division exchange (#1124). Following the
maintainer's [product direction](product-direction.md), adopted operators should
normally be enabled by default and removable with `--exclude-operators`.
This research branch adds no production operator or default-set change; it
records the evidence and minimum implementation/validation scope.[^report]

Variable-bound slices expose range requirements outside the current numeric
neighbor pool. Literal slices overlap existing neighbors, and omission can
produce equivalent behavior. True/floor exchange exposes fractional and
negative-input requirements that multiplication/modulo mutants do not expose
on these examples. Unsupported overloads raise runtime errors and are not counted
as useful missing-assertion leads.[^report][^python314][^python315]

Do not add survivor-triggered scheduling (#1122) on the present evidence. With
the same 12-candidate pool, line suppression executes seven candidates but
misses an independent requirement on the same line. Exact-span suppression
executes ten and retains both useful requirements. Existing line diversity
already finds both; neither new strategy increases useful requirements at
ceilings 4, 8 or 12.[^python314][^python315]

# Evidence and limits

The adapter uses current `plan` and public line-diverse preview ordering, direct
weak/strong controls, and real `verify` calls. Adaptive replays use completed CLI
outcomes, paying a new baseline per chosen ID. Their traces keep unexecuted and
deferred IDs separate from verification verdicts. Per-call completeness covers
only selected IDs and does not verify the entire pool.[^experiment]

The nine independent small examples ran on macOS arm64 with CPython 3.14.9 and
3.15.0b1. These are limited empirical results, not real-project efficacy,
universal equivalence, Linux/Windows results, hard macOS memory guarantees or a
Ruff operator implementation. Future adoption must preserve static discovery,
source ranges, cancellation, candidate caps and supported-Python syntax checks.
Revisit the selection decision only with evidence that its useful test
improvements justify additional report/resume/concurrency contracts.[^report]

[^report]: [Research report, decisions and minimum follow-up scope](../superpowers/reports/issues-1121-1122-1124/README.md).
[^experiment]: [Disposable experiment adapter](../superpowers/reports/issues-1121-1122-1124/experiment.py).
[^python314]: [Python 3.14.9 execution evidence](../superpowers/reports/issues-1121-1122-1124/results-python314.json).
[^python315]: [Python 3.15.0b1 execution evidence](../superpowers/reports/issues-1121-1122-1124/results-python315.json).
