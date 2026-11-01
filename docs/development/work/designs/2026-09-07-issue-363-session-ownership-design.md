# Issue #363: require ownership for session result operations

## Selection and scope

The open bug list gives #338 priority P2, but main already classifies memory
violations using only `oom_kill` and tests nonterminal pressure counters.
Among the remaining P3 issues, #363 is selected for persisted-result integrity:
a library caller can currently write into an incomplete run owned by another
handler, including replacing an inconclusive result. #341 concerns a rare
old-kernel PID-recycling race; selector and diagnostic issues remain separate.

Work lives only in `.worktrees/issue-363-session-ownership`, on
`fix/issue-363-session-ownership`, based on main commit `520a388`. The user
authorized autonomous implementation through PR creation and requested that
documents be committed in each issue's own worktree.

## Contract

`begin` and successful `load` retain a run-specific OS ownership lock in the
handler. `finish` releases it. Mirror the existing `finish` guard in `persist`
and the resume-oriented `lookup` operation, before starting a transaction or
reading run state. Check the requested run ID, not merely whether the handler
owns any run. Do not acquire ownership implicitly in either operation.

Unauthorized requests return `session.persist.owner` or `session.lookup.owner`
with the request's effect ID. Ownership rejection takes precedence over run
state, database corruption, duplicate-result, and transaction errors. This
intentionally changes errors for callers that skip `begin`/`load` or invoke
result operations after `finish`. `lookup` is a resume operation; this change
does not introduce a separate general-purpose database inspection API.

Successful owners retain existing transaction, result-replacement, diagnostics,
and completion checks. No database migration, JSON schema, lock algorithm, or
state-machine ordering changes are required. A handler that finishes an
incomplete run must successfully load it again before result operations, even
when no other handler has claimed it yet.

## Implementation and evidence

1. Establish a clean session-handler test baseline.
2. Add real SQLite/two-handler regressions for unauthorized insert/overwrite,
   lookup, ownership of another run, failed load, and ownership handoff.
3. Observe those regressions fail before adding guards.
4. Add guards and update API documentation. Adjust existing malformed-database
   and commit-failure fixtures to establish ownership before fault injection,
   preserving coverage of their original database behavior.
5. Run session tests and relevant CLI integration tests, contracts, formatting,
   and Clippy; validate OS locking on macOS and a nonroot Linux container.
6. Commit implementation, tests, and validation report here; create a PR.

Direct tests use the real ownership locks and database, with database snapshots
to show rejected requests have no side effects. Extend the existing Lean
session model with the same ownership requirement and operation-specific error
codes. Prove that unauthorized persist/lookup preserve state, add public-API
handoff cases, and regenerate the corpus from Lean. Preserve the existing
depth-8 default and add broken-ownership witnesses; run Lean commands serially
under 20-second wall and 768-MiB RSS limits. After the depth-8 source run reached
the deadline, add an explicit `--depth 1..8` option and validate depth 4 locally.
Corpus rendering does not depend on the exploration depth. No duplicate model
is introduced. The validation report distinguishes these bounds from proofs.
Independent code review checks guard placement and fixture coverage.

GitHub API access is limited to issue/PR discovery and publishing; no polling
loop or manual Windows/macOS CI dispatch is part of this delivery.
