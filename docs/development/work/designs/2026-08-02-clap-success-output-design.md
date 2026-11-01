# Clap success output routing design

## Problem

Clap represents help and version display as errors with exit code zero. The CLI
currently writes every `CliError::Clap` value to stderr, so successful
`--help`/`--version` invocations cannot be piped or redirected from stdout.

## Design

Keep the injected writer architecture used by `run_with_io`. When a Clap error
has exit code zero, write its rendered text to stdout; otherwise write it to
stderr. Return Clap's exit code unchanged. This follows the same success/error
boundary as `clap::Error::print()` while remaining testable with caller-owned
writers.

Use `write!` rather than `writeln!` because Clap's rendered diagnostic already
owns its final formatting. Writer failures remain best-effort, matching the
existing top-level diagnostic behavior.

## Tests

End-to-end writer tests cover root help, version, and an invalid option. Help
and version must exit zero with non-empty stdout and empty stderr; the invalid
option must remain non-zero with empty stdout and a stderr diagnostic.
