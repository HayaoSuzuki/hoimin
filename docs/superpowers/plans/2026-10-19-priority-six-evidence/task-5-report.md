# Task 5 (#1010): derived status and complete human result rows

2026-10-19. Implementation base: `6ea967d6b1682596d6a2c0dda3d1c458f9ce5e6a`, after Task 4's independent review gate. The controller approved `task-5-design.md` and R11. This report records Task 5 only; combined integration and native CI remain controller-owned.

## Delivered contract and reused APIs

`MutantFinished` now implements computed serialization and checked deserialization. A private borrowed output view computes required `status` using `self.outcome.status()`; a closed owned input view requires its seven-value `MutationStatus`, compares it with `outcome.status()`, then discards the received projection. The public struct's fields and existing construction sites are unchanged. Empty diagnostics retain their existing omitted/default behavior. No independent mutable status enters execution state or persistence.

The existing raw `OutputEvent` discriminator envelope remains intact. JSON spool records, final JSON `mutants[]`, and JSONL `mutant_finished` share this serde boundary. Direct typed decoding, progress JSON/JSONL, fingerprinted reads, and typed review receive the same consistency check. Invalid known-status disagreements identify both received and expected classifications and reach public exit 2. Outcome remains the source for summary, progress and resume.

Human completion rows now read `mutant finished: ID path:line:column operator "original" -> "replacement" status`. The renderer uses Task 2's `HumanText` Display for the full ID/path/operator and Debug for quoted expressions, and keeps `outcome.status().as_str()` for the verdict. Each row contains its own context under parallel completion.

Report/event schema remains the same unpublished **9** established by Task 4, with package floor **0.14.0**. The event schema requires status via its existing seven-value definition; the result schema inherits that requirement through its existing mutant-event reference. R11 makes Rust deserialization the semantic authority. Generic JSON Schema validates shape and enum membership but cannot establish status/outcome agreement; the schema description, usage, compatibility notice and consumer skills state that limit.

Updated only the four current schema-v9 original/current JSON/JSONL goldens, preserving historical v8 and older bytes. Existing valid fixtures that intentionally change outcome classification now update the projected status too. Two existing Lean reader adapters copy expected status into the new wire field, and one progress-detail heap fixture does the same. Lean models/corpora and their expectations were not changed.

Updated usage and compatibility guidance, both repository mutation consumer skills, and the existing session/report and JSON-spool OKF concepts. The usage status descriptions now account for output collection, reused results and cancellation after process start. New OKF source records identify this implementation base and modified-source hashes; historical verification metadata was preserved.

Plans remain schema 10, sessions 5 and fingerprints 13. Candidate IDs, execution classification, exit policy, configuration, Task 4 janitor evidence/projection, #999 Creusot production integration and #806 were not changed.

## Five requirements/design reviews

The detailed approved passes are in `task-5-design.md`; the following findings guided implementation:

1. **Canonical classifier:** confirmed close-timeout dominance includes `Exit(1)` and cancelled execution. Added literal complete/partial cases for every termination and corrected loose user guidance that every nonzero exit is killed or every not_run never started.
2. **Projection-only ownership:** traced completed and synthetic machine emissions to the same report type and disk spool/JSONL writers. Chose private serde views instead of public status plumbing or transport-specific wrappers; persistence needs no new field.
3. **Checked input:** retained raw discriminator decoding, closed fields, and duplicate-key evidence. Planned positive controls and missing-status RED so old blanket unknown-field rejection could not masquerade as semantic validation.
4. **Complete human row:** existing field escaping already met Task 2's policy; only ID was absent. Kept a single row render and canonical verdict, with reverse-completion and arbitrary-field tests.
5. **Consumer migration:** identified outcome-mutating raw fixtures and current/historical golden distinctions. R11 explicitly separates schema shape from semantic readers; Task 4 handoff confirmed required-nullable janitor and runtime presentation can remain intact.

## RED evidence and setup corrections

All logs are task-owned `/tmp/hoimin-priority-six-task5-*.log` files. Before product edits:

- `red-core.log`: **31 failures**, including absent serialized status (`Null` versus literal verdict), missing-status acceptance, and matching new-wire input rejection by the old closed decoder. The property shrank its missing projection to exit 0 with complete output. Explicit exit 1 + close-timeout expected `error` and observed absent status.
- `red-human.log`: **12 failures** in generated external-field cases; expected complete ID-bearing rows differed from the old ID-less output.
- `red-public.log`: **2 failures** in JSON/JSONL positive controls because the old reader rejected the new matching status field.
- `red-review.log`: **1 failure**, rejecting matching status as unknown before any tampering.
- `red-schema.log`: **1 failure / 15 passes**; generic schema incorrectly accepted a mutant event with complete outcome but no status.

After the implementation allowed those matching controls, the added public progress test exposed my test setup error: it supplied one report although the command requires two. The assertion on the mismatch cause kept this from passing merely on exit 2. Corrected it to two inputs; both actual executable cases pass. The new public review test similarly exposed its missing required `--store` option; adding a fresh store path reached the intended status mismatch. These were test setup corrections, not product-policy changes or silently accepted failures. The first Python formatter check required formatting the new test expressions; the final lint/format checks pass.

