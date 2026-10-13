# Configuration root invariant (#843)

Base: latest fetched `origin/main`, `b3c1c36c07a5fc9ce77fc44ae125ecde62aaef26`.
Branch: `fix/843-config-root-invariant`.

## Design

Both normalized configurations must use the same project root for selection
and execution. Extend the existing private selection validator to receive the
configuration root. Use `Utf8Path` equality, as the existing manifest check does;
do not canonicalize paths or access the filesystem during validation.

Move normalized configuration validation into the common manifest header
validator. Remove the separate root comparison and subsequent duplicate
configuration validation. Keep live fingerprint environment capture at its
existing verification boundary. Preserve the mismatch diagnostic text,
`selection root differs from normalized config root`.

Keep the existing two assignments in `fingerprint_inputs::recheck_config`:
there is only one production relocation site, so adding a public setter or
another abstraction would expand the API without removing duplication.
Retain all saved fields, schema versions and fingerprint algorithms.

## Design self-review

1. Traced consumers: target selection reads `selection.root`, while workspace
   and analyzer setup use `root`; their agreement is a configuration contract.
2. Checked constructors and conversions: raw construction clones one root;
   run/plan conversions preserve both fields. Public mutation/deserialization
   still requires validation.
3. Compared path semantics: retain component-based path equality, including
   harmless trailing separators; no filesystem identity or symlink resolution.
4. Reviewed compatibility: preserve the mismatch message and schema; other
   invalid configuration diagnostics may precede invalid manifest records after
   validation moves earlier. Valid manifests retain their behavior.
5. Reviewed scope/cost: one private validator and existing manifest entry points
   suffice. No generic path framework or relocation API. The narrow ownership
   design scores 10/10 on the eight software-design-philosophy diagnostics;
   the prior ownership/comment gaps are addressed by shared validation and its
   documented contract, without claiming all public fields are immutable.

## Plan

- [x] Fetch main, create an isolated branch, inspect callers, and run existing
  core plan/config JSON tests before editing production code.
- [x] Add negative tests for both public validators after mutation and JSON
  deserialization; observe failure before implementing the common invariant.
  Cover equal roots and harmless component spelling as positive cases.
- [x] Integrate core validation at the manifest boundary; pin the existing
  root diagnostic and schema-error precedence with small unit tests and the
  existing verify regression.
- [x] Exercise fingerprint rechecking with a separate worker root and changed
  worker bytes; run existing configuration, plan and fingerprint regressions.
- [x] Run formatting and Clippy, complete five implementation and five test
  self-reviews, obtain an independent review, commit the change and this record,
  and clean the dedicated target directory.

## Plan self-review

1. Mapped both configuration validators to mutation and deserialization
   negatives; neither entry point can rely solely on the CLI regression.
2. Selected string assertions for the initial negative tests so RED means an
   accepted inconsistent config, not a missing error variant compilation error.
3. Covered all header consumers: generated output, read-only review and verify;
   keep live environment checks exclusive to verify.
4. Selected a worker-root test with distinct original/worker contents to catch
   accidentally reading the original workspace; avoid a new large E2E.
5. Bounded resources: two build jobs, no debug info or incremental compilation,
   dedicated `target/issue-843`, test threads limited to two, RSS/disk monitoring
   for CLI builds. macOS rejected `ulimit -v`, so use process-tree monitoring
   rather than claim an unavailable hard address-space limit.

## Verification and remaining reviews

Baseline: core `plan_config` and `config_json` passed before production edits.

## Implementation self-review

1. Re-read the core diff and all three `validate_selection` call sites. Both
   public validators and raw construction pass their own root; the equality
   rule exists in one place. Corrected a missing `Utf8Path` import found by
   compilation before the successful core run.
2. Traced all three `validate_header` consumers again. Review, verify and plan
   output now share configuration validation; only verify captures live
   fingerprint environment state. Renamed that helper to describe this role.
3. Compared diagnostic order and text. Schema/kind/ranking versions still win
   over root errors. Root mismatch still wins over other configuration errors.
   Selection errors now precede fingerprint-environment errors; normalized
   configuration errors now precede record/candidate errors for multiply-invalid
   inputs. These diagnostic priority changes are intentional and documented.
4. Re-read relocation and fingerprint consumers. No production fingerprint or
   root-relocation code changed. The added worker test distinguishes exact-file
   hashing from hierarchy input discovery, checks each changed worker input,
   and asserts the prepared configuration remains unchanged.
5. Reviewed the whole diff for scope and API changes: fields, serde attributes,
   versions, dependencies and fingerprint encoding are unchanged. The only new
   public Rust item is `ConfigError::SelectionRootMismatch`; exhaustive Rust
   matches may need a new arm. This is an internal Rust API change, with no
   CLI option or stored-schema migration. No performance claim is made.

Core verification: all `cargo test --locked -p hoimin-core` tests passed.
The two root negatives first failed with `unwrap_err()` receiving `Ok(())`;
the new header delegation test first failed because records were checked
without validating the missing selector. The existing root diagnostic test
already passed before the refactor. Test-only missing imports were corrected
before recording the header's behavioral RED result.

## Test self-review

1. Compared RED and GREEN logs: the two core negatives changed from accepting
   inconsistent roots to returning the exact mismatch diagnostic; the header
   test changed from record rejection to the expected missing-selector error.
