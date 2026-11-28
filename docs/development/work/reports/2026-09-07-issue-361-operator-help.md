# Issue #361: mutation operator help on applicable commands

## Scope and cause

This change fixes [#361](https://github.com/tokyogas-tech/hoimin/issues/361) on
branch `fix/issue-361-operator-help`, based on `c462867`. The shared
`RawMutationArgs` parser makes `--operators`, `--exclude-operators`, and
`--profile` available to both `hoimin run` and `hoimin plan`. Command assembly
attached the generated operator roster only to `run`, so `plan --help` described
the flags without listing the accepted operator IDs and selector families.

`verify` does not accept those mutation-selection flags. It executes candidates
already stored in a plan and accepts only its manifest, candidate or top
selection, selection policy, and output format. This change therefore does not
add an operator roster or new flags to `verify`.

## Implementation and compatibility

`root_command` generates one operator roster from
`MutationOperatorSelection::valid_names()` and attaches it to exactly the `run`
and `plan` subcommands. Completion generation now uses that same assembled Clap
command, keeping parsing, displayed help, and completion generation on one
command-construction path.

No argument names, defaults, accepted values, parsing rules, configuration,
schemas, or execution behavior changed. The generated completion scripts retain
the same argument surface; after-help text does not create completion options.
The regression coverage also confirms that `verify` rejects and does not
advertise `--operators`, `--exclude-operators`, or `--profile`.

## TDD and verification evidence

The baseline `cli_config` suite passed 54 tests. We extended the existing real
binary help test to exercise both mutation commands and every accepted operator
ID and selector family. Before the production change, the focused test failed on
`plan --help`, first reporting the missing `augmented_add_sub` entry. After the
command assembly change, the focused test passed. We added negative coverage for
the actual `verify` interface, and the complete `cli_config` suite passed 55
tests.

Final checks used the isolated target directory
`/private/tmp/hoimin-issue-360-target`:

| Command | Result |
| --- | --- |
| `cargo test -p hoimin-cli --test cli_config` | 55 passed |
| `cargo test --workspace --all-features` | Exit 0; all workspace, integration, and doc tests passed |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Exit 0 |
| `cargo fmt --all -- --check` | Exit 0 |
| `git diff --check` | Exit 0 |

The macOS workspace test used a temporary `.venv` link to the repository's
existing development environment; the link was removed after verification.
No Linux or Windows execution was performed. Independent review found no
semantic issue; its formatting observation was resolved with `cargo fmt`.

## Lean correspondence for operator selection

The oracle reads the published operator index in the CLI reference for IDs,
default membership, and family membership. Lean expands repeated and
comma-delimited selectors, resolves the included set before the excluded
set, and rejects an empty result. The proofs establish that every selected
member was included and that exclusion removes an operator even when it
appears more than once in the inclusion arguments.

The corpus has 310 cases. It selects all 173 documented operators and all
12 families, exercises every pair of families, and checks unknown names,
empty comma fields, whitespace, and exclusions that empty the set.
The Rust adapter exercises both CLI parsers and compares selected membership
and typed configuration errors. For accepted inputs, it serializes and
reloads the normalized run or plan configuration and compares the original
operator order after restoration. The Lean projection uses alphabetical
membership from the reference. Rust persistence keeps its declaration order.

The sensitivity check detects adding defaults to explicit choices, checking
excluded names before included names, omitting exclusions, and trimming
selector tokens.

```sh
(cd formal/HoiminOracle && lake exe generate_environment_fingerprint -- --check-operator-selectors corpus/operator-selectors.jsonl)
(cd formal/HoiminOracle && lake exe generate_environment_fingerprint -- --sensitivity)
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --test cli_config operator_selection_matches_lean
```
