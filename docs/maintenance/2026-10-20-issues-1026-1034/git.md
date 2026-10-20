# Git target selection and owned lifecycle evidence (#1026, #1032)

Date: 2026-10-20. Working branch: `fix/issues-1026-1034`.

## Reproductions and contracts

The new `changed_git_environment::scoped_zero_env_four` test failed before the
#1026 implementation: selected rows were `[2,3,4,5,6,7,8,9,10]`, expected `[6]`.
The central command builder removes only `GIT_DIFF_OPTS` for every target Git
command. Existing literal-pathspec environment handling remains conditional on
literal mode. User test environments retain `GIT_DIFF_OPTS`.

The new `changed_git_cancellation::total_timeout_reaps_first_git` test failed
before the #1032 implementation: actual exit 2, expected 4, after a 1s timeout
plus shutdown grace. Its helper announces its PID before blocking and is cleaned
up by the test even in the reproduced failure. This is a substitute process
ownership test, not a measurement of real Git performance.

`GitRunner` starts one owned Tokio task per Git child. Dropping the waiting future
requests cancellation through the shared `ProcessCancellation`; the owned task
continues. Cancellation and spawn share the same mutex gate. Cancellation closes
the two inline pipe readers, requests direct-child termination and awaits reap.
Successful cancellation is distinguished from failed cleanup by the internal
`GitError` type; only the former becomes `target.cancelled` at the effect boundary.
The shell accepts that result as its already observed timeout/cancellation rather
than an infrastructure failure. Timeout remains exit 4; SIGINT remains exit 130.

Shutdown waits for Git lifecycle completion with the shell's remaining shutdown
budget. Expiry does not abort or drop a lifecycle owner. A termination failure is
recorded persistently, and the owner still waits for the child. Failed reap or an
unexpected lifecycle drop never produces successful drain evidence. Normal
completion requires complete stdout/stderr byte collection and root reap.

The guarantee concerns the direct Git child, not a descendant sandbox. In
particular, cancellation closes inherited output pipes even if another process
holds them. A test creates such a descendant, proves the Git root has already been
reaped, then cancels and cleans the descendant up itself. It does not claim that
the runner terminates that descendant.

Default `TargetHandler::resolve`, `TargetHandler::handle` and `handle_git` keep
their existing signatures and use the same runner. Plan/verify callers therefore
have the same normal completion and dropped-waiter cleanup behavior. Dropped
waiters require the Tokio runtime to remain alive for asynchronous cleanup; the
runner is not a process owner that can complete async reap after runtime teardown.
No timeout or interrupt behavior is newly promised for standalone `plan`.

## #1026 design reviews (five passes)

1. Traced every target Git launch: repository probe, HEAD probe and checked Git
   commands previously constructed commands independently. All must use one
   builder, including inventory, scoped patch/numstat and full rename diff.
2. Compared command precedence with the reported row expansion: explicit
   `--unified` cannot override inherited `GIT_DIFF_OPTS`; removing that one
   variable at construction fixes both shrink and expansion cases.
3. Reviewed environment ownership: process-wide mutation would change user test
   behavior and permit cross-test races. Command-local removal is required.
4. Reviewed scoped versus rename semantics: keep inventory/pairing, flags and
   pathspec batching unchanged. Rename fixtures retain enough identical text to
   use Git's full rename-aware patch path.
5. Defined independent expected rows for default/zero/two contexts (6 versus
   4..8), and compare complete candidate arrays to detect ID/order/ranking drift.
   Persisted normalized selection must contain the requested context.

## #1026 implementation reviews (five passes)

1. Inspected construction after centralization: `Command::new("git")` exists
   only in `git_command`; environment removal is owned there.
2. Checked that HEAD and worktree status handling retain their old semantics;
   centralization does not turn a missing HEAD into a checked-command failure.
3. Checked literal-pathspec variables: removal stays conditional and all Git
   arguments retain their old order and content.
4. Reviewed conversion/intersection boundaries: Git LF rows still translate
   before joining physical source rows; context is never re-expanded afterward.
5. Checked public wrappers and user-process environment: signatures preserved;
   the public run regression executes a real Python baseline with inherited
   `GIT_DIFF_OPTS`, intentionally leaving a surviving mutant (normal exit 1).

## #1026 verification reviews (five passes)

1. Observed the new regression fail at the expected selected-row assertion,
   rather than at setup, parsing or Python discovery.
2. Ran the 18 macro-generated independent cases after centralization; all passed.
   Parameters cover default/0/2 context, unset/unified0/unified4 environment,
   scoped/full-rename diff.
3. Re-ran the matrix after introducing owned lifecycles; all 18 still passed,
   showing the runner did not change target selection or candidate identities.
4. Inspected assertions for independent expected rows, exact complete candidate
   arrays, changed-line ranking reason and saved normalized context.
