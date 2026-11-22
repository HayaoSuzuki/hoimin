# Evaluation review record

The five design and five plan passes are recorded in [design.md](design.md).
This record covers the retained evaluation implementation and its tests. These
are self-reviews, not independent peer-review approvals.

## Implementation self-review: five passes

1. **Selection and contamination.** Inspected full candidate descriptors and
   found the pilot's `--source .` also selected tests and the other subject.
   Removed the source selector, checked every candidate path against its subject,
   and reran all arms. Kept the pilot separately and excluded its kill counts.
2. **Comparison integrity.** Checked IDs, spans, original/replacement text,
   candidate populations, concrete input corpora, baseline results and complete
   endings. Broad discovery input includes every narrow input. Each comparison
   changes one intended test dimension. Held-out inputs are labeled separately.
3. **Failure attribution.** Replayed all 21 distinct kills in fresh copies and
   inspected failing tests. Distinguished intended early `OSError`, assertion
   failure and missing expected error from collector/infrastructure failure.
   Added a negative audit test for pytest exit 2 and fixed its demonstrated miss.
4. **Resource and source ownership.** Checked one-worker hard Windows CLI limits,
   bounded argv, timeout coverage, literal fixed files in diagnostic replay,
   no network seam, and original source hashes. The replay probe only writes
   temporary copies; source targets and product code remain unchanged. Native
   Lean's internal allocation cap is documented separately from aggregate RSS.
5. **Adoption and complexity.** Checked that the report does not turn toy scores
   into industrial effectiveness, nine existing retry checks into new bugs, or
   original rollback assertions into proven mutation sensitivity. Existing CLI
   orchestration covers the demonstrated needs. Deferred collectors, engines,
   new product flags and speculative operator additions.

## Test self-review: five passes

1. **One case and AAA.** Each parameter or generated input gets fresh state;
   one operation history is one case. Drivers may branch to simulate operations,
   but tests do not choose unrelated assertions by scenario. Mutant diagnostics
   record concrete case IDs; setup red is distinguished from semantic sensitivity.
2. **Input boundaries.** Checked empty/zero/negative/duplicate coverage, a held-out
   seed, concrete saved inputs, and an independent membership oracle. Exhaustive
   checking covers 400 lists over -3..3 with lengths 0..3; generated checks extend
   lengths and values without claiming exhaustiveness.
3. **State and fault boundaries.** Checked fresh/warm caches, changed values,
   deletion and missing reads; generated histories reset for every example.
   Fault tests distinguish success after 0..2 failures from exhaustion after 3,
   and generated tests vary old/new Unicode values and failure counts. No sleep
   or live HTTP request occurs in the experiment.
4. **Bad evidence and negative controls.** Checked rejection of timeout, OOM,
   errors, inconclusive/not-run outcomes, failed baseline, incomplete report and
   collection exit 2. The last failed before its fix and passed afterwards.
   Lean accepted eight declarations without `sorryAx`, and rejected two
   intentionally changed definitions. Failed negative controls are not proofs.
5. **Correspondence and regression scope.** Checked 21 diagnostic replays, 12
   already-existing upstream propagation tests, the report's matrix, and the
   small subject contracts. Formal results are model-only; no Lean-to-Python or
   Rust/OS proof is claimed. Product behavior is unchanged, so broad unrelated
   product tests and mandatory CI targets are unnecessary for this evaluation.

## Software design philosophy assessment

Final score: **10/10 within the evaluation's fixed scope (8/8 diagnostic rows)**.
No failed rows remain. This is a design judgment about the research artifact,
not an empirical benefit score or an evaluation of the entire hoimin repository.

| Diagnostic | Evidence |
| --- | --- |
| One-sentence module responsibilities | Subjects supply behavior, tests supply independent requirements, evaluator runs CLI comparisons, audit checks evidence, model runner checks proofs. |
| Interfaces simpler than implementation | One fixed experiment invocation hides discovery, selection checks, resource settings and saved evidence; no framework configuration API is exposed. |
| Implementations can change locally | Test input semantics belong to the subject tests/corpus; the evaluator does not implement the cache or retry algorithm. Model changes do not affect the product. |
| Interface comments explain promises | Module docstrings distinguish primary CLI observations, diagnostic replay and model-only proofs; requirements precede subject code. |
| Reviews include design | Every adoption decision considers the added public concepts and maintenance cost, not just killed-mutant counts. |
| Important knowledge has an owner | The evaluator owns budgets and selection; the audit owns admissible outcome rules; subject tests own the specification. No new product protocol duplicates these. |
| Boundaries can be understood without implementation | README, design and module docstrings identify inputs, outputs, assumptions and the order of reproduction. |
| Design improvement receives deliberate effort | A pre-execution design, alternatives, plan and five-pass reviews were completed; findings removed selection contamination and an incorrect binary diagnosis before final adoption recommendations. |

The two initial missing rows (interface comments and discoverable boundaries)
are addressed by the design document and explicit module contracts. Shared saved
evidence is intentional within this fixed experiment; it is not promoted as a
versioned public schema. If multiple future evaluations need the same protocol,
reconsider its ownership rather than copying a large generic framework now.

## Documentation assessment

`technical-documentation` score: **10/10**, no failed diagnostic rows after fixing
the relative reference to the prior propagation review. The report has one reader
and decision task, separates empirical findings from borrowed research, supplies
verified commands and source links, and names the limits of timing and proof.
