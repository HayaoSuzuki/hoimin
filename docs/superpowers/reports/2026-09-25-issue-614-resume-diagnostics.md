# Issue 614 verification and reviews

Initial design and implementation plan, each with three reviews, were committed as84205bf before production changes. Independent review subsequently corrected the schema-version assumption and concurrent-history classification; revised design/plan reviews document both changes.

## Implementation self-reviews

1. Selection and side effects: retained candidate SQL, newest compatible incomplete ordering, budget decoding, old-schema error, ownership acquisition, conditional update and successful reuse. Diagnostics add no query to the successful-resume path. No-candidate history uses one SELECT with EXISTS predicates and existing fingerprint index, with no history materialization or config diff reconstruction. The report stores an outcome only when resume was requested.
2. Public contract: independent review found the closed RunStarted schema prohibited the new field. Corrected report version3→4, archived both schema3 contracts, preserved historical fixtures, generated new v4 fixtures through typed ReportHandler, and extended progress to historical3/current4 with matching versions throughout each document/stream. New metadata is rejected under historical versions. Config remains opaque to progress; schema2 keeps its existing isolated adapter.
3. Concurrent truth: independent review found a matching eligible row can appear between the original candidate query and history query. Added an eligible-history predicate with highest-priority candidate_changed rather than claiming fingerprint mismatch. The existing conditional-update failure also reports candidate_changed. No retry, lock, eligibility or ownership policy changed. Reasons are observations, not an atomic history snapshot across both queries or a detailed source/config explanation.

## Test self-reviews

1. Public execution: initial RED against an existing CLI showed no resume field although exit4 and a real run were produced. New subprocess tests cover no history, actual reuse/run-ID preservation/null reused termination, changed-source mismatch/new run ID/reexecution, complete match, reduced budget, no-resume omission and human/JSON/JSONL. Controlled Python, unique child-only temp roots and30s kill-on-drop deadlines prevent global environment mutation and unbounded tests.
2. Schema and compatibility discrimination: real fresh/resumed CLI outputs validate the closed schema4 in JSON and every JSONL event. New goldens are semantically regenerated, old v3 goldens remain byte-preserved and validate archived schema. Progress tests read old3/new4, retain schema2 tests, reject mixed document/event/diagnostic versions, and preserve opaque configs. During revised verification, closed ProgressRunStarted initially rejected resume; added its typed optional field rather than weakening unknown-field validation.
3. History/model correspondence: Lean defines newest eligible selection and explicit no-candidate precedence.64 strict cases execute real SessionHandler SQLite history with both insertion orders;17 concurrency observations are explicitly model-only, not a claim of deterministic OS scheduling. Direct production-classifier unit cases cover an eligible row appearing after candidate-none. Existing schema/corruption/ownership/contention/budget/reuse tests remain enabled. Model proves finite policy properties, not SQL isolation, digest injectivity or filesystem ownership.

## Independent review

batch_verify_606 found two actionable issues (closed JSON schema and between-query history change), both corrected and read-only re-reviewed with no additional finding. Parent reviewed the Lean model and real SQLite adapter independently; category-domain guards were strengthened without recomputing expected reasons in Rust.

## Verification

- Initial focused production:65 passed/1 ignored (public3, report27, session35); core116 passed.
- Revised schema/compatibility focused:155 passed/1 ignored (public3, report29, progress88, session35).
- Pre-review full run was superseded during contract changes; its output is not used as final evidence.
- Clippy findings corrected: human unit statement semicolon; version validation moved to its dedicated function to avoid101-line structure function; independent SQL facts grouped into one tuple. Corpus-only boolean schema has a targeted lint rationale.
- Lean guarded model/native/generate/check/sensitivity/stats succeeded under20 seconds/2GiB, proof heartbeat10000. Corpus81 (64 strict/17 model-only). Workflow registry40 tests passed.
- Final full workspace, latest-main integration and static gates recorded below after completion.


Final schema review by batch_verify_606 confirmed archived v3 schema definitions are unchanged except IDs/local reference names; current v4 enum/code definitions match Rust, mixed JSONL diagnostic versions are checked, and valid old2/3 data remains supported. Added its optional archived-result-schema check. Root also required present resume metadata to be non-null, matching the closed schema; missing metadata retains the default. Added old2/3-field and null negative tests. This is a format boundary check, not a semantic expected-result reconstruction.

Full workspace before the final narrow schema-input/guard refinements: 2401 passed, zero failed, 22 ignored across 111 groups. Final targeted/static checks follow.

Final pre-integration checks: targeted report/progress/oracle/public tests124 passed; schema reference helper extraction rechecked all29 report tests. Both exact CI Clippy commands, both formatting commands and diff check passed. Workflow registry40 Python tests passed. Final tests additionally preserve archived v3 result-schema validation and reject explicit null resume metadata without conflating it with an omitted field. Full Lean reproduction/review details are in the companion `2026-09-25-issue-614-lean-oracle.md` report.


## Latest-main integration

Rebased the two unpublished Issue614 commits onto c684749, excluding parent624 at2cf6bad. Resolved only append conflicts in progress tests and four Lean registries, retaining both main's regression/environment coverage and this Issue's schema/diagnostic coverage. No production conflict required changing selection semantics. Current environment fingerprinting and schema11 SQLite tests are included in final integration tests.

Integrated tests: 170 passed/1 ignored, exact workspace Clippy passed, workflow registry40 passed, formatting/diff checks passed. Guarded Lean corpus freshness passed again after rebase. Vendored parser remains unchanged from the previously passing exact vendor gate.