5. Added the public baseline environment regression. Its first observed exit 1
   was the correct surviving-mutant result; corrected the test's initial exit-0
   expectation. This was a fixture expectation correction, not a product fix.

## #1032 design reviews (five passes)

1. Traced shell serial cancellation to the missing target token and the three
   `Command::output` launches. Dropping their future did not retain child ownership.
2. Reused the existing shared cancellation spawn gate, rather than adding a
   second flag or allowing a check/spawn race.
3. Chose a Git runner separate from user test resource limits: existing Git
   parsing needs complete binary output and must not acquire test resource policy.
4. Defined cancellation separately from normal command failure: only proven
   kill/reap/pipe closure is a cancellation completion; stderr and real errors
   retain their existing failure reporting.
5. Connected lifetime and proof to the existing shell budget; detached ownership
   survives a dropped waiter, and drain expiry must retain a failure result.
   Explicitly bounded the guarantee to direct children and live-runtime cleanup.

## #1032 implementation reviews (five passes)

1. Audited spawn admission: the shared cancellation lock spans check and spawn;
   every Git path now reaches the same runner.
2. Audited normal byte capture: wait and both pipe readers run concurrently,
   avoiding full-pipe deadlock; success requires all three results.
3. Audited cancellation: inline readers drop before kill/reap, including when
   the root wait already completed while an inherited pipe remained open.
4. Audited waiter drop and panic paths: a cancel-on-drop guard stops dispatch;
   a lifecycle guard retains active accounting and persistent missing-proof
   errors if the owned task unwinds or is dropped.
5. Audited shell integration: both process-drain checkpoints include Git drain;
   Git errors invalidate `all_reaped`; only `target.cancelled` is replaced by the
   previously observed stop event, preserving real infrastructure failures.

## #1032 verification reviews (five passes)

1. Observed the new first-Git timeout reproduction fail with exit 2 before the
   runner/shell integration, proving it covers the original shutdown defect.
2. Ran all four public stop regressions: first/later Git command × total timeout/
   SIGINT. Each passed with exit 4/130, no living direct helper at CLI exit and no
   later Git requests. Starts are synchronized by helper-written PID files.
3. Ran runner tests covering already cancelled admission, active-child reap,
   dropped waiter ownership, a reaped root with a held-open pipe, and exact normal
   stdout/stderr/nonzero status. All five passed before adding the final race case.
4. Added a synchronized cancellation/normal-exit race; either full successful
   output or typed cancellation is allowed, but reap and drain proof are required.
5. Reviewed failure ownership again after parent review found that the initial
   reap-error return still released the child. Corrected the cleanup helper to
   retain the child and active lifecycle while retrying wait. Two injected child
   lifecycle cases now prove retention after a failed wait, persistent drain
   failure after eventual reap, and preservation of simultaneous kill/wait
   diagnostics. Real OS-induced kill/wait failures and Windows/Linux behavior
   remain unconfirmed; the injections exercise the shared sequencing helper.

## Commands and results

All Cargo commands use:
`CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/target`.

- `cargo test -p hoimin-cli --test changed_git_environment scoped_zero_env_four -- --exact --nocapture`
  — red: selected 9 rows rather than 1.
- `cargo test -p hoimin-cli --test changed_git_environment`
  — green: all initial 18 matrix cases.
- `cargo test -p hoimin-cli --test changed_git_cancellation total_timeout_reaps_first_git -- --exact --nocapture`
  — red: exit 2 rather than 4.
- `cargo test -p hoimin-cli --lib target::git::runner::tests -- --nocapture`
  — green: initial five lifecycle tests.
- `cargo test -p hoimin-cli --test changed_git_environment --test changed_git_cancellation --test changed_git_scope --test target_handler`
  — four cancellation cases and 18 matrix cases passed; the added user-environment
  case initially failed its incorrect exit-0 assertion with actual exit 1; corrected.
  Cargo stopped before the two remaining integration binaries in that invocation.

- `cargo test -p hoimin-cli --lib target::git::runner::tests -- --nocapture`
  — final eight cases passed, including the normal-exit race and two injected
  cleanup-failure cases. The first six passed before adding failure injection.
- `cargo test -p hoimin-cli --test changed_git_environment --test changed_git_cancellation --test changed_git_scope --test target_handler`
  — final focused run passed: 19 environment cases, 4 cancellation cases, 11
  scoped-Git cases and 52 existing target-handler cases (86 total).
- `cargo clippy -p hoimin-cli --all-targets --all-features -- -D warnings`
  — Git API documentation warnings corrected; this invocation failed on concurrent
  runtime preparation changes: non-Send shell future, fingerprint pattern function
  length, preparation `err_expect`, shell collapsible-if, timing argument ownership
  and assigning-clones. Runtime owner notified with exact diagnostics.

Parent owns combined full-suite validation and reports actual final results
separately. No claim here that a full workspace suite has already passed.

