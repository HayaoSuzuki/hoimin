---
type: Playbook
title: Measuring test execution costs
description: Separate collection, compilation, process startup and data processing before optimizing tests.
status: draft
sources:
- id: measurement
  resource: ../performance/2026-10-12-test-bottlenecks.md
  revision: 8056485
  working_tree: modified
  sha256: 795b46e169ebc2cbc08365a691f8daa493b8f0cff14680c164174a366de70435
- id: observations
  resource: ../performance/test-execution-2026-10-12/measurements.json
  working_tree: untracked
  sha256: 02d2f3e1d795e80b0f0fb306d65e71a74a1247f5ab56af614f7dfbc84318a5e2
- id: guide
  resource: ../development.md
  revision: 44ccb12
  working_tree: modified
  sha256: 14e9be0ad1dd6f51991568790bdc371923ebdb506af9519d87b1710ff84fb1c2
---

# Measuring test execution costs

Measure Python collection, test setup/calls/teardown and child-process work
separately. Measure Rust compilation separately from a warmed workspace command.
Retain case identities, outcomes, skips, assertions and property strategies when
comparing variants. Profile separately; cumulative nested costs cannot be summed,
and a parent Python profile excludes child CPU work.[^guide]

The 2026-10-12 Windows comparison retained all 1,006 original Python cases and
added three checks. A lighter already-fake workflow recorder reduced three fixed
scenarios from about 96 to 41–42 seconds across two observations per variant.
The full-suite pair observed about 24% lower wall time; this is not a Linux/CI
projection or an estimate for every future run. The Rust workspace passed, and
its repeated E2E invocation was removed from the full gate because the same
default-feature target already executes in that workspace command.[^measurement]

Retain actual CLI, CPython, timeout, descendant-cleanup and OS resource checks.
Use lightweight recording stubs only where the underlying operation was already
substituted. The recorder's NUL transcript is private test infrastructure and
does not change any product JSON format.[^measurement]

The selected Python profile attributes little parent-process self time to
standard JSON frames. It does not measure Rust or child-process serialization,
and no Arrow comparison was performed. Reconsider formats only after measuring
representative data flow, conversion, memory and compatibility costs. Recheck
when workflow commands, fixture behavior, dependency versions, target selection,
property budgets, host OS or concurrency change.[^observations]

[^measurement]: [Measurement report](../performance/2026-10-12-test-bottlenecks.md).
[^observations]: [Retained observations](../performance/test-execution-2026-10-12/measurements.json).
[^guide]: [Development guide](../development.md#measuring-test-execution-costs).
