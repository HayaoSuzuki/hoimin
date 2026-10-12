# Second-order mutation adoption assessment

## Outcome

Do not implement #763's composite execution infrastructure or #764's search
subsystem now. The bounded investigation found one genuine missing test, but
the existing first-order `string_literal_empty` operator exposes the same gap.
Retain that regression case and this evidence. This is an adoption decision for
hoimin's local-agent workflow, not proof that higher-order mutation cannot help.

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

## Literature assessment

[Jia and Harman (2009)](https://doi.org/10.1016/j.infsof.2009.04.016)
introduced subsuming higher-order mutation and evaluated ten programs. The
publisher's full text was unavailable in this session; its abstract and later
papers establish historical context, not a fresh full-text review.

[Harman et al. (2014)](https://discovery.ucl.ac.uk/id/eprint/1456659/) report
positive effectiveness/efficiency results from SSHOMs constructed from 15,792
first-order mutants in four Java programs. Selected combinations can help in
that setting. This does not establish cost-effectiveness for arbitrary
second-order Python pairs in a local agent workflow.

[Chen (2018)](https://arxiv.org/html/1809.04563) derives nonempty kill-set
containment and searches configurations using variational execution and SAT/BDD
reasoning. Its small Triangle experiment includes orders beyond two. Its
4.5-second search is not a Python benchmark. We use the definitions without
implementing a variational interpreter.

[Oh et al. (2021)](https://arxiv.org/html/2104.11005v1) use causal dependence
to select SSHOMs in two C subjects. The dependence analysis itself needs mutation
executions. They distinguish SSHOM counts from distinct kill vectors and also
report that higher-order mutants can be easier to kill. Our shared-killer and
proximity ordering is a lightweight hypothesis, not CPDA or its reproduction.

[Guizzo et al. (2020)](https://discovery.ucl.ac.uk/id/eprint/10100738/1/fse-v1.9.pdf)
show why mutant counts are unreliable substitutes for actual execution cost.
We record actual times but make no savings claim from tiny in-process timings.
The [mutmut maintainer](https://github.com/boxed/mutmut/issues/174#issuecomment-638792659)
also reports negligible benefit for the additional cost. That experience is
relevant Python-tool evidence, not a controlled experiment or general proof.

## Finite experiment results

Windows, CPython 3.14.6, one process/worker. Each variant evaluates every case
twice, without fail-fast. The separately specified validation sets partly
overlap the original semantic input space; they are broader validation,
not fully disjoint holdouts.

| Subject | First-order edits | Eligible pairs | Projected cases | Validation cases | First-order survivors | Pair survivors | SSHOMs / strict |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| proofreadtools length bound | 6 | 14 | 4 | 99 | 0 | 0 | 6 / 0 |
| hoimin path classification | 10 | 45 | 23 | 41 | 0 | 1 | 1 / 0 |

The proofreadtools cases project one direct assertion and three pruning
conditions from `tests/test_variant_candidates.py`; they do not execute
Levenshtein, candidate enumeration or the whole upstream suite. Validation uses
length pairs 0..9, excluding two empty inputs, with an exact rational oracle.
The hoimin projection uses the existing explicit path cases; validation covers
path classes and `CONTRIBUTING.md`.

Both orderings exhaust all eligible pairs for all three seeds. Repeated
discoveries are not distinct improvements. Median arm times include first-order
preparation, compilation, both evaluations and validation:

| Subject | First-order only | Random pairs + first order | Guided pairs + first order |
| --- | ---: | ---: | ---: |
| Length bound | 0.000667 s | 0.002102 s | 0.002160 s |
| Path classification | 0.001878 s | 0.009771 s | 0.010247 s |

Generation took 0.000772/0.002248 s; baseline took 0.000133/0.000265 s.
The initial complete probe took 0.0931 s internally and 0.5953 s as a process.
These times do not model command isolation, fixtures, cleanup or plan
maintenance. There is no equal-wall-time win: the first-order subset is
exhausted earlier, and omits several existing operator families.

Only two of the six length-bound SSHOMs also satisfy containment on broader
validation; the path-classification SSHOM does not. Relationships depend on T.
No equivalence, score replacement or semantic subsumption is inferred.

### One useful gap, also detectable by first-order mutation

The surviving pair changes two tokens within the documentation `.md` guard:

```python
# Original:
path == "CONTRIBUTING.md" or path.startswith("docs/")
# Pair:
path != "CONTRIBUTING.md" and path.startswith("docs/")
```

Each constituent is killed by existing cases. Together they incorrectly
select every check for `CONTRIBUTING.md`, while retaining the tested behavior.
The original correctly selects no Python/Rust/formal/distribution checks for
this documentation file; its documentation workflow still runs separately.
Random ordering reaches this pair at positions 30/32/44 for seeds 763/764/765;
guided ordering reaches it at position 31 each time. Its constituents have
disjoint original killing-case sets, so shared-killer priority does not target
this particular gap.

A post-hoc first-order comparator replaces just `"CONTRIBUTING.md"` with `""`.
This is the existing default-enabled `string_literal_empty` family in
`crates/hoimin-cli/src/analyzer/rust.rs` and `crates/hoimin-core/src/config.rs`.
We instantiate the known replacement by hand, without claiming a complete
analyzer-generated plan. It also survives the original cases and fails the
same missing assertion. This comparator is not a preregistered search arm or
a measured time-to-discovery advantage.

The entire existing `tests/test_ci_selection.py` then ran against owned copies
of the full module and tests, including the CLI subprocess checks:

| Variant | Original 66 tests | With CONTRIBUTING case: 67 tests |
| --- | --- | --- |
| Original | Pass | Pass |
| First constituent | 1 failure | Not rerun |
| Second constituent | 3 failures | Not rerun |
| Second-order pair | Pass | Only new case fails |
| First-order string replacement | Pass | Only new case fails |

The outcome is **one useful regression case**, and **zero demonstrated gaps
exclusive to second-order mutation**. The existing classification test table
now includes that path. Product behavior is unchanged. Broader first-order
support should be considered before adding composite infrastructure for this
same weakness.

### Artificial truth tables

For `int(x == 0) + int(y == 0)`, independently changing both zero literals to
one is detected at `(0, 1)` and `(1, 0)` for each constituent, while the pair
survives. Adding `(0, 0)` detects the pair and yields strict observed subsumption.
First-order adequacy for a selected subset therefore does not guarantee
second-order adequacy, even though exclusive value was not shown in our subjects.

Changing both constants in `x + 1 - 1` to two cancels for integer x. The finite
table confirms the observations; algebra explains this particular identity.
This is not a general equivalence detector. Each artificial subject has two
selected edits and its sole pair, covering the complete selected combination
space with the three measured variants.

## Costs, limits and reproduction

The isolated eight-run confirmation took 58.256 s, including interpreter,
pytest, Git/CLI calls and evidence capture; individual runs took 6.53–7.74 s.
An earlier attempt passed baseline and killed both constituents, then exceeded
its 20-second pair limit. That was inconclusive. The successful rerun used
45-second per-run and 240-second overall guards. Both attempts and the finite
probe remained below the ten-minute execution budget. Literature reading,
source acquisition, analysis and agent review are additional costs, not
included in execution timings or presented as zero-cost work.

An initial fixture incorrectly treated `docs/cli-reference.md` as generic
documentation. Baseline rejected it before results were saved; the expectation
was corrected to the existing explicit Rust policy. No production defect or
mutation benefit was inferred from this fixture error.

The sampler observed 5,357,568 process bytes during startup. The short probe can
finish between samples, so this is not a reliable peak or a hard memory cap.
Source/case/pair counts are finite, without compiler/NLP model loading. C: free
space stayed above 186 GB. No local Rust build was performed, leaving no owned
target to cargo-clean; no shared target was cleaned.

Retained evidence:

- `evidence.json.gz`: source excerpts, byte edits, fixed cases, all rows,
  repeated-evaluation stability flags, timings and seeds. Whole-file hashes
  refer to the exact input files supplied to the experiment.
- `analysis.json`: derived counts, ordering positions and uncompressed data hash.
- `process.json`: outer execution observations, with sampler limits above.
- `followup.json` and `followup-logs/`: full isolated confirmation and earlier
  inconclusive-attempt notes. Raw file hashes can depend on checkout newlines;
  the updated test's LF-normalized hash is also recorded.
- `probe.py`, `followup.py`, `test_probe.py`: unsupported research artifacts;
  no product registration, new dependency or automatic execution path.

To replay, use an authenticated GitHub contents API to retrieve the fixed
proofreadtools file and decode its base64 `content` into
`.superpowers/issue-763-764-sources/src__proofreadtools__checks___variant_candidates.py`.
Use the stated hoimin base revision for the module/constants and Python 3.14:

```powershell
python docs/superpowers/reports/issue-763-764/probe.py
python docs/superpowers/reports/issue-763-764/followup.py
python -m pytest tests/test_ci_selection.py docs/superpowers/reports/issue-763-764/test_probe.py -q
python -m ruff check --no-force-exclude docs/superpowers/reports/issue-763-764
python -m ty check --config 'src.include=["docs/superpowers/reports/issue-763-764"]' docs/superpowers/reports/issue-763-764
```

The probe overwrites research evidence; use a separate checkout/output copy to
retain original measurements. The confirmation gets old tests through
`git show e7c0f98:tests/test_ci_selection.py`, and uses the current regression
case for updated tests. Final analysis source computes the artificial expanded
sets from observed validation rows and writes deterministic gzip; the original
recorded JSON/timings are preserved, with identical set results.

No generalization is claimed to stateful code, larger applications, other
operator sets or industrial efficiency. This inexpensive adoption gate does
not complete the larger equal-budget study originally envisioned by #764.

## Implementation self-review: five passes

1. Checked immutable UTF-8 byte offsets, nonoverlap rejection and order-independent
   edits; original sources are never mutated in place.
2. Checked fixed reviewed expressions, candidate caps, one worker and deadlines;
   there is no user-defined executor or new product input format.
3. Checked complete observations, repeated stability, arithmetic exceptions versus
   infrastructure failures, and nonempty kill-set containment.
4. Checked preparation/validation accounting, exhaustion and the post-hoc
   comparator; cached matrix ordering is not timed as a paid search experiment.
5. Checked full-module isolation, including subprocess imports, and the single
   regression assertion; product/schema/workflow behavior is unchanged.

## Test self-review: five passes

1. Initial checks failed to collect before the probe existed; this is setup red,
   not evidence of behavioral fault sensitivity.
2. Properties compare containment with its casewise definition and reject
   empty-set success, rather than trusting survivor counts.
3. Properties cover variable-length Unicode edits and reversed order; explicit
   cases reject stale and overlapping edits.
4. Artificial tables cover strict subsumption and cancellation. Baseline rejects
   incorrect fixtures; timeout/incomplete observations are never filled as pass.
5. Original tests let the pair/string mutant survive; the additional case kills
   each for the intended reason while the original passes. Actual worktree
   verification passed 67 existing tests plus 5 research checks (72 total).

## User-requested package metadata

The user supplied and requested inclusion of the package description
`Mutation Testing CLI, Designed for AI Agents` and removal of an obsolete Ruff
policy-origin comment. Preserve both edits verbatim. Lint selection, dependency
requirements and runtime behavior are unchanged; the description changes
distribution metadata. No claim about higher-order efficacy depends on it.


## Independent final review and final verification

A separate reviewer marked this ready with no material findings. They reproduced
all 77 distinct baseline/first-order/pair rows from recorded sources, excluding
timings, and checked counts, containment, cross-arm stability, source/evidence
hashes and the eight isolated confirmation logs. They also checked default
first-order availability, the separate documentation workflow, metadata scope
and catalog references. Review was read-only and lightweight; they did not rerun
Rust builds, isolated pytest suites or external literature searches.

The implementer's final checks passed: 72 tests, explicit research/regression
Ruff and formatting, explicit type checking, `uv lock --check --offline`, touched
knowledge YAML/source hashes/local links and `git diff --check`. The final probe
source reproduced all saved observations with timing fields excluded, preserving
the original measured evidence.
