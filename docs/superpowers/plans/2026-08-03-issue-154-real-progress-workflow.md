# Issue #154: Real Run-to-Progress Workflow

## Goal

Exercise the published workflow at the process boundary: generate two reports with the compiled `hoimin` binary, persist its stdout, and pass the files to the compiled `hoimin progress` command. This closes the gap left by unit tests that construct report values or call `run_with_io` directly.

## Scope

- Use one temporary Python source tree with exactly one `binary_add_sub` candidate.
- Run that candidate first with a test command that kills it, then with a test command that lets it survive.
- Require the runs to exit `0` and `1`, respectively, and require their stable candidate IDs to match.
- Persist both unmodified stdout documents and invoke `hoimin progress --format json` with them in chronological order.
- Assert the complete comparison contract: state, common/added/removed counts, improvement/regression counts, and all three scores.
- Bound each real process invocation to 30 seconds while retaining and reaping the spawned `Child`. On Unix, timeout cleanup sends the same real `SIGINT` used by the CLI's cancellation contract, gives hoimin five seconds to tear down its supervised process tree, sends the second bounded interrupt if finalization is stuck, and retains a final kill-and-reap fallback. Pipe draining is separately bounded. Temporary files are owned by `TempDir` and are removed only after cleanup has reaped the CLI.
- A generic non-interactive Windows test process cannot reliably inject a console control event into an inherited CI console. The timeout-only Windows branch therefore uses bounded `taskkill /T /F` process-tree termination and then explicitly reaps the retained CLI child. The ordinary workflow and its 30-second bound remain cross-platform; the Unix-only timeout regression test supplies the real graceful-signal proof already established by the repository's signal tests.

Production APIs and report fixtures are unchanged.

## Existing oldest-schema evidence

The oldest supported schema criterion remains covered by these existing tests:

- `progress::input_accepts_the_oldest_schema_v2_normalized_config` proves the progress reader accepts the checked original schema-v2 report.
- `report_handler::original_schema_v2_report_fixture_matches_the_published_schema` proves that fixture still matches the published report schema.

The real workflow test intentionally uses two current reports from the same binary. It does not add a weaker mixed-version parsing assertion.

## Regression test

`progress::real_run_reports_expose_exact_regression_through_progress` catches, among other bugs:

- report output no longer being parseable or routed through stdout;
- outcome-dependent or otherwise unstable candidate IDs across identical source trees;
- `killed -> survived` no longer being classified as one regression;
- incorrect common/added/removed bookkeeping; and
- incorrect previous, current, or delta score serialization.

`progress::real_run_timeout_reaps_the_supervised_process_tree` starts a real timed-out mutation verifier with its own descendant, records both PIDs, and asserts that the graceful cleanup path has stopped both before the helper returns. The assertion is Unix-only because that is the platform where the test can inject the repository's proven real `SIGINT`; Windows still has deterministic bounded tree cleanup as described above.

Mutation effectiveness is checked by temporarily removing the production `killed -> survived` regression increment. The focused test must fail on the exact `latest.state` assertion, after which the production line is restored and the same test must pass. No production mutation is committed.

Cleanup effectiveness is checked against the superseded direct `kill_on_drop` timeout implementation. With that implementation, the real timeout test observed the verifier root PID still alive and failed before performing its own final test teardown. With retained-child graceful cancellation, the same test observes both the verifier and its descendant stopped.

## Verification

Run:

```console
cargo test -p hoimin-cli --test progress real_run
cargo test -p hoimin-cli --test progress
cargo test -p hoimin-cli --test progress input_accepts_the_oldest_schema_v2_normalized_config -- --exact
cargo test -p hoimin-cli --test report_handler original_schema_v2_report_fixture_matches_the_published_schema -- --exact
cargo fmt --all -- --check
cargo clippy -p hoimin-cli --tests -- -D warnings
git diff --check
```
