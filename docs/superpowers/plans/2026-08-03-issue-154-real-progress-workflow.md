# Issue #154: Real Run-to-Progress Workflow

## Goal

Exercise the published workflow at the process boundary: generate two reports with the compiled `hoimin` binary, persist its stdout, and pass the files to the compiled `hoimin progress` command. This closes the gap left by unit tests that construct report values or call `run_with_io` directly.

## Scope

- Use one temporary Python source tree with exactly one `binary_add_sub` candidate.
- Run that candidate first with a test command that kills it, then with a test command that lets it survive.
- Require the runs to exit `0` and `1`, respectively, and require their stable candidate IDs to match.
- Persist both unmodified stdout documents and invoke `hoimin progress --format json` with them in chronological order.
- Assert the complete comparison contract: state, common/added/removed counts, improvement/regression counts, and all three scores.
- Bound each real process invocation to 30 seconds and enable kill-on-drop so timeout cleanup is conditional and does not leave the direct child running. Temporary files are owned by `TempDir` and are removed only after all awaited processes have exited.

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

Mutation effectiveness is checked by temporarily removing the production `killed -> survived` regression increment. The focused test must fail on the exact `latest.state` assertion, after which the production line is restored and the same test must pass. No production mutation is committed.

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
