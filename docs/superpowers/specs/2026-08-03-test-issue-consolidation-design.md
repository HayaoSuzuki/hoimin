# Test-Issue Consolidation Design

## Objective

Resolve the test issues identified in the `Test-Issue Consolidation` section
of the 2026-07-31 triage without duplicating regression coverage already added
by the related bug fixes. Each issue must end with either verified acceptance
coverage and closure, or a precisely documented platform-specific remainder.

## Scope

The first phase covers the ten issues folded into bug fixes: #153, #154, #155,
#156, #158, #161, #162, #164, #165, and #168. The second phase covers the
independent follow-ups #157, #160, #166, and #167. The broad fixture and
provenance proposals #159 and #163 are resolved last using evidence gathered
from the concrete tests; they do not introduce a shared fixture crate or a
repository-wide annotation pass.

Windows-only tests in #153 and #162 may remain as explicit follow-ups when they
cannot be made deterministic in GitHub-hosted Windows runners without adding
production-only fault-injection APIs. Portable, Unix, and Linux acceptance
coverage must not be weakened to accommodate that exception.

## Approach

Use an audit-first workflow. For every issue, map each requested acceptance
criterion to a current test and its fixing PR. Criteria already exercised at
the correct architectural boundary require no duplicate test. Missing criteria
receive the smallest test that can observe the production path described by
the issue.

Every issue is handled in its own worktree and branch based on the latest
`origin/main`. Its design or implementation note is committed in that
worktree. Pull requests are merged sequentially when they modify the same test
module, preventing later branches from accumulating avoidable conflicts.

## Issue Groups and Order

### Existing-behavior audit

Start with issues likely to be partly or fully satisfied by earlier bug fixes:

1. #155: session and sessionless termination parity, plus live-run ownership.
2. #153: first- and second-signal behavior through real operating-system
   signals.
3. #158: real executable stream routing for help, version, and reports.
4. #162: abnormal exit and stale-root identity tests for Windows Job Objects
   and the corresponding cgroup safety guarantees.

When every criterion is already covered, add an issue comment linking the test
names and fixing PRs, then close the issue without a code PR. Partial coverage
produces a focused test PR for the missing criterion only.

### CLI and persistence acceptance tests

Process the integration-heavy issues next:

1. #154: feed reports emitted by real runs into `hoimin progress`.
2. #156: exercise `run --changed` and `plan --changed --diff-base` through CLI
entry points and assert candidate selection, not merely argument parsing.
3. #161: exercise the session operations under write-lock and stale-read
contention, with bounded completion and typed diagnostics.

These tests use temporary repositories and databases. They must not depend on
wall-clock races: child readiness, held locks, and release points are
coordinated explicitly.

### Generative regression tests

Then add deterministic property coverage:

1. #165: generated zero-context Git diffs with hostile content and quoted-path
decoder totality/round trips.
2. #168: self-comparison, reversal symmetry, and mutant-order invariance for
progress comparisons, including duplicate-content stable IDs.
3. #164: bounded adversarial state-machine schedules covering cancellation,
deadline, failure, and completion reorderings.

All property tests use deterministic proptest persistence and bounded case
counts suitable for CI. A property must compare against an independently
stated invariant; it must not reproduce the implementation algorithm as its
oracle.

### Independent follow-ups

After the folded issues are resolved:

1. #157 adds full-stack timeout reporting and the missing survivor exit-code
assertion. OOM and process-limit scenarios are included only where the CI
backend can enforce them deterministically; unsupported platforms retain an
explicit issue remainder.
2. #160 checks in human-reviewable JSON and JSONL artifacts for the original
and current schema-v2 report shapes, plus SQLite fixtures at database schema
versions 1, 2, and 3. Regeneration tests compare parsed events and database
schema/data semantics rather than volatile timestamps, run IDs, SQLite page
counters, or byte-for-byte database images.
3. #166 property-tests candidate spool push/finish/replay, resume offsets, and
the serialized record-size boundary.
4. #167 property-tests fingerprint invariance under permitted permutations and
duplicates, and sensitivity to every canonical field class.

## Broad-Issue Disposition

#159 is not implemented as a new shared crate. Adversarial inputs live next to
the subsystem tests that consume them, avoiding a test-only abstraction that
couples unrelated domains. Once the concrete issues demonstrate coverage for
the cited blind spots, #159 receives links to those tests and is closed as a
consolidated umbrella.

#163 becomes a narrow convention: add a `pins:` comment only when a test locks
an intentionally surprising policy or a former defect whose expected outcome
is not evident from the assertion. Existing tests touched by this work receive
such comments when applicable. A repository-wide backfill is explicitly out
of scope; the issue is closed after the convention is documented in the test
guidance.

## Verification

Each code PR must demonstrate a failing test or missing-coverage observation
before its change, then pass its targeted tests. Before PR creation it must
also pass formatting, linting, diff checks, and the relevant crate or Python
suite. CI must be green on supported runners before merge. Platform-skipped
criteria are listed in the PR and issue rather than being reported as covered.

An independent read-only review checks every PR for test validity, false
positives, nondeterminism, and whether the test actually crosses the boundary
named by the issue. After all phases, query the complete issue set and report
which issues are closed and which retain an explicit platform-specific
remainder.

## Completion Criteria

- Every requested criterion has a test link or an explicit unsupported-platform
  remainder.
- No test duplicates equivalent coverage at a weaker boundary.
- Each change is isolated by issue, documented, reviewed, merged, and followed
  by worktree cleanup.
- #159 and #163 are resolved through the constrained dispositions above.
- The final report distinguishes closed work from intentionally retained
  Windows or backend-specific follow-ups.
