# Correspondence worksheet — before model construction

Source: [frozen Issue #1172](specification.md), SHA-256 `94f1d12ee069e0f39cc6c6a8c42728968ab7dfa3a5711549ced219a07a8469e7`.

| Premise / observation | Planned Lean representation | Production configuration | Public observation | Evidence | Mode |
| --- | --- | --- | --- | --- | --- |
| Explicit selected/kept definitions | Lists of unique definition IDs and half-open source spans | `reduce` absent | None yet | Q5/Q10/Q16/Q25 | model-only |
| Fixed selected mutants, positive original kill | Unique IDs plus finite outcome map | `reduce` absent | Future JSON results | Q13/Q14/Q17 | model-only |
| Same command/plan/environment/variant/timeout | Opaque identity tuple on evidence and proposal | `reduce` absent | Future provenance manifest | Q19/Q23 | model-only |
| Candidate ordinary baseline succeeds | Boolean observation supplied by future runner | `reduce` absent | Future baseline output | Q12 | model-only |
| Three exploration timings | Triple of arbitrary natural duration ticks; median | `reduce` absent | Future timing records | Q11 | model-only |
| Three final paired speedups | Three ordered original/reduced pairs, rational threshold | `reduce` absent | Future timing records | Q18 | model-only |
| Certification before deadline | Monotonic logical completion tick, strict deadline boundary | `reduce` absent | Future lifecycle events | Q20/Q22; equality boundary is accepted in clarifications.md | model-only |
| Only certified best can produce diff | Optional certified proposal in transition state | `reduce` absent | Future diff plus JSON | Q9/Q19/Q22 | model-only |
| Definition erasure preserves kept code | Half-open interval nonintersection | No erasure implementation | Future parsed source and byte diff | Q5/Q10; AST/encoding excluded here | model-only |
| No same-premise adapter | No substitute `run`/`verify` execution | New entry point not implemented | Not available | Current CLI and Issue state | model-only |

No case is `strict` or `internal-fixture`. Tool setup failures, deadlines or failed corpus generation are `infrastructure-error`, not a production mismatch. Future adapters must construct the same premises before promotion.

## Boundaries and assumptions

Observed durations, baseline results and mutant outcomes are inputs, not predicted executions. Identity equality assumes collision-free handles bound to the actual bytes and command; hash collisions, concurrent external mutation and dishonest runner records are outside the model. Natural arithmetic is exact; future bounded machine arithmetic must prevent overflow. AST selection, Python execution semantics, cleanup, OS termination latency and physical elapsed time are not proved. Threshold validity, observed zero times and deadline equality are accepted supplements recorded in clarifications.md. The initial specification.md is preserved; specification-current.md captures the updated Issue text.