## Focused validation evidence

Cargo commands used `CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=/tmp/hoimin-priority-six-target`, with `--test-threads=2` and one Cargo operation at a time. Python used the existing frozen `.venv` pointing to `/tmp/hoimin-priority-six-venv`; no dependency installation occurred.

| Log | Command scope and observed result |
| --- | --- |
| `green-core.log` | Initial new report_status suite: 31 passed. |
| `green-core-suites.log` | `cargo test -p hoimin-core --test report_status --test report_policy --test resume_policy --test report_resume -- --test-threads=2`: report_status 31, report_policy 32, resume_policy 29, report_resume 4 passed. Includes tagged/direct round trips, every outcome, close-timeout dominance and tampering, raw equal/conflicting duplicate keys, null/unknown/wrong-type status and signed-exit property. |
| `green-outcome.log` | `cargo test -p hoimin-core --test mutation_outcome -- --test-threads=2`: 17 passed, including diagnostic independence and current-wire acceptance. |
| `green-cli.log` | Initial `report_handler`, `progress`, `review_cli`: 145 / 130 / 23 passed. |
| `green-final-cli.log` | After new public parallel/duplicate-reader cases and spool assertions: `cargo test -p hoimin-cli --test report_handler --test progress --test report_heap -- --test-threads=2`: 146 / 138 / 1 passed. Public resume checks both fresh executed and resumed killed provenance in JSON/JSONL. The new jobs-2 executable run emits two complete distinct full-ID human rows; deterministic reverse completion checks escaped expressions, and the arbitrary-text row property covers all external fields. |
| `green-review.log`, `green-review-control.log` | Focused `cargo test -p hoimin-cli --lib review:: -- --test-threads=2` passed 86 existing tests and exposed the new case's missing-store setup. After correcting that setup, `cargo test -p hoimin-cli --lib public_review_rejects_contradictory_status_after_matching_control -- --test-threads=2` passed the added case. No unresolved review failure remains. |
| `green-delivery.log` | `cargo test -p hoimin-cli --lib report::delivery:: -- --test-threads=2`: 6 passed, protecting owned delivery/cancellation/flush behavior. |
| `green-adapters.log` | `cargo test -p hoimin-cli --test lean_progress_input_oracle --test lean_progress_decision_oracle --test progress_details_heap -- --test-threads=2`: 4 / 5 / 1 passed. This reran existing Rust/CLI adapters against their recorded corpora; it is not a new Lean proof or native CI run. |
| `green-python.log` | Frozen Python `pytest -q` for mutation outcome, session mode, review reference, verification selection, changed selection, environment snapshot schemas and skills: **106 passed**. Uses `Draft202012Validator`; includes missing/noncanonical status, matching shape and the explicit R11 shape-only semantic limit. |
| `docs.log` | Safe YAML parsing of the two changed OKF concepts and two skill headers; concept type mappings, unique source IDs, new source hashes/footnotes and local link targets pass. Prose was read separately for scope and evidence claims. |
| `clippy.log` | `cargo clippy --workspace --all-targets --all-features -- -D warnings`: passed. This is the required lint scope, not a full integration-test claim. |

`cargo fmt --all`, `git diff --check`, and Ruff lint/format of the changed Python test pass. The mandatory commit hooks use the same Cargo resource environment; their outcome is captured in `commit.log`. The task-owned resource check observed 96 GiB available on `/tmp` before commit. No unrelated worktree/target cleanup was performed.

## Five implementation reviews

1. **All writer paths:** inspected the completed/synthetic machine projection and shared report serializers. Borrowed wire serialization computes status per record before spool `write_all`; final JSON copies those bytes. JSONL, public reuse and synthetic not-run use the same type. No writer-specific classifier or post-spool patch exists.
2. **Strict readers and duplicate data:** inspected the owned `Wire`'s required enum, `deny_unknown_fields`, comparison and move into the domain type. Kept the raw event envelope. Direct/tagged and JSON/JSONL file tests reject missing, noncanonical, duplicate-equal and duplicate-conflicting status; public progress and review reject semantic tampering.
3. **Summary/resume authority:** reviewed the diff and outcome consumers. The public result has no status field, and summary/progress/resume still call outcome classification. Existing conclusive reuse provenance remains under `MutationOutcome::Reused`; SQLite/session code is unchanged. Public resume tests inspect this distinction.
4. **Human row fields:** checked full ID, location, operator, quoted original/replacement and verdict in the one `writeln!`. All external strings use the existing `HumanText` field policy once. Reverse completion, controls and actual jobs-2 output provide independent context without matching starts.
5. **No legacy fallback or adjacent regression:** checked schema9 and four v9 fixture diffs, preserving v8 and earlier bytes. Required nullable janitor, handler presentation, checked configuration and release-floor files have no product changes here. Reader adapters were migrated directly without defaults or compatibility constructors; no Lean model, formal scope, new command or exit-policy change was introduced.

