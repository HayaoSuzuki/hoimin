# Task 6 Report: Focused capability workspace mutation improvement

## Status

Saturated on the macOS host with all meaningful native viable mutants caught.

## Fixed target

- Production files:
  - `crates/hoimin-cli/src/workspace/root.rs`
  - `crates/hoimin-cli/src/workspace/mutation.rs`
  - `crates/hoimin-cli/src/workspace/reset.rs`
- Function regex:
  `WorkerWorkspace::apply_mutation|MutationFile::into_writable|WorkerWorkspace::reset_from_snapshot|WorkerWorkspace::matches_snapshot|WorkerRoot::remove_entry_if_exists|WorkerRoot::restore|WorkerRoot::snapshot_matches|WorkerRoot::components|WorkerRoot::open_parent|WorkerRoot::reject_link|WorkerRoot::reject_link_if_present|required_directory`
- Test arguments: `--lib --test workspace_handler --test workspace_recovery`
- Timeout/jobs: 120 seconds, 2 jobs.
- No production target or mutation selector changed between retained runs.

## Test improvements

- Added a candidate-hash-only mismatch assertion, keeping the worker bytes equal to the manifest.
- Added direct changed-snapshot detection and verified an unchanged reset retains the same Unix
  file object.
- Covered snapshot comparisons independently for missing entries, wrong kind, changed bytes, and
  changed permissions.
- Covered missing removal, missing optional link checks, explicit link rejection, and
  `create=false` missing-parent behavior.
- Covered non-directory-parent errors so they cannot be classified as missing entries.
- Added a same-filesystem final-entry replacement test for the Unix mutation writable reopen,
  proving device/inode identity is required.
- Replaced unbounded two-party barriers in read/write/remove/mutation/reset race tests with
  deterministic zero-capacity channels and five-second `recv_timeout` waits. Hook-removal
  mutants now fail promptly instead of consuming the 120-second mutation timeout.

## Outcomes

- Initial focused run: 62 total; 36 caught, 20 missed, 2 unviable, 4 timeout.
- Retained round 2: 62 total; 52 caught, 8 missed, 2 unviable, 0 timeout.
- Final round 3: 62 total; 55 caught, 5 missed, 2 unviable, 0 timeout.

Final retained artifacts are under `/tmp/issue28-mutants-r3` and are not committed.

## Saturated exceptions

- `MutationFile::into_writable` first `|| -> &&`: on Unix, a reopened final entry under the same
  retained parent cannot have a different device while retaining the inspected inode in a
  meaningful filesystem scenario. The separately meaningful same-device/different-inode branch
  is now caught.
- `WorkerRoot::snapshot_matches` non-`NotFound` guard forced to true: ordinary API paths acquire
  and validate the stable parent before this final stat; deterministically injecting another
  final-stat error would require a new production test seam solely for the mutant.
- Three `snapshot_matches` byte/permission predicates are in the `cfg(windows)` branch and are
  not executable on the macOS host. Cross-platform characterization exercises the same contract,
  but Windows CI/mutation execution is required to classify these platform-specific mutants.
- Two function-return mutants remain unviable:
  `MutationFile::into_writable -> Ok(Default::default())` and
  `WorkerRoot::open_parent -> Ok((Default::default(), Default::default()))`.

No mutant was excluded and no production behavior was changed to improve the score.
