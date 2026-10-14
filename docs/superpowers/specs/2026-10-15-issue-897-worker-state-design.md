# Worker-owned execution state

Issue: #897. The concrete Issue design was reviewed and approved for implementation by the user. This document records its implementation boundary; no new workflow approval is required.

## Problem and boundary

WorkerPhase and independently optional candidate/result allow invalid combinations. Each transition synchronizes several fields, and stop recovery reconstructs which data should exist. Replace those fields with one WorkerState enum. Preserve the RunState orchestration, effect identity validation and lifecycle behavior. Base this branch on #895 so worker result states own the typed mutation outcome without a second migration.

## Ownership

Idle, Reading and Resetting hold no candidate or result. Ready, Lookup, Applying, Starting and Running own a MutationCandidate. Ready explicitly represents the short interval after discovery before dispatch. Persisting and Finishing own a whole MutantResult; the candidate is part of that result and is not duplicated. SyntheticStarting and SyntheticFinishing own their candidate and synthetic classification. Every transition replaces one state value.

Resetting releases result ownership: its final output has already been acknowledged and summary accounting completed. Workspace restoration state belongs to the shell, not to an obsolete candidate/result retained in core.

## Stop behavior

Lookup/Applying/Starting recover a not-started candidate. Running recovers a started but unclassified NotRun outcome. Persisting recovers the classified result and accounts it once; Finishing recovers the same result without counting again. Synthetic states preserve start/finish balancing and synthetic status. Idle/Reading/Resetting do not invent a candidate. Preserve existing sorting, pending-effect retirement and output acknowledgement ordering.

## Validation and design assessment

Use existing adversarial property-based transition tests and lifecycle oracles for public behavior. Add focused AAA tests only for ownership or ordering gaps: one invariant per function, prefer arbitrary worker/candidate/status data over repeated rows. A Rust compiler rejection is appropriate evidence for removing invalid representable combinations; do not claim a behavioral RED where none exists. Keep runtime checks for genuinely wrong phases/effect identities, remove only checks that verify phase/Option synchronization.

Current scoped design score: 5/10, with interface simplicity, change locality and explicit boundaries failing. Target: 10/10 within the Worker state boundary by making data ownership explicit, eliminating synchronization knowledge, keeping orchestration centralized, documenting lifecycle intent and retaining generated trace checks. This does not score the whole repository.

## Five design self-reviews

1. State inventory: checked every WorkerPhase and its candidate/result invariant, including transient Idle+candidate assignments; Ready makes that valid interval explicit.
2. Ownership: Persisting/Finishing contain one result, not an independently copied candidate. Resetting requires no result for later processing.
3. Recovery: compared every stop branch and summary record point, especially Persisting vs Finishing, to avoid lost or duplicated accounting.
4. Interface depth: keep one enum and local access/stop helpers; do not introduce one class per phase or a RunState-wide type-state framework.
5. Boundary and compatibility: this is private state. Pending/retired completion checks and public schemas remain owned by existing modules; #895 owns any result wire changes.

Checkpoint assessment: the seven structural diagnostics pass (8.75/10 equivalent). Design-time allocation has not been measured; recording that ratio is the remaining action needed to substantiate 10/10. This is a scoped Worker-boundary assessment.