## Integration review corrections

The independent review identified two additional lifecycle boundaries. The
runner now checks the public run's existing request gate and original deadline
under the same lock order as test-process spawning. It can reject the next Git
command before the shell propagates lifecycle cancellation. An initial wait or
pipe-read failure also persists in the drain state even when cleanup reaps the
root successfully and the shell supersedes the original effect with a stop.

Three regression cases failed before these corrections: cancellation between
commands, expiry before scheduler observation, and a successful cleanup hiding
the initial wait failure. The complete runner suite then passed all 11 cases.
The shell also prioritizes stop observation and reconciles a cancelled Git
completion that races the final stop poll.

## #1034 janitor follow-up from independent review

Independent review found that controlled selection computed deadline D, then its
locked worker constructed a later D2 from the same duration. Cancellation froze
the synthetic clock at D, so D < D2 could permit all remaining traversal and cursor
persistence. The controlled path now passes its original absolute deadline into
the locked worker. Existing ordinary selection creates its one cutoff before
entering that same worker.

A related review found that deletion slices carried a capped duration but lost
the dynamic preparation stop predicate. A slice could therefore continue for its
existing five-second maximum after cancellation. `CleanupBudget` now carries both
the duration and optional shared stop callback through capping, subtraction and
all existing cooperative filesystem deadline checks. First removal, retries,
empty-unmarked-root removal, marker removal, permission repair and directory
reopening receive that same control. Ordinary owner cleanup accepts its existing
duration without a stop callback. Cancellation cannot interrupt a filesystem
syscall already in progress; the checks stop subsequent guarded operations.

Design reviews (five passes):

1. Reconstructed both selector deadlines and the frozen-clock comparison; a
   single absolute cutoff removes the incorrect extra budget.
2. Reviewed coordinator lock ownership: controlled selection uses try-lock and
   unlocks on both deadline failure and successful selection.
3. Reviewed the cursor write boundary: stopping after the first iterator entry
   must prevent completion and leave coordinator bytes unchanged.
4. Traced claimed-tree removal through first slice, pending rounds and nested
   helpers; a duration alone cannot communicate cancellation mid-slice.
5. Chose to extend the existing cleanup budget/check mechanism with an optional
   callback, preserving filesystem identity checks and normal cleanup policy.

Implementation reviews (five passes):

1. Checked that controlled selection passes D directly and no locked helper
   computes a fresh D2 for that path.
2. Added an iterator-entry hook beside the existing enumeration-completion hook
   to synchronize the regression without clocks, sleeps or check-count guesses.
3. Verified `CleanupBudget::min` and `saturating_sub` preserve the same callback;
   global elapsed/stop checks continue to cap each local slice.
4. Inspected nested removal, root control-marker, reopening, permission and
   platform-specific helpers: their existing checks all receive the full budget.
5. Verified that acknowledged removal progress remains in `CleanupCapabilities`
   and the janitor ledger when cancellation leaves the remaining root unresolved.

Verification reviews (five passes):

1. Observed `preparation_stop_after_first_janitor_entry_preserves_cursor` fail:
   the stopped traversal returned a completed selection.
2. Observed `preparation_stop_after_first_deletion_preserves_remaining_entries`
   fail: the deletion slice completed despite stop after the first payload file.
3. Re-ran both regressions after the fixes: both passed; first-entry cancellation
   observes exactly one entry, and first-deletion cancellation preserves the
   other two files and lease with one acknowledged removal.
4. Added `controlled_janitor_propagates_stop_into_claimed_tree_deletion` to test
   the production controlled entry point. An anchored directory capability makes
   its stop observation independent of the claim rename.
5. Ran library Clippy with all features and denied warnings: passed. Broader
   focused test compilation initially encountered concurrent shell test pattern
   borrow errors; these were reported rather than attributed to janitor behavior.

Commands (using the same `CARGO_TARGET_DIR`):

- `cargo test -p hoimin-cli --lib preparation_stop_after_first_janitor_entry_preserves_cursor -- --nocapture`
  — red: stopped enumeration incorrectly completed.
- `cargo test -p hoimin-cli --lib preparation_stop_after_first_deletion_preserves_remaining_entries -- --nocapture`
  — red: cancelled slice incorrectly completed deletion.
- `cargo test -p hoimin-cli --lib preparation_stop_after_first -- --nocapture`
  — green: both exact regressions passed.
- `cargo clippy -p hoimin-cli --lib --all-features -- -D warnings`
  — green.
- `cargo test -p hoimin-cli --lib workspace::owned`
  — final focused run green: 86 passed, 0 failed, 3 ignored subprocess fixtures;
  the concurrent-bootstrap test additionally invoked its child fixtures successfully.
  Includes the three new cancellation regressions and existing cursor, identity,
  retained-marker, partial-progress and deep-tree cleanup checks.
