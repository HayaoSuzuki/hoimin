# MutationStatus canonical names (#857)

Base: `6c638da243865273c55886739e9bbf77803f8fcd`.
Branch: `refactor/mutation-status-names-857`.

The core model owns the seven existing names through `MutationStatus::as_str`
and `MutationStatus::from_name`. Human reports and session persistence call
those methods. The progress details formatter retains its killed/survived
guard and delegates within that branch. Serde derives, process termination
encoding, status classification, session replacement/reuse rules and schemas
retain their existing definitions.

## Design self-reviews

1. Located the full human/session name tables and both session decoder callers;
   assigned canonical names to the existing core enum.
2. Compared inherent methods with a shared trait, codec object and caller-local
   wrappers; selected two methods with no additional abstraction.
3. Checked all seven spellings against the existing tables and snake_case Serde
   representation; kept the Serde derive and tested its wire names independently.
4. Compared normal decoding with contracts-only decoding; retained each caller's
   error construction and the distinct messages `unknown mutation status` and
   `unknown status`.
5. Checked policy boundaries: the progress guard admits two statuses; reuse and
   replacement classify all seven elsewhere. Name conversion must not expand
   those policies or change process termination encoding.

## Planning self-reviews

1. Chose literal expectations for seven enum/name/JSON cases and noncanonical
   case, separator, whitespace, NUL and Unicode inputs; avoided helper-derived
   expected names.
2. Selected existing core report compatibility targets for the baseline, then
   planned a missing-API red run and focused green run before caller refactoring.
3. Planned consumer tests at real boundaries: human report lines, SQLite status
   cells, loaded enums and resume decisions, using existing fixture helpers.
4. Planned invalid-row checks for lookup and replacement, including complete
   corruption diagnostics and retained database contents; extended the existing
   progress details fixture to check human output in both directions.
5. Assigned focused CLI, workspace, contracts and Clippy verification to the
   coordinator after core checks, using the task resource guard and one active
   build across the two issue worktrees.

## Implementation self-reviews

1. Compared the new explicit matches with each former name table, including
   `out_of_memory`, `process_limit` and `not_run`; checked strict fallthrough.
2. Removed both full CLI output tables and the session decoder, including the
   obsolete human-report import; checked the contracts-only decoder call.
3. Inspected the remaining progress match: only killed/survived delegate and
   the original unreachable branch and message remain.
4. Reviewed the consumer tests against realistic mistakes: swapped name rows,
   case normalization, unknown-name fallback, changed corruption classification,
   attempted replacement of corrupt rows and changed reuse decisions.
5. Ran rustfmt on the eight changed Rust files and checked the diff; production
   edits affect name conversion only. No resource-mode or termination codec,
   status policy, schema, output format or default changes appear in the diff.

## Verification self-reviews and evidence

1. Read the baseline log: `cargo test --locked -p hoimin-core --test report_policy
   --test report_resume` passed 41 tests. The guard recorded exit 0, 12.4 seconds,
   peak process-group RSS 389,185,536 bytes and no resource stop.
2. Read the red log: the new `mutation_status_names` target failed with exit 101
   and three E0599 errors for the missing `as_str`/`from_name` methods. This is
   API-absence evidence; it is not a runtime assertion failure.
3. Read the green log: the new target plus the two baseline targets passed
   44 tests. The guard recorded exit 0, 4.0 seconds, peak process-group RSS
   430,473,216 bytes and no resource stop. Later edits added method attributes
   and CLI consumers; final-tree verification is recorded below.
4. Inspected final source diffs and literal test expectations; direct rustfmt
   completed and `git diff --check` passed. Existing replacement tests cover the
   two determinate and five inconclusive statuses. These are static reviews,
   not claims that the new CLI tests have executed.
5. Reviewed the remaining verification routes: `report_handler`, `session_handler`,
   `progress`, workspace tests, contracts-enabled tests, formatting and Clippy
   were assigned to the coordinator; final execution is recorded below.

Logs: `/tmp/hoimin-857-baseline.log`, `/tmp/hoimin-857-red.log` and
`/tmp/hoimin-857-green.log`, with corresponding `.summary` files. These are
local execution artifacts, not repository fixtures.

## Bounded design assessment

For canonical-name ownership only, the initial assessment was 4/8 diagnostic
rows, or 5/10. The failed rows and fixes were:

| Failed diagnostic | Fix |
| --- | --- |
| Change an implementation without affecting callers | Remove caller-owned full name tables and use enum methods |
| Interface comments describe the abstraction | Document canonical output and exact, unnormalized input |
| Each module hides a design decision | Put canonical-name knowledge in the enum's module |
| Newcomers understand boundaries without reading implementations | Give callers the same two documented methods; retain policy guards at their existing owners |

The final local assessment passes 8/8 rows, or 10/10: responsibilities fit one
sentence each, two methods cover the mapping, callers delegate name knowledge,
comments specify the contract, reviews discuss complexity, the enum owns names,
boundaries retain their policies, and the work invests in removing duplication.
This score covers this finite mapping and its callers, not the whole application
or the depth of its other modules. No failed local row remains after inspection;
runtime verification is recorded separately below.

## Formal methods and knowledge maintenance

Seven literal cases exhaust the enum. Negative fixtures cover exact-name
rejection; existing session and progress tests cover policy consumers. A new
Lean model or proptest generator would add machinery to a finite lookup without
addressing new state, concurrency or unbounded input behavior. Existing models
and generated corpora remain unchanged; the coordinator's workspace checks
exercise their Rust adapters.

Consulted `docs/development.md`, `docs/okf-workflow.md`, the knowledge index and
scope guide, and `docs/knowledge/design/session-report.md`, then compared the
session/report claims with current code. The OKF workflow exempts internal
cleanup that changes neither contracts nor evidence scope. This refactor keeps
session compatibility and historical audit guarantees unchanged, so it requires
no concept/index update or historical source-metadata refresh. This dated record
owns the local implementation and verification evidence.


## Final verification

`cargo test --locked --workspace --all-features` passed 2,825 tests with 29
ignored across 151 top-level summaries on macOS. This includes `report_handler`,
`session_handler`, `progress`, core status tests and all core/CLI contracts;
all newly added consumer tests passed. Nested subprocess summaries are excluded
from the count. The guard recorded exit 0, 363.0 seconds and peak process-group
RSS 1,910,767,616 bytes without a resource stop. Builds used two build workers and
two test threads, a task-owned target, no incremental/debug artifacts,
and the sampled 8 GiB RSS/12 GiB target/20 GiB free-space limits.

Independent read-only review found no actionable issues. Both SQLite decoder
callers retain their distinct diagnostics. Successful persists under contracts
exercise `RawResult::into_result` through the commit postcondition. Its invalid
status diagnostic is preserved by source comparison rather than a new direct
negative test; strict decoder rejection is checked at core and normal DB entry
points. This does not broaden the evidence to arbitrary SQLite corruption.

Formatting and whitespace checks passed. No Python code, public schema,
compatibility floor or historical OKF guarantee changed. Hosted CI results
belong to the eventual PR revision and are separate from these local results.

Strict workspace Clippy (`--locked --workspace --all-targets --all-features --
-D warnings`) passed on the final code.