## Five validation reviews

1. **Close-timeout sensitivity:** literal exit-one/partial and the complete/partial termination matrix require `error` before termination-specific statuses. The initial absent-projection RED and final canonical outcome/status assertions avoid expectations computed by production status(). A tampered killed verdict on those error cases is rejected by tagged decoding.
2. **Every outcome and provenance:** runner-visible cases cover executed exit 0/nonzero/negative, timeout, OOM, process limit, cancelled, all their partial-output counterparts, reused killed/survived, and synthetic not-run. Signed-exit/property cases broaden the finite literals; public resume and no-output/zero-time outcome tests preserve synthetic meaning.
3. **AAA and independent cases:** new Rust macros expand runner-visible cases; properties use an independent exit/output oracle and arbitrary external text. Tests exercise serde/files/public commands rather than source text or implementation mocks. Existing consumer-skill structural tests do not establish agent pressure-test behavior; no subagents were used. Python edits affect tests only, so there is no changed production Python target requiring mutation verification.
4. **Actual public exit 2:** JSON and JSONL mismatch tests launch the real executable with valid-ID/required-janitor positive controls; review exercises the public command entry with a valid saved plan/configuration and store argument. All assert the status mismatch cause, preventing CLI-argument errors or unrelated malformed fixtures from satisfying the contract. Raw duplicate tests avoid `Value` normalization before reader decoding.
5. **Parallel readability and resource/evidence limits:** the jobs-2 executable test checks complete distinct rows without assuming completion order; a deterministic B-before-A fixture checks identity association and quoted controls. Row properties forbid embedded controls/separators. Focused spool/heap/delivery tests remain bounded. These are native macOS observations, not native Linux/Windows success or proof about arbitrary independent library writers. Controller R12/native manual CI remains outside this task.

## Handoff

No unresolved implementation ambiguity or deferred product issue remains. R11's deliberate cost remains: schema-only consumers cannot certify semantic status/outcome agreement. Task 6 consumes the same completed unpublished report/event schema 9 and floor 0.14.0, the unchanged runtime `ReportPresentation` APIs, and Task 2's `HumanText`. Internal Rust callers still construct `MutantFinished` with the same outcome-only fields; custom wire producers must now include matching status. The controller owns the next independent scoped review and combined integration/catalog/release consistency checks.

## Commit

Committed as `b2b011959cd41f7ad23bfd707079eb464dd61e39` (`feat(report): derive checked status and complete human result rows`). All four mandatory hooks passed: workspace/vendor formatting and workspace/vendor clippy. The commit used the required Cargo environment. Git status is clean after the commit. The SDD design/report remain local ignored controller artifacts, following the existing task-record layout.

## Fix round 1: independent review I1

The independent review found that the original `projection_case!` combined serialization, matching decode and contradictory decode, with decoding operations inside Assert. A projection failure could mask later acceptance/rejection evidence. This contradicted the binding single-behavior AAA requirement even though the product contract passed its checks.

Changed only `crates/hoimin-core/tests/report_status.rs`. Each of the 17 literal outcome cases now generates six independently scheduled tests: direct serialization, tagged serialization, direct matching decode, tagged matching decode, direct contradictory rejection, and tagged contradictory rejection. Each test has one target operation in Act. Decoder fixtures overwrite the projected field with the independent literal in Arrange; tagged fixtures add the discriminator without invoking the tagged serializer under test. Expected outcomes and the production classifier are unchanged.

The same file's equal/conflicting duplicate-key cases now schedule direct and tagged rejection separately, and its signed-exit property now separates projection from matching decode. The remaining missing/noncanonical/extra-field cases preserve their existing setup controls.

Changed-case accounting: 17 mixed outcome cases became 102 single-behavior cases (+85); two duplicate cases became four (+2); one combined property became two (+1). The other 11 cases are unchanged. The suite therefore grew from **31 to 119** cases without adding a production behavior or changing its outcome matrix.

Focused evidence: `CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=/tmp/hoimin-priority-six-target cargo test -p hoimin-core --test report_status -- --test-threads=2` passed **119/119**, logged in `/tmp/hoimin-priority-six-task5-fix1-report-status.log`. `cargo fmt --all` and `git diff --check` pass. This is a test-structure refactor; the original product RED evidence above remains historical evidence, and no new production RED run is claimed. No broad suite was repeated.

Review checks: confirmed one target Act per generated test; kept literals independent from `outcome.status()`; retained all 17 outcomes including close-timeout/cancelled/reused/not-run; kept positive and contradictory decoding independently runner-visible; and checked the diff contains only the test file, with production/classifier/schema/fixture bytes unchanged. The required commit hooks use the same resource environment and are recorded in `/tmp/hoimin-priority-six-task5-fix1-commit.log`.

Fix committed as `faff6415d6a54f71ad881df97e521cc83ac19894` (`test(report): separate status wire behavior cases`). All four mandatory hooks passed, and Git status is clean. Cargo ownership is released to the controller for the scoped follow-up review.