2. Audited negative assertions: both validators are called after mutation and
   deserialization, changing either root field across the cases. Matching roots
   still pass; component-equivalent nonexistent paths require no filesystem.
3. Checked compatibility results: core JSON/plan round trips, legacy plan input,
   read-only review, environment fingerprints and the exact verify diagnostic
   pass. Schema-version precedence is asserted separately from root rejection.
4. Checked worker results: the new relocation test passes for unchanged copied
   inputs and rejects changed exact/hierarchy files without changing the source
   config. Existing shell fingerprint recheck/ABA tests also pass.
5. Audited execution scope and failures: core has 366 passes (2 ignored), CLI lib
   has 828 passes (18 ignored), CLI config has 59 passes, and the six related
   integration binaries have 165 passes (1 ignored). Ignored tests are existing
   benchmark/inspection/subprocess fixtures. This is focused regression coverage,
   not a claim that every workspace integration binary or OS matrix ran.

Commands used (with `CARGO_BUILD_JOBS=2`, `CARGO_INCREMENTAL=0`, dev/test debug
info disabled and `RUST_TEST_THREADS=2`):

```text
cargo test --locked -p hoimin-core
cargo test --locked -p hoimin-cli --lib --test cli_config --test plan --test fingerprint_inputs --test fingerprint_env --test fingerprint_env_plan --test review_cli --test exception_hierarchy
cargo test --locked -p hoimin-cli --test plan --test fingerprint_inputs --test fingerprint_env --test fingerprint_env_plan --test review_cli --test exception_hierarchy
cargo fmt --all -- --check
git diff --check
```

The first CLI run passed lib/config but eight hierarchy tests could not spawn
their hard-coded worktree `.venv/bin/python`. The original checkout's existing
Python 3.14 environment was linked from an ignored worktree-local `.venv/bin`;
all six integration binaries then passed without production changes. The
second command's failed overall exit is not counted as a successful whole run.

The monitor samples the Cargo process group's RSS and dedicated target size
every three seconds; it stops at 8 GiB RSS, 5 GiB target or less than 10 GiB
free disk. This is a sampled guard, not a hard allocation cap. The initial
sandboxed monitor could not invoke `ps`; that build was stopped and restarted
with approved process inspection. Recorded test processes peaked at sampled
1,601 MiB RSS and 1,095 MiB target size.

`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`
passed (25.48 seconds; sampled peak RSS 1,074 MiB, target 1,111 MiB).
Formatting and whitespace checks passed. Total unique selected tests passed:
1,418, excluding subprocess fixture reruns and the 21 existing ignored entries.

## Independent review and completion

A separate fresh-context reviewer inspected the full diff, surrounding callers,
this record and validation logs. Verdict: no Critical, Important or Minor
findings. No reviewer builds ran concurrently with the resource-bounded checks.

The reviewer explicitly set aside these behaviors; dispositions are:

- Mandatory validation at every public execution API: retain existing behavior;
  #843 specifies the configuration validators. Callers must still validate
  configurations after mutation/deserialization.
- Canonical filesystem identity/symlink equivalence: retain existing path
  equality. Distinct path spellings that require filesystem resolution are
  intentionally not treated as the same root.
- Other platforms/full workspace integration matrix: not claimed by this local
  verification. CI remains responsible for platform-specific differences.
- Detecting deletion of only the worker `selection.root` assignment: current
  hierarchy discovery derives its root from `config.root`, so this is not an
  observable distinction in the worker hashing test. The paired assignments
  remain unchanged and were independently inspected; no artificial API/test
  hook was introduced solely to expose that intermediate clone.

`target/issue-843` was cleaned with `cargo clean --target-dir target/issue-843`
(1.1 GiB removed). Commit hooks subsequently repeated workspace/vendor formatting
and Clippy successfully, using the worktree's default target rather than the
monitored target. That hook run did not inherit the monitoring wrapper; its
new build artifacts (855 MiB) were also removed. Cargo refused to clean that
parent directory because it lacked `CACHEDIR.TAG`; after verifying it contained
only this worktree's generated artifacts, it was removed directly. No other
checkout's target was cleaned.

The branch/worktree and ignored verification logs are retained for review. The
implementation and this record are committed together; no push or merge is
part of this local completion.

## Full-suite follow-up and PR preparation (2026-10-13)

On the unchanged implementation commit `16285fd`, unfiltered `cargo test`
completed with exit 0: 2,837 passed, 0 failed and 29 ignored across 149 test
binaries/doctest groups, excluding nested subprocess result duplication.
The first full run exposed an incomplete worktree virtual-environment link:
linking only `bin` did not expose pytest. Linking the existing `pyvenv.cfg`
and `lib` corrected the local environment; the entire command then passed.
No production or test source changed. The final run used two build/test threads
and the same resource monitor (319 MiB sampled peak RSS, 4,038 MiB target).
The dedicated target was cleaned afterward (3.9 GiB removed).

PR preparation adds the root-validation contract to the existing selection
OKF concept, referencing the actual implementation commit and content hashes.
Documentation self-review: (1) checked each claim against both validators and
manifest callers; (2) verified source revisions/hashes; (3) preserved historical
sources and version statements; (4) separated the full-suite result from the
initial focused checks and documented the environment correction; (5) checked
that the documentation-only follow-up changes no executable code or public
format. Other operating systems remain unverified locally.
