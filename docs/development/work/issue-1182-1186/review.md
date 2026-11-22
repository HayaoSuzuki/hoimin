# Guidance implementation and verification review

The five design and five plan passes are in [design.md](design.md) and
[plan.md](plan.md). These reviews cover the #1182/#1186 documentation work;
the two historical evaluation reports retain their own separate reviews.

## Application scenarios

Five fresh-context exercises without the added guidance and five with the skill
examined input/assertion interaction, a strict mock's extra kill, and existing
integration coverage under different argv. Agents only reasoned from supplied
outcomes; they did not execute experiments or represent human reviewers.

The unguided exercises already made the central behavioral judgments correctly.
For example, one response said, "For these tested alternatives, input and assertion
changes are jointly necessary; neither change alone explains the kill."
Another said, "The mock increases mutation score, but the ninth kill does not yet
demonstrate improved correctness coverage." A third said, "The survival is scoped
to the focused command; the existing suite already contains a detecting test."
Do not call this an observed agent-behavior failure repaired by the new skill.
One control prompt ambiguously grouped the fixed fake with 8/9 observations;
exclude its fake-count interpretation from any comparison. These are qualitative
usability exercises, not a randomized or identical-prompt effectiveness study.

**Ruling:** The user explicitly requested reusable guidance based on the evaluations.
Proceed with that documentation even though unguided scenario answers are already
reasonable; claim discoverable, reviewable instructions, not a measured improvement
in agent success rate. No new automatic classifier follows from these exercises.

All five guided responses correctly separated joint input/assertion changes,
call-shape overconstraint and existing coverage; none invented human acceptance.
Representative evidence:

| Exercise | Observed interpretation |
| --- | --- |
| Guided 1 | "The supplied data do not prove equivalence for all inputs." It requested ordinary contract cases before repairing the fake. |
| Guided 2 | "B's ignored arguments alone do not establish a substitute defect." It required a new plan for changed argv. |
| Guided 3 | "The two runs" must not be treated as a controlled improvement; it asked whether merely citing existing evidence requires a new plan. |
| Guided 4 | "Cite the existing command/result; no rerun is necessary merely to record it." It separated human review from recommendations. |
| Guided 5 | It explicitly marked missing baseline/failure diagnostics as unverified and the ten minutes as a ceiling, not measured expenditure. |

The first three exercises identified two clarifications: citing an existing run
does not itself require replanning, and proposing a simpler manual alternative is
not measuring human effort. Both were clarified; the final two exercised that
wording. Pre-existing Unix shell setup assumptions remain owned by the planning
workflow and installation reference; this change adds no execution commands.

## Implementation self-review: five passes

1. Contract ownership: execution safeguards remain in SKILL.md. The new reference
   explains decisions and links back instead of duplicating budget commands or
   introducing flags, schemas, automatic generators or classification rules.
2. Behavioral interpretation: joint improvements are explicit; a fixed fake can
   mask a request change, but correct caller isolation is valid. Exact call shape
   is enforceable only when an independent contract requires it.
3. Evidence boundaries: existing integration coverage is not credited as a new
   kill. Controlled subjects, real projections, real copies and actual bugs remain
   distinct. Human acceptance and unknown costs are explicit record fields.
4. Publication: one installed skill includes all local references. Knowledge stays
   on GitHub under existing site policy, with a published overview link. All six
   Markdown sources from the two research reports are added to the source catalog.
5. Adoption scope: next skill/docs tasks are implemented, while cross-project
   repairs and human-review measurements remain research questions. The PR must
   reference rather than auto-close #1182 and #1186.

## Test self-review: five passes

1. Regression value: use existing skill and publication tests; do not add phrase
   assertions that merely repeat new prose. No production behavior changed.
2. Packaging: `gh skill publish --dry-run` exits 0. An isolated copy contains
   LICENSE, SKILL.md, installation.md and test-design.md; all local links stay
   inside that package and resolve. No release is published by the check.
3. Provenance: parse new/changed OKF frontmatter, verify the ten added/referenced
   source hashes and footnotes, and compare report totals with raw plan/execution
   files. Historical outcomes are distinguished from checks run for this PR.
4. Tests: run the existing skill/docs suite and both retained research corpora.
   Research tests retain parameterization, property-generated corpora and AAA;
   no new executable examples require inventing an unrelated implementation.
5. Review and publication: strict site build and local source links are checked.
   A fresh reviewer of the whole branch found no actionable or blocking issues,
   checked reported totals and baselines, and verified 139 local links. CI results
   are reported separately from local checks; pending checks are not called passed.

## Design and documentation scores

software-design-philosophy: initial 6/8, 7.5/10; final 8/8, 10/10 for this scoped
documentation design. Each owner has one describable purpose, interfaces are short,
examples are hidden behind a bundled link, comments/intros state intent, design is
reviewed, each page owns a decision, boundaries are discoverable, and design
improvement was done before publication. No diagnostic remains failed. This is
a self-assessment of complexity, not a score for empirical research quality.

technical-documentation: initial 8/10, with provenance verification and source
discoverability incomplete; final 10/10 after adding OKF metadata, catalog entries
and the overview link. Audience, document type, verified behavior, headings,
procedures, sample handling, voice, accessible links/tables, timeless guidance and
English terminology pass. Historical dates belong to evidence, not promises about
current behavior. No unverified command examples or image-only instructions added.

Durable check receipts are in [validation.json](validation.json) and
[checks.json](checks.json). Source mutations and Lean proofs were not rerun for
this docs-only change; their exact historical receipts remain in the two reports.
