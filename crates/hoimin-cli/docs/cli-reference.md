# CLI reference

Source: [Rust CLI definitions](../../../crates/hoimin-cli/src/cli.rs).

Regenerate with `{regenerate}`; append ` -- --check` to verify without writing.

See the [usage guide](../guides/usage.md) for examples, OS-specific resource limits,
plan/verify inheritance and validation beyond Clap. Conditional requirements
and numeric parser bounds have no general stable Clap reflection API; their
help text and the manual guide remain authoritative. The metadata below lists
reflected arity, repetition, required arguments/groups, conflicts and delimiters.
