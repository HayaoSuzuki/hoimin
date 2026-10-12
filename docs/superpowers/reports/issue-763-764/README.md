# Second-order mutation adoption assessment

## Approved probe and decision rule

On 2026-10-12 the user approved a research-first assessment of #763/#764,
with a single worker and a ten-minute execution budget. Main was updated to
`e7c0f98` before creating `research/763-764-second-order-assessment`.
Product infrastructure is conditional on demonstrated useful test improvements.

Compare artificial examples and two fixed pure functions:

- proofreadtools `_length_ratio_upper_bound`, revision
  `409ed955c0695ec04976842ba9fd22c64c03c33a`;
- hoimin `tools/ci_selection.py::categories_for`, revision `e7c0f98`.

Use at most 16 first-order edits and 120 nonoverlapping pairs per function.
Use existing operator families (integer neighbors, min/max, arithmetic,
comparison, membership and Boolean replacements). Do not import the full
proofreadtools NLP stack. Record the extracted function and original file hash;
projected fixed calls are **not** a full upstream test-suite execution.
Use a separately specified finite validation set to investigate survivors.
Survival on a finite set does not establish semantic equivalence.

Compare first-order-only, random pair ordering and pair ordering favoring
shared killing cases then source proximity. Seeds are 763, 764 and 765.
Each arm has a 60-second maximum, within the ten-minute total. Charge baseline,
first-order measurements needed for guidance, generation and analysis separately.
If a finite candidate set is exhausted early, report that fact; do not claim an
equal-wall-time advantage from unequal amounts of available work or cached results.

For a fixed case set T, observed SSHOM requires
`empty != K_T(H) <= K_T(m1) & K_T(m2)`; strict subsumption requires a proper
subset. Empty or incomplete rows are not SSHOM evidence. Distinguish killed
pairs, surviving pairs, cancellation and new actionable gaps. Confirm any proposed
additional assertion passes the original and fails the pair for the intended
reason, and compare its diagnostic value with first-order witnesses.

Only bounded, reviewed arithmetic and path-classification expressions execute.
There are no mutated loops, arbitrary input files, commands, plugins or new
product ingress. No Rust compilation, new dependencies, workflow changes or
shared-build cleanup are needed. Inputs/output and candidate counts are bounded;
record observed memory/disk use without claiming an unimplemented hard memory cap.
The probe is research-only and does not implement #762's product kill matrix.

## Design self-review: five passes

1. Checked the user wants test improvements within a local agent budget, rather
   than a score, maximal SSHOM count or an unconditional feature implementation.
2. Checked #763/#764's triage: collect cheap evidence before composite candidate,
   persistence, resume and restoration changes; keep #762 independently deferred.
3. Checked mathematical definitions distinguish strict/non-strict subsumption,
   survivors, cancellation and finite observation from semantic equivalence.
4. Checked source versions, function extraction, independent expected outputs
   and projected-test limitations; do not mislabel them as full upstream tests.
5. Checked single worker, finite source/cases/pairs, total/arm deadlines and no
   external product inputs, heavy dependencies, mandatory CI or shared cleanup.

## Plan self-review: five passes

1. Freeze candidate families, caps, seeds and targets before observing outcomes.
2. Validate the relation classifier against independent artificial truth tables,
   including a survivor that becomes a strict SSHOM when a case is added.
3. Preserve source hashes, edits, per-case results, validation witnesses and
   timings so the recommendation can be independently checked.
4. Record generation/baseline/guidance costs and early exhaustion; do not present
   ordering an already collected matrix as a new paid search experiment.
5. Review implementation and test evidence five times, then obtain a separate
   final review before committing results and opening the research PR.
