# Issue #364: candidate-scoped verification discovery

## Cause and scope

Plan verification resolved and hashed the complete saved target selection, then
passed every target to the analyzer even when verification requested candidates
from only one file. The final lookup compared only the requested IDs, so parsing
and candidate generation for unrelated files added no verification evidence for
the requested work.

Verification now derives the requested path set from candidates already checked
against the manifest, current source bytes, and stable IDs. It filters the
resolved `TargetSlice` list in place, preserving target order and every saved
line and symbol restriction. Rediscovery receives the original operator
selection, profile, candidate limit, and analyzer timeout.

All source records and fingerprint inputs are still resolved and compared before
rediscovery. Only analyzer parsing and candidate generation are scoped to files
that contain requested IDs. Baseline and mutant execution behavior is unchanged.

## Candidate and sequence validation

Fresh rediscovery still requires the requested stable ID and compares every
source-derived candidate field: path, span, original text, replacement,
operator, line, column, symbol, and file hash. A candidate that is absent under
the planned target slice, operators, profile, or limit remains invalid.

`MutationCandidate.sequence` is not part of `CandidateIdentity`; the analyzer
assigns it as a global spool ordinal after candidate generation. Filtering out
unrequested files necessarily changes later-file ordinals. Reconstructing the
old offset from unrequested manifest candidates would trust the data being
validated, so scoped verification does not do that.

Sequence validation is instead structural. Every retained manifest must contain
each value in `1..=candidate_count` exactly once. Validation does not require
manifest order to be sequence order because manifests are rank-sorted, and it
does not invent an ordering rule for candidates with equal analyzer sort keys.
Consequently, verification no longer authenticates the exact association
between a requested candidate and its original global ordinal, nor does it use
fresh analyzer output to prove the completeness of unrequested-file candidate
sets. These are enumeration properties, not stable candidate identity. Runtime
discovery assigns the execution sequence independently.

This change introduces no manifest schema, CLI, configuration, dependency, or
report-format changes. Existing generated plans, including truncated plans,
rank order that differs from discovery order, and multiple operators at one
span, remain valid.

## Regression evidence

The baseline plan integration suite passed 38 tests after linking the controlled
Python environment into the worktree. A behavioral test then selected a
candidate from the second of two files. Once target filtering was introduced but
before sequence was separated from semantic comparison, it failed with
`candidate descriptor differs`; this demonstrated the real later-file
renumbering boundary. After the sequence split, the expanded plan integration
suite passed 44 tests.

The new coverage verifies:

- requested-file filtering preserves original target order plus exact line and
  symbol slices;
- end-to-end preparation does not parse an unrequested file after its current
  bytes pass source-record validation;
- a later-file candidate verifies after subset rediscovery;
- a later-file candidate retained at a global candidate-limit boundary verifies
  from a truncated plan;
- rank-sorted sequence permutations and same-span, different-operator candidates
  remain valid;
- a sequence set with a duplicate and missing ordinal is rejected;
- symbol tampering is rejected by full semantic descriptor comparison; and
- the existing verification timeout still bounds scoped discovery before the
  test command.

## Verification

Commands ran on macOS with Rust 1.98.0, Python 3.14.7, and the isolated Cargo
target directory `/private/tmp/hoimin-issue-356-target`:

```sh
cargo test -p hoimin-cli --test plan
cargo test -p hoimin-cli --lib plan::tests
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
git diff --check
```

The plan integration suite passed 44 tests. The focused plan unit suite passed
6 tests with one benchmark ignored. The full all-features workspace suite exited
0; the main CLI library passed 531 tests with 9 ignored, and every integration,
core, and documentation test binary completed without failure. Workspace Clippy,
formatting, and whitespace validation exited 0.

The full workspace suite used the main checkout's Python environment through a
temporary worktree `.venv` symlink. Linux and Windows execution were not run for
this plan-validation-only change.
