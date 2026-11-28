# Issue #348: portable path colon boundaries

Issue: https://github.com/tokyogas-tech/hoimin/issues/348

Branch: `fix/issue-348-portable-paths`, based on main `fdf8150`.
Worktree: `.worktrees/issue-348-portable-paths`.

## Outcome

Normalized root-relative paths now reject a colon in every component. The same
policy applies to candidate and diagnostic records, exact fingerprint files,
fingerprint include patterns, and the Windows final-name UTF-16 guard. Paths
such as `pkg/file.py:stream` can no longer cross these boundaries as an NTFS
alternate data stream.

This change does not add the optional Windows trailing-dot or trailing-space
policy; that behavior is outside issue #348.

## Root cause and design

`normalized_relative_path`, exact fingerprint normalization, and fingerprint
pattern validation checked only the first slash-separated component for a
colon. A drive-prefixed path was rejected, but a nested alternate-stream name
was accepted. The Windows final-name guard rejected separators, NUL, `.` and
`..`, but did not reject the UTF-16 code unit for `:`.

The core validator now makes colon rejection part of `valid_path_part`, so the
existing all-components traversal enforces one rule for every component. The
two fingerprint validators likewise test every component. The final-name guard
rejects `:` alongside the other forbidden code units before any Windows file
operation receives the name.

## TDD evidence

Before the production changes, the focused regressions failed at the expected
boundaries:

- Candidate validation accepted `pkg/calc.py:stream`.
- Exact fingerprint resolution classified `nested/file.toml:stream` as missing
  instead of invalid.
- Fingerprint pattern resolution classified the same path as unmatched instead
  of an invalid glob.
- The platform-neutral UTF-16 final-name test accepted `name:stream`.

After the fix, the complete candidate-policy, fingerprint-input, analyzer
protocol, and final-name focused suites passed. The analyzer protocol test was
updated because it previously asserted that a nested-colon candidate path was
accepted.

## Verification

| Check | Result |
| --- | --- |
| Candidate-policy integration suite | 11 passed |
| Fingerprint-input integration suite | 20 passed |
| Analyzer-protocol integration suite | 40 passed |
| Platform-neutral Windows UTF-16 guard test | 1 passed |
| macOS serial workspace suite, all targets | 1,453 passed; 12 ignored; 0 failed |
| Workspace Clippy, all targets and features, warnings denied | Exit 0 |
| Formatting and whitespace checks | Exit 0 |

The final macOS commands used Rust 1.98 and the separate target directory
`/private/tmp/hoimin-issue-356-target`:

```sh
CARGO_TARGET_DIR=/private/tmp/hoimin-issue-356-target cargo test -p hoimin-core --test candidate_policy
CARGO_TARGET_DIR=/private/tmp/hoimin-issue-356-target cargo test -p hoimin-cli --test fingerprint_inputs
CARGO_TARGET_DIR=/private/tmp/hoimin-issue-356-target cargo test -p hoimin-cli --test analyzer_handler
CARGO_TARGET_DIR=/private/tmp/hoimin-issue-356-target cargo test -p hoimin-cli workspace::root::tests::windows_final_names_are_single_components --lib
CARGO_TARGET_DIR=/private/tmp/hoimin-issue-356-target cargo test --workspace --all-targets -- --test-threads=1
CARGO_TARGET_DIR=/private/tmp/hoimin-issue-356-target cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
git diff --check
```

Python-backed integration tests used the main repository's frozen Python
3.14.7 environment through a temporary worktree-local `.venv` symlink. The
symlink was removed after the workspace run.

Native Windows test execution was not available on the macOS host. An attempted
`x86_64-pc-windows-msvc` compile check reached the dependency build but could
not run because the host does not provide the MSVC assembler `ml64.exe`. The
UTF-16 name guard remains compiled and executed by its platform-neutral unit
test on macOS.

Independent review found no blocking issues. The coordinator reran the 11
candidate-policy tests, 20 fingerprint-input tests, formatting, and whitespace
checks before committing. Changed files do not overlap #349 or #368.

## Lean correspondence for stored paths and exact fingerprint files

Stored mutation candidate paths require a nonempty sequence of slash-separated
components. Both lexical policies reject empty components, `.` and `..`,
backslashes, and NUL. The portable policy also rejects ASCII colons in every
component. The Unix workspace policy permits colons in otherwise valid names.
Candidate validation checks the schema, then the stored path, then the hash.

An exact fingerprint file argument can contain `.` components and repeated or
trailing separators. The normalizer removes those components and returns a
nonempty portable path. It rejects a leading separator and any `..` component.
The Windows form converts native backslashes to slash separators before
normalization. The corpus includes lexical names that are meaningful only at
the validation boundary, such as names with glob characters or control bytes.
Whether the normalized spelling resolves to a regular file depends on the
filesystem and the retained-root reader.

| Observation | Lean representation | Rust correspondence |
| --- | --- | --- |
| Stored path admission | Nonempty component list and portable or Unix policy | `RelativePathPolicy::allows` for each policy |
| Candidate error precedence | Matching schema and hash flags with a lexical path result | `validate_candidate_with_context` with a valid source and mutation |
| Exact file argument | Platform-specific separators and cleaned components | `normalize_exact_path` returns the exact path or `InvalidPath` with the original input |

Lean proves that the portable policy implies the Unix workspace policy, that
normalization produces portable components and fixes already portable inputs,
and that successful component normalization is idempotent. It also proves
that an invalid path takes precedence over any hash result when the schema
matches. The 122 cases cover all one- and two-component combinations of nine
representative spellings, nine nested combinations, 15 additional raw path
spellings, and eight schema/path/hash combinations. Five sensitivity checks
detect accepting colons past the first component, normalizing stored candidate
paths, accepting parent traversal, treating Windows separators as literal, and
checking the hash before the stored path.

The fingerprint integration test observes normalization and deduplication
through `resolve`, using a real file and an exact argument containing repeated
separators and dot components. The public fingerprint fixture uses a path
that can be created on every supported host.

On macOS, both lexical policies, candidate validation, and Unix fingerprint
normalization matched all 122 cases. The Windows normalization expectations
were generated in Lean. The candidate policy suite passed 92 tests, the
fingerprint unit and integration suites passed 18 and 56, and the CI workflow
suite passed 222. The longest input was 22 UTF-8 bytes. Lean checks peaked at
1,307,008 KiB under a 2 GiB limit and 20-second timeout. Single-job Rust checks
peaked at 1,751,136 KiB under a 3 GiB limit.

```sh
(cd formal/HoiminOracle && lake exe generate_environment_fingerprint -- --check-relative-path corpus/relative-path.jsonl)
(cd formal/HoiminOracle && lake exe generate_environment_fingerprint -- --sensitivity)
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-core --test candidate_policy
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --lib fingerprint_inputs
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --test fingerprint_inputs
```
