# Issue 709: evidence before repeated verification

The user asks whether integrated repeated verification is worth implementing,
using the original paper before starting product implementation. This change is
a bounded decision experiment. No new runtime flag, status or report schema is
assumed necessary in advance. Use an isolated worktree; commit the assessment,
reproducible probe, observations and Lean model, then clean artifacts.

Observe existing saved-plan verify calls for a fixed candidate set. Controlled
commands cover consistently passing/failing mutants, alternating mutant results,
per-test alternation masked by another deterministic failure, and baseline failure.
Also repeat a small fixed candidate set against two installed Python packages using
explicitly authored checks. Record costs and observation boundaries; neither these
checks nor the small sample represent the projects' upstream test suites.

The decision is whether evidence supports the full integrated feature now, a
smaller audit first, or rejection of the premise. Absence of observed instability
in small samples cannot establish stability. A reproducible changed command-level
classification can justify a targeted diagnostic but not automatically a general
score correction. Resource outcomes and incomplete attempts are not ordinary kills.

## Design self-reviews

1. User intent: evaluate usefulness before product code; a no-go decision is an allowed outcome, not an implementation failure.
2. Observation unit: paper's individual tests differ from hoimin's command exit; include an explicit masking counterexample.
3. Sampling: test both killed and survived candidates, and separate deterministic controls from package probes.
4. Cost: finite attempts and wall deadlines; do not download the 9.5GB artifact or >130GB raw dataset.
5. Compatibility: exercise current verify without changing status, score, schema or session behavior; any integrated follow-up needs a separate contract.
