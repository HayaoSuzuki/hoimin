# Issue 614: explain resume selection

Add optional structured `resume` to RunStarted, omitted when resume was not requested. Tagged outcome is `{"status":"resumed"}` or `{"status":"fresh","reason":"..."}`. Human output adds a line after run started: resumed existing run, or starting a new run plus stable code and explanation. JSON/JSONL retain their regular envelope, event sequence; emitted report schema advances from 3 to 4. Progress retains historical schema-2 and schema-3 readers; each document/stream must use one version consistently. Archive the closed schema-3 contracts and add schema-4 golden artifacts rather than rewriting historical fixtures.

Keep the existing SQL candidate query, schema validation, ownership acquisition, conditional budget update, and run selection unchanged. Extend SessionLoaded with an optional fresh reason (serde default for old events), then propagate that reason through RunState until the real RunStarted after BeginSession completes. Successfully owned candidate means resumed regardless of newer incompatible/completed history. No extra DB inspection on the successful path. The conditional-update race that loses eligibility reports `candidate_changed`; it remains a fresh run as before.

When the original query finds no candidate, first preserve existing old-schema errors. A single aggregate EXISTS query observes history, with precedence: same-fingerprint incomplete run above requested budget → `budget_decreased`; same-fingerprint complete run → `matching_run_complete`; any incomplete run → `fingerprint_mismatch`; any remaining completed history → `no_incomplete_run`; empty history → `no_prior_run`. The budget rule is required by #624. Coexisting matching completed and unrelated incomplete history reports matching_run_complete. Corrupt or unreadable session/ownership errors remain errors. Reasons describe observed history, not a detailed configuration diff or an atomic promise about future concurrent writes.

Old externally supplied SessionLoaded events without a reason fall back to `no_compatible_run`, avoiding invented specificity. Non-resume runs omit the field even if they use a session. Internal metadata is finite and contains no stored run/config secrets. Docs state reason precedence and preserve old-schema error behavior.

Lean will cover finite history facts and outcome precedence, using generated cases through real SQLite SessionHandler.load. It does not prove SQL engine isolation, digest injectivity, filesystem ownership, or concurrent scheduling. Existing ownership/schema/corruption tests remain required.

## Design self-reviews

1. Selection preservation: original query and ownership ordering are untouched. A successful old candidate always beats newer unmatched rows. Preserve the update-race branch and explicit old-schema errors.
2. Diagnostic truth: a matching complete row is stronger evidence than unrelated incomplete history; no-history differs from completed-other-history. Add budget_decreased because max-mutants now has a separate monotone eligibility constraint. Never claim which source/config field changed.
3. Output and compatibility: attach at actual RunStarted after session initialization, not as free text in JSON. Optional serde fields allow old reports and events; report sequence and exit semantics stay unchanged. No-resume omission keeps unrelated consumers stable.

## Independent-review corrections (before revised implementation)

1. Published RunStarted has a closed JSON Schema, so adding resume requires output schema4 and corresponding schemas/goldens. Earlier schema-unchanged assumption was incorrect. Current progress must accept historical3 and new4 consistently; mixed versions remain invalid.
2. Candidate-none and history inspection are separate snapshots. The history query also observes matching eligible rows; if one has appeared, use candidate_changed rather than falsely claiming fingerprint mismatch. This reason now describes history changing during selection as well as conditional-update eligibility loss.
3. Reviewed the revised boundaries: old schema files/fixtures remain immutable, new public output must validate the current closed schema, old3 progress retains opaque-config handling and exact sequence/summary validation. Matching eligible history takes precedence over all no-candidate reasons without retrying or altering selection.
