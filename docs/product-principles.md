# Product principles

hoimin is designed for AI agents improving Python tests on a local development
machine. The main workflow is to select a small set of mutations, investigate
survivors, add useful behavioral tests, and verify those improvements within a
resource budget. Humans resolve specification questions and review proposed
changes; they should not have to classify a large undifferentiated survivor list.

The maintainer established this direction on 2026-10-12 and clarified the
local-only scope on 2026-10-23. An external agent uses the CLI and the
[distributed skills](agent-skills.md); hoimin contains no autonomous LLM.

## Optimize for useful test improvements

A survivor is a lead to investigate. It can expose a missing assertion or test
case, represent equivalent behavior, make a change allowed by the specification,
or require more investigation. The agent must identify an observable requirement
before adding a test. Ambiguous requirements need a human decision; the current
implementation alone is not an independent specification.

Prefer a small test that passes on the original code and fails on the selected
mutant for the intended behavioral reason. Do not freeze implementation details
or alter production code just to kill a mutant. Record the rationale and evidence
for useful, unhelpful, suspected-equivalent, and unresolved candidates separately
from their execution outcomes. An LLM's equivalence assessment is not a proof.
The existing [review commands](usage.md#human-review-of-mutation-candidates)
can retain these assessments without changing scores or suppressing candidates.

Prioritize focused selection, readable changes, machine-readable diagnostics,
reproduction, stable candidate identity, and bounded re-verification. Stop when
the agreed scope or budget is exhausted, or remaining candidates lack a justified
test improvement. Report what remains unverified rather than chasing every kill.

## Treat scores as observations

Coverage shows execution, while mutation testing probes whether checks notice
selected changes. A kill ratio still depends on the target, operators, test
command, and treatment of inconclusive outcomes. Equivalent mutants can survive
any behavioral test, and specification-permitted changes need not be rejected.
Increasing the ratio is therefore not the product's objective or a standalone
guarantee of test quality.

Use comparable before/after results as supporting evidence for an improvement.
Keep the candidate set and verification conditions identifiable, distinguish
ranking scores from mutation scores, and follow the existing
[progress comparison rules](usage.md#mutation-progress-reports). Neither a
survivor nor reported saturation proves equivalence or completeness. A
sampling interval, if added later, would address sampling uncertainty; it would
not resolve these limitations of the underlying metric.

## Resource control is a core requirement

Unattended mutation loops can exhaust memory, fill disks, and make a developer's
machine unusable. Memory, disk, process, output, and time limits, descendant
termination, cleanup, and honest partial results are essential to delegating
execution to an agent. They are part of the main use case, not optional
large-scale CI optimizations.

Start with narrow targets, few candidates, and one worker. Inspect the selected
backend and account for concurrent process trees before increasing parallelism.
On a resource-limit stop, measurement failure, or failed/deferred cleanup, stop
the loop, diagnose it, and account for retained paths. Do not silently raise
limits or retry until the machine is exhausted. Keep timeout, error, cancelled,
and not-run outcomes distinct from conclusive kills and survivors.

These controls reduce risk; they are not a guarantee that the host cannot run
out of resources or a sandbox for untrusted code. Hard memory enforcement is
backend-dependent and per root tree, the CLI's analyzer is outside descendant
memory limits, and disk monitoring is cooperative unless backed by a verified
quota. Read the [resource contract](usage.md#limits-and-defaults) and
[disk-safe execution guide](usage.md#disk-safe-execution) before choosing budgets.

## Keep product features focused on local work

Running hoimin in users' CI pipelines is outside the intended product scope.
Do not add mutation-score thresholds, regression gates or compatibility aliases
for those features. Complete run/verify executions return success even with
survivors, and valid progress comparisons return success even with regressions.
Agents use the reported statuses, scores and histories to choose useful work.
Keep hoimin's own development CI, release checks and resource-control tests.

Evaluate proposals by the useful test improvements an agent can make within a
local budget, the clarity of the evidence, and the cost of operating and
maintaining the feature. New input formats, collectors, and execution paths need
concrete value that justifies their validation and security burden. Avoid tests
that merely repeat implementation details or add cost without protecting a
meaningful behavior; preserve checks for incorrect verdicts, input validation,
resource control, cleanup, and distribution failures.

Newly adopted mutation operators normally belong to the default runtime set;
agents can exclude them with `--exclude-operators`. Use opt-out as the default
adoption policy, rather than requiring agents to discover and enable accepted
operators individually. Research candidates still need an adoption decision;
this policy does not enable unevaluated proposals automatically.

User-facing CI result aggregation (#760) is outside this product scope.
Sampling intervals (#765) remain deferred until concrete local demand exists.
Static prechecks (#758) need a demonstrated benefit beyond the existing verification command. Higher
order mutation proposals (#762–#764) need small, inexpensive evidence of useful
test improvements before product infrastructure is built. These are prioritization
criteria, not implementation commitments or changes to Issue states.

A separate Python fuzz-testing tool (#806) has its own goals and adoption
decisions. Keep it separate from hoimin feature and code improvements (#807);
neither requires implementing the other.
