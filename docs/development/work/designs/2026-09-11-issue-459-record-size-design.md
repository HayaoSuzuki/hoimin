# Issue #459: Executable candidate record limits

## Contract and cause

Every generated plan candidate must fit the existing 2,097,152-byte spool record limit, including compact JSON escaping, UTF-8 and one final newline. Plans containing oversized candidates fail explicitly; no candidate is silently discarded. Saved oversized manifests fail before source rediscovery or baseline. Direct run rejects the candidate during its existing post-baseline analysis phase and reports incomplete infrastructure failure.

Only CandidateStore::push currently checks this limit. Plan discovery collects candidates in memory without using the spool encoder's size rule, allowing plans that cannot execute.

## Design

Extract the existing counting serialization into crate-private `CandidateStore::record_size(&MutationCandidate) -> Result<usize, StoreError>`. It performs no payload allocation and returns the newline-inclusive size after validation. Store::push uses this size for its single record allocation and write. Plan discovery calls it before retaining each converted candidate; plan header validation applies it to the flattened MutationCandidate, excluding ranking-only fields. Add candidate path, line and operator context at analyzer and plan error boundaries.

The direct run retains one counting pass and one encoding pass per record. Plan creation may validate again at the existing completed-manifest boundary, maintaining the rule that generated manifests satisfy their reader's checks. Keep fixed bounded record allocation and do not increase or remove the limit.

A pre-baseline discovery pass for direct run would duplicate analysis and change scheduler/timeout ordering; this Issue does not add one. Plan and verify provide pre-execution validation, while direct run rejects at its first candidate conversion/spool opportunity. Earlier candidates in a failed target discovery are not reported as a completed run.

Sequence fields remain part of serialized size. Verification rediscovery preserves ordered target/candidate discovery and omits unrequested source targets, so retained candidate sequence widths cannot exceed the original all-target discovery width. The store still enforces the actual encoded record at write time.

## Verification and limits

Reuse exact spool boundary tests and compare the shared size result with independent serde_json::to_vec length plus one for escaped/Unicode payloads. CLI tests cover valid below-limit plans and oversized generation, actual old-style oversized manifests, pre-baseline verify rejection and direct-run failure with a path/operator diagnostic. No abstract Lean model is needed to measure serde's actual encoding; independent encoded bytes and real CLI paths are the oracle.

## Design self-review

1. Contract: record size includes the newline and all candidate fields, excluding plan ranking metadata; error is explicit and bounded allocation remains.
2. Lifecycle: plan generation and manifest loading reject early. Direct run's existing baseline-first order is a documented limit, not a claimed preflight guarantee.
3. Integration: counting logic is extracted once, reused by store/discovery/header; actual writer validation remains authoritative for sequence and future field changes.
