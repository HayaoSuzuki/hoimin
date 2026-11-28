# Import root normalization

`--import-root` supplies an ordered list of project-relative directories.
The CLI removes redundant separators and current-directory components,
resolves parent components within the project, and keeps the first occurrence
of each normalized root. Checked run and plan settings compare the supplied
path strings against the normalized list. A saved value with redundant syntax
or duplicates is rejected. An invalid root is reported before a difference
in canonical spelling.
An empty CLI argument fails during argument parsing. In checked settings
the empty string is an invalid path.

The Lean model distinguishes Unix-style separators from Windows separators
and drive prefixes. It proves that an unmatched parent component at the
project root is rejected, that an invalid first path determines the error,
and that the first normalized path retains its precedence. A named Windows
case covers a drive prefix exposed after reducing earlier components.

The corpus enumerates 64 three-component paths and adds 21 cases for
ordered aliases, exact spelling, and platform boundaries. The Rust adapter
uses both public CLI parsers. It also checks direct construction and JSON
deserialization of checked run settings, then validates and restores
serialized plan settings. It compares ordered path strings and the typed
configuration error, including the offending path. Filesystem resolution,
symlinks, and runtime availability of import directories are outside this
lexical model.

The sensitivity check detects sorting the roots, discarding a leading parent,
silently normalizing persisted settings, and applying Unix-style separator
rules on Windows.

```sh
(cd formal/HoiminOracle && lake exe generate_environment_fingerprint -- --check-import-roots corpus/import-roots.jsonl)
(cd formal/HoiminOracle && lake exe generate_environment_fingerprint -- --sensitivity)
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --test cli_config import_root_normalization_matches_lean
```
