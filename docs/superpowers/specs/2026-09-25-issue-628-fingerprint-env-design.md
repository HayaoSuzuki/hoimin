# Issue 628: explicitly tracked inherited environment

Add repeatable `--fingerprint-env NAME` to the mutation arguments shared by `run` and `plan`. `verify` uses the names and digest persisted in the plan, checks the current inherited environment before any baseline, and rejects a changed snapshot. This feature adds an explicit compatibility input; unselected environment changes keep their existing behavior.

## Public contract

Names must match the portable ASCII identifier grammar `[A-Za-z_][A-Za-z0-9_]*`. Reject empty, `=`, NUL, non-ASCII, and other names before project work. On Unix names are case-sensitive; on Windows normalize ASCII names to uppercase to match the OS's case-insensitive lookup. Sort and deduplicate names, so repeats and argument order cannot change compatibility. This initial name restriction avoids approximating Windows Unicode case mapping. Values retain arbitrary OS-native bytes on Unix and UTF-16 code units on Windows, including non-Unicode values.

Observe the inherited process values during command preparation, before hoimin adds worker paths or metadata. In particular, tracking `PYTHONPATH` tracks its inherited value, not a generated worker path. External processes cannot mutate the parent environment; concurrent in-process environment mutation by an embedding caller is outside this snapshot contract. No change to worker environment construction is required.

For each selected name, distinguish absent from present-empty and present-nonempty. Compute a domain-separated BLAKE3 digest over a version/platform tag, entry count, framed canonical name, presence tag, and framed native value. Length prefixes prevent cross-entry/name/value concatenation ambiguity. Hash incrementally; do not build or persist a plaintext environment snapshot.

Persist only the canonical selected names and the aggregate digest. Add defaulted `fingerprint_env` (empty vector) and optional `fingerprint_env_hash` to normalized run/plan configuration, omitting empty/absent fields when serializing. Do not add captured values to config, diagnostics, reports, the SQLite session, or manifests. A digest does not promise secrecy for low-entropy values; the guarantee is that this feature does not add their plaintext to output/storage. Test commands can independently print their own environment, as before.

## Configuration, fingerprint, and manifest boundaries

Normalize names in `RawRunConfig` conversion and validate canonical names in normalized config. Raw run configuration may await capture; prepared run configuration carries the digest. Persisted `PlanConfig` requires a valid lowercase 64-hex digest exactly when names are nonempty; empty names with a digest, missing digest with selected names, noncanonical names, and malformed digest are invalid. A verify mismatch is an exit-2 stale-plan error, without printing either the captured value or digest. Verify does not accept an override list that could weaken the manifest's declared inputs.

Carry the canonical names and digest into `FingerprintInput` and frame them under new tag 13. Increment fingerprint schema 10 to 11, retaining the existing session policy for older fingerprints and the monotone budget behavior from Issue624. If no compatible run exists and the latest incomplete run has an older fingerprint schema, preserve `session.resume.incompatible` and its instruction to start a new session; do not silently reinterpret that error as a fresh run. Name-list changes, selected variable presence/value changes, and platform encoding changes alter the digest/fingerprint; unrelated variables do not.

Historical `RunConfig` and `PlanConfig` values lacking the new optional fields deserialize as untracked. The run-event schema explicitly permits additive normalized-config fields, so its public version stays unchanged. The plan manifest's serialized contract changes; increment plan schema 4 to 5 and require regeneration of prior manifests through the existing version check. Ranking rule version remains 4. New schema-5 plans with no selected variables omit the added fields and remain valid.

## Implementation shape

- Add CLI plumbing and core normalized-name/digest validation, with no environment reads in hoimin-core.
- Add a small CLI `fingerprint_env` module with an injected native-value reader for deterministic unit tests and a production wrapper over `std::env::var_os`.
- Validate names before OS lookup even for direct library callers, then capture alongside existing file fingerprint inputs in `prepare_run_config`. Capture and compare declared environment inputs in plan verification before project execution/baseline; preserve the verified digest when entering the prepared run path.
- Extend the existing run fingerprint encoding and version; no SQLite schema or session selection changes.

## Validation and model boundary

Public subprocess tests use `Command::env`/`env_remove`, never global unsafe environment mutation. Reproduce strict `1` to weak `0`: the tracked resume starts a fresh run and observes survived, while a fresh control agrees. Test equal values reusing the run/result, untracked changes reusing as before, absent versus empty, selected-name addition/removal/replacement, name order/duplicates, native non-Unicode values, Unix case distinction/Windows canonicalization, and no captured plaintext in report/diagnostic/session/manifest artifacts. Plan/verify tests prove equal capture verifies and changed capture fails before a marker-writing baseline; malformed persisted fields fail before work. Existing budgets/status/exit behavior must stay intact.

Add a bounded Lean model and versioned generated corpus for the 32 combinations of tracking on/off and before/after values absent, empty, one, zero. Prove tracked reuse agrees with fresh semantics and absent/empty remain distinct; include an always-flattened broken encoding witness and observe RED before implementation. The strict Rust consumer runs the actual CLI with isolated SQLite sessions and compares generated status, reuse/run ID, execution count, termination, baseline, completeness, and exit expectations. Hash collision resistance, arbitrary byte encoding, Windows API behavior, timing, and concurrent mutation are outside the finite model and are covered only to their stated Rust test boundaries.

Use 10,000 heartbeats per new proof, one global Lean slot, and the 20-second/2-GiB guard. Add the model/native entry/corpus/sensitivity commands to the existing closed CI registry in lakefile order.

## Design self-reviews

1. Compatibility and scope: run-only capture would leave plans blind to a declared input. Include plan/verify with persisted names plus digest, retain budget624 selection, and reject missing/detached plan digests rather than silently refreshing them.
2. Native encoding and privacy: string conversion would conflate invalid bytes; absent-as-empty would incorrectly reuse. Use native tagged framing and an injected reader, never serialize values. Explicit portable name grammar gives exact supported Windows case semantics without an unverified Unicode approximation; do not describe hashing as encryption.
3. Persistence and versioning: legacy missing fields must mean untracked, but new plan semantics require a clear version boundary. Default/omit optional config fields, retain the additive report contract, increment plan and fingerprint versions, and require canonical names/digest consistency before baseline. Keep worker-generated paths out of captured input.

Independent design/plan review by the parent agent found no blockers. Follow-up implementation checks: preserve older-session error policy, inspect current SQLite schema4 fixture fingerprint metadata/version assertions, and update plan-schema fixtures consistently while retaining historical database fixtures.
