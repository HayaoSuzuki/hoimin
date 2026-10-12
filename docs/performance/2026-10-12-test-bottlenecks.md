# Python and Rust test execution costs

## Scope and approved approach

Baseline: `44ccb12`, Windows, Python 3.14 and Rust 1.99.0. The user approved
measuring both test suites and improving demonstrated bottlenecks. This is a
bounded maintenance change: retain test cases, assertions, property strategies
and resource/cleanup guarantees. Separate compilation, collection, execution
and serialization. Adopt changes only after equivalent checks and repeated
measurements; do not assume Arrow or another runner will improve small records.

## Plan

1. Measure Python collection and the complete existing suite with a fixed seed;
   profile separately so instrumentation does not contaminate baseline timings.
2. Build Rust test artifacts in an owned directory with two build jobs and no
   debug information. Measure build separately from warmed test execution.
3. Inspect slow tests, process startup, serialization and repeated CI commands.
   Select the smallest supported improvement; retain real process integration
   coverage and platform-specific checks.
4. Alternate before/after runs on the same inputs and host. Check case identities,
   assertions and outcomes, not just wall time. Record unsupported platforms and
   noisy measurements explicitly.
5. Run relevant quality gates and an independent final review. Commit evidence
   and current maintenance guidance, open a PR, and clean owned Cargo artifacts.

## Design self-review (five passes)

1. Scope: test/development costs, not changing mutation verdicts or public formats.
2. Correctness: distinguish pure unit work from actual process/OS enforcement;
   never globally patch sleep or replace subprocess coverage with mocks.
3. Resources: use two Rust build jobs, a single measurement at a time, check free
   disk, and clean only this task's Cargo target.
4. Measurement: cold compilation and warm runtime are different quantities;
   profiling overhead and Windows results cannot establish Linux speedups.
5. Alternatives: measure redundant work and startup before adding concurrency,
   new dependencies, or Arrow; retain JSON unless evidence supports migration.

## Plan self-review (five passes)

1. Baseline: fresh main and clean work branch confirmed before retained changes.
2. Comparability: fixed Python seed, same tools/inputs, paired repeated runs.
3. Coverage: preserve all collected cases and explicit failure/cleanup checks.
4. Failures: separate sandbox/environment failures from software failures; stop
   invalid timing runs instead of counting failures as fast successes.
5. Delivery: preserve raw observations, limitations, quality results and review;
   avoid introducing permanent profiling infrastructure without recurring value.
