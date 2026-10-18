# PR #981 CI repair — independent review

Reviewed base `111244cf1f75b215e04580e38fbba797995f77cc`, the uncommitted snapshot `/tmp/hoimin-pr981-ci/fix-review.diff`, current affected files, and recorded verification evidence. Review followed the supplied `requesting-code-review/code-reviewer.md` rubric. The checkout, index, HEAD, and branch were not modified; no Cargo commands, passed suites, or subagents were run.

## Strengths

- `deny.toml:23` and `deny.toml:28` move only the named `hoimin-core`/`hoimin-cli` exceptions to `>=0.12.0, <0.13.0`. An independent parsed TOML comparison against base confirmed every other value is identical, including global licenses, third-party exceptions, dev/build scope, and registry/git restrictions. Workspace version/license inheritance confirms these are the affected repository crates. Both saved real policy command receipts show exit 4 before and exit 0 after, using both locked, all-feature graphs without target filtering; their logs identify the own-crate rejection and subsequent `licenses ok, sources ok`.
- `docs/performance/shapes.json:504` and `:519` register the existing complete and truncated functions separately, retaining `--exact`, active status, and the operation metric. An independent parsed JSON comparison confirmed the single old gate is replaced by exactly these two gates, with every other field unchanged.
- The Rust adapter and Lean corpus remain unchanged. Inspection confirms the complete case requires two target reads, sequences `[1, 2]`, an untruncated result, and a finished spool; the truncated non-final case requires one target read, sequence `[1]`, truncation, and a finished spool. Both wrappers select their separate existing case through the shared assertion helper.
- The existing runner still checks subprocess failure and rejects zero passing tests. Existing tests cover zero-match, ignored-only, and failing output. The captured original gate log has zero tests/five filtered out and its artifact records the expected infrastructure error. Independently compared all 35 current active IDs and the current registry SHA256 against `gates-green/result.json`; each corresponding log contains exactly one passed, zero failed, zero ignored test.
- Current policy/performance guides describe the repaired configuration. Removing the stale current gate count avoids a second current count authority. Independently checked all four changed OKF records: content hashes match their actual source bytes, comparison revision is the supplied base, modified state is explicit, and each source exists at that revision. Historical execution statements remain unchanged.
- Evidence is candid about scope and failure: recorded pytest output has 116 passes, schema output validates 29 shapes, and scoped OKF output retains 86 baseline findings while recording no-new-findings success. `checks.json` records the corrected site command's exit 0 and the initial external-output invocation's exit 1; the initial log identifies the unchanged output-path guard. The supplied Linux job advances through workspace/CLI-reference checks before failing at the old performance filter. The maintenance note does not attribute a new full-suite run or new-head CI success to this repair.

## Issues

### Critical

None found.

### Important

None found.

### Minor

None found.

## Recommendations

Proceed with the executor's remaining completion review, receipt/report archival, commit/push, and new-head remote CI verification. Those steps remain executor responsibilities and are not outcomes of this review.

## Declined to judge

- Broader resource-safety implementation in the original PR: outside this narrow configuration/documentation repair and unchanged relative to the supplied base.
- Fresh all-workspace or cross-platform execution: explicitly excluded from reruns; reviewed the supplied earlier Linux evidence and current targeted receipts instead.
- Lean generator/proof soundness and corpus regeneration: unchanged here; inspected the two retained adapter projections rather than conducting a new formal audit.
- Whole-CLI memory bounds, native-stack bounds, release timing/RSS, and universal performance guarantees: not established by these deterministic gates and not claimed by the updated guide.
- Existing 86 OKF catalog/reference findings and historical audit counts: retained baseline or explicitly historical evidence, with no expansion required by this repair.
- Third-party dependency legal/supply-chain certification and independent re-download of the cargo-deny release archive: no dependency or tool-policy change; verified the narrow policy and recorded command evidence without a new external audit.
- Receipt/report copies added after the review snapshot: inspected `checks.json`; archival completeness and later completion passes remain with the executor. The implementation verdict applies to the reviewed repair snapshot.
- Future commit/push effects and remote CI at a new head: not yet evaluated by this review; require the executor's subsequent observations.

## Assessment

**Spec readiness: Pass.** Both diagnosed causes are addressed within the requested bounds, including retention of both terminal cases and zero-match rejection.

**Quality readiness: Pass.** No severity findings or unjustified deviations found. The reviewed repair is ready for the executor's remaining completion and remote-CI workflow.

**Ready to merge? Yes, within this narrow code-review scope.** Overall integration readiness remains subject to the executor's new-head remote-CI assessment.
