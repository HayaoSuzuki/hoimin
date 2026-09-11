# Issue 487: Report the selected resource backend throughout a run

Issue: https://github.com/tokyogas-tech/hoimin/issues/487

## Failure and contract

The selected backend already knows whether enforcement is hard or best effort. Actual process results carry that mode, but RunStarted::minimal invents hard with an empty mechanism, and synthetic reused/stopped mutant output hardcodes hard. A best-effort run therefore reports contradictory resource metadata. Report the selected backend's actual mode and stable mechanism consistently in the run header and synthetic outcomes, while preserving the actual process observations already supplied by execution.

ResourceControl contains the singular fields mode and mechanism. Do not derive mode from the user's allow-best-effort permission: that flag permits a fallback and does not identify the backend that was selected. Do not turn a platform diagnostic sentence into a stable mechanism identifier. The ResourceBackend variant and its mode method provide the relevant provenance.

## Data flow and scope

Establish one typed backend description at the existing initialization boundary after backend selection and before StartRequested. Carry it into RunState so core emits a truthful RunStarted and synthetic MutantFinished for reused, cancelled and not-started candidates. Cover All, Explicit and Ordered candidate selection constructors. Prefer the narrow constructor/builder change justified by existing API contracts; the behavior of any public constructor without backend information must be deliberate rather than silently inventing provenance.

Do not patch only serialized JSON while leaving core output events contradictory. No new asynchronous phase or filesystem operation is needed just to carry an already-selected description. Keep resource preparation, backend cleanup, ownership, deadlines and actual enforcement unchanged. The description says which backend was selected, not that this patch proves OS enforcement limits.

For a resumed run, synthetic reuse belongs to the current run's selected policy. It does not claim the reused mutant was executed again. Preserve termination/output absence and elapsed time zero for synthetic records, existing reuse eligibility, status/counts and historical session records. Actual newly executed process results continue to carry their observed mode. Do not rewrite stored history merely to make current output appear consistent.

Stable mechanism names should identify the actual portable, Linux hard or Windows backend without asserting unsupported aggregate limits. The independent #488 work addresses the Windows limit scope separately. This branch starts4adf809 and does not assume #472 session artifacts or other reporting changes.

## Verification

Capture public JSON and JSONL RED on real best-effort execution, showing inconsistent header/synthetic fields while baseline and actual mutant mode is best_effort. After the fix, assert nonempty stable mechanism and consistent current policy across the run header, baseline and actual/synthetic mutant records. Exercise fresh runs and actual natural-incomplete resume, with the session database outside the project because this independent base lacks #472.

Use at least one reusable killed result and one nonreusable timeout so resume proves both synthetic reuse and real reexecution. Verify unchanged candidate IDs/statuses, null synthetic termination/output, zero synthetic elapsed time, and historical stored provenance. Add core controls for cancelled/not-started synthetic output and all selection paths. A hard-mode control must use actual available native capability or be explicitly classified as a description/transition unit test; local macOS portable execution is not evidence of native hard enforcement.

A bounded Lean transition/oracle extension may add useful evidence that an input policy survives fresh/reused/stopped output construction. It must compare derived expectations with actual Rust events and state the supplied-backend premise. It cannot prove the OS enforced the advertised limit. Any Lean run uses the existing serial30second/2048MiB/250ms guard, -j1 and -DElab.async=false. Generator/CI changes require exact Python workflow inventory registration and tests/test_ci_workflow.py.

Run focused resource/report/resume/core tests, full workspace all features, fmt and all-target/all-feature Clippy. Preserve current schemas unless an actual format change requires deliberate versioning; populating an existing field alone is not a new schema shape.

## Design self-review

1. Traced backend.mode, actual process mode, RunStarted::minimal and synthetic_finished_output. Distinguished configuration permission from selected enforcement provenance.
2. Located the existing backend-before-machine initialization and all three candidate-selection constructor paths. Core should emit the same policy rather than relying on a serialization-only correction.
3. Require real fresh/resume JSON+JSONL observations, synthetic-versus-executed field controls, preserved history and explicit hard-native limits. Keep Windows aggregate-scope correction independent.

## Constructor decision after call-site trace

Require the existing ResourceControl value in all four RunState constructors (including with_fingerprint) and RunStarted::minimal. ResourceBackend::resource_control maps the actual variant and mode to stable portable/linux_cgroup_v2/windows_job_object names; ProcessHandler exposes that selected description. This eliminates an omitted-policy state without an optional builder, fake hard fallback or new unknown report variant.

Controller approved the explicit source-API change after tracing about62 repository call sites, mostly tests. Cost: repository and external Rust callers must now supply a description; the serialized JSON shape and enforcement behavior remain unchanged. Core transition tests supply an explicit policy and are not proof that a native backend enforces it. This is a material API choice and must be disclosed in PR/final reporting. No constructor-compatibility waiver is hidden in default test helpers.

Three refinement reviews:1 checked every constructor and minimal output constructor and the shell's backend-before-state boundary;2 compared required input with optional builder/missing-policy failure and rejected fabricated provenance;3 separated source-API migration from schema compatibility and required both-mode core controls plus allselection paths. No Lean extension: actual Rust transition observations and real JSON/JSONL fresh/resume evidence cover policy propagation directly.

## Resume history observation refinement

The resumed run retains its existing run_id. Reexecuted timeout results legitimately replace their persisted row under the current session contract, so unchanged-history assertions target the reused killed candidate's stored record, keyed by candidate ID. Do not require every row to remain byte-identical after actual reexecution. The defect is fabricated current output provenance and not ordinary session persistence. Three checks: trace lookup/reexecution versus reuse; compare actual stored killed row before/after; separately observe the new timeout result and current policy.
