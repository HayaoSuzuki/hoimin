# Tracing and terminal progress review

## Agreed scope

Show execution stages, completed mutant counts, and elapsed time during interactive
`run`, `plan`, and `verify` commands. Enable detailed diagnostic logging through
`RUST_LOG`. Keep report stdout compatible. Work on `feat/tracing-progress` and
submit a pull request. Perform at least three self-review passes per stage.

## Design self-review

1. **User behavior:** existing human lifecycle messages do not show elapsed time
   while a test is running. Add periodic, plain progress lines on terminal stderr;
   count completed mutants without inventing a total for truncated or resumed runs.
   Help, completions, and historical `progress` comparisons stay free of live UI.
2. **Output compatibility:** enable progress only when stderr is a terminal.
   Default redirected stderr retains existing diagnostics. Explicit `RUST_LOG`
   enables additional tracing records: text on a terminal, JSON Lines otherwise.
   These debug records are distinct from the versioned report-event schema.
   Never write tracing or live progress to stdout. Keep library subscriber setup
   under the embedding application's control; initialize only in the binary.
3. **Runtime constraints:** publish small progress snapshots and bounded, lossy
   log messages to a dedicated output thread. Neither a slow sink nor log volume
   may block the Tokio executor. Bound shutdown flushing and release the guard
   before `process::exit`. Instrument async functions with async-aware spans and
   explicitly propagate spans into spawned process/blocking tasks. Avoid dumping
   source, configuration, environment, or test argv into tracing fields.

## Baseline

`cargo test -p hoimin-cli --test report_handler`: 29 passed.

## Implementation and validation

Results will be recorded after the corresponding review passes and checks.
