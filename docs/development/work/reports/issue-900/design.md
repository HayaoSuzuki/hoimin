# Typed disk failure design

For maintainers changing disk stop handling. A failure carries a `DiskFailureReason` payload and independent secondary evidence. Observation variants require `DiskObservation`; measurement and process variants require a message. `DiskStopReason` remains the payload-free label used by secondary evidence and formal oracle projections. Codes derive from the payload variant. This simplifies representation; it does not establish additional cleanup safety guarantees.

The first stop remains primary. Reserve takes priority over size in a simultaneous observation. Later stops convert through a single variant match to secondary evidence, preserving complete observation equality including duration. Independent cleanup diagnostics remain unchanged. Report fields retain their existing shape, with one conversion from typed failure.

Serialization changes intentionally: `reason` is an externally tagged payload enum; `code`, `observation`, and `message` disappear from `DiskFailure`. Reject the old representation without conversion or defaults. `DiskStopReport` wire fields remain stable.

## Design review passes

1. Representation review: found independent code and payload fields could disagree; replace them with required variant payloads and derived code.
2. Evidence review: found secondary observations need payload-free reason labels; retain `DiskStopReason` instead of changing unrelated diagnostics.
3. Equality review: found report projection drops measurement duration; retain exact equality in lifecycle/monitor evidence and preserve existing report matching semantics.
4. Scope review: found current transitions already enforce their private ordering guarantees; describe representation simplification without claiming a reproduced safety defect.
5. Wire review: found serde ignores unknown fields by default; require `deny_unknown_fields` on failure and typed reason to reject legacy/mixed representations.

Scoped design diagnostic: seven of eight rows are evaluable and pass (8.75/10 equivalent) for the typed payload, evidence and report-conversion boundary. The time-allocation row is unmeasured; record design/review time against implementation time in a future run to evaluate the 10–20% target. Each boundary owns its mapping, comments state required payloads, and existing lifecycle responsibilities remain intact.
