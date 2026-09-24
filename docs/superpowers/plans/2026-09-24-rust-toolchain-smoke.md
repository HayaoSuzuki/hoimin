# Rust toolchain smoke contract repair

> Execution: implement inline with `superpowers:executing-plans`; review the final branch independently.

## Design

PR #578 changes Rust 1.98.0 to 1.98.1. Its Wheel smoke job fails before
building a wheel because `RepositoryRustToolchainContractTests` compares the
declaration to a second, hard-coded `1.98.0` value. The exact repository pin
belongs in `rust-toolchain.toml`; a contract test should check that the pin is
a complete stable release, not duplicate its current value.

Create `fix/rust-toolchain-smoke` from current main, include the 1.98.1 update,
and create a replacement PR referencing #578. Leave Renovate's branch alone.
Accept canonical numeric major.minor.patch strings, including future patch and
minor updates. Reject floating channels, incomplete versions, prereleases,
suffixes, whitespace and leading-zero components. Keep the exact declaration
keys, minimal profile and clippy/rustfmt component checks. Package/wheel version
identity is a different contract and remains exact.

The existing reproducibility design is the source contract:
`docs/superpowers/specs/2026-08-24-rust-toolchain-reproducibility-design.md`.
Append a dated amendment rather than rewriting its historical evidence.

## Implementation plan

1. Run the existing workflow test suite on main. Set only the toolchain pin to
   1.98.1 and rerun `RepositoryRustToolchainContractTests` to reproduce the
   reported assertion failure.
2. Extract the existing declaration assertions into a test helper and replace
   only the channel equality with `assertRegex` using
   `r"\A(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\Z"`.
   Keep profile, component and declaration shape assertions unchanged.
3. Exercise that helper with 1.98.0/1.98.1/1.98.10/1.99.0, floating and malformed
   channels, changed profile and missing/changed components. The fixture tests check
   acceptance boundaries without installing hypothetical toolchains.
4. Update `docs/development.md` to refer to the manifest as the source of the
   exact version. Append the design amendment and refresh its OKF source entry.
5. Run the entire Python unittest suite, Rust quality gates and a real wheel
   build/install smoke test with Rust 1.98.1. Record results and limitations.
   Python changes are confined to test modules: the hoimin mutation-testing
   skill explicitly excludes mutating test modules, so no hoimin run applies.
6. Perform the remaining three implementation and three test self-reviews,
   obtain independent review, commit, push the separate branch and create a PR.

## Design self-reviews

1. Root cause: inspected job 107497542605; the only failure is the literal
   version assertion. All Rust jobs on PR #578 succeeded. Wheel selection is
   not the failing code, so changing wheel package-version checks is unrelated.
2. Reproducibility: keep an exact manifest pin and existing CI installation
   commands. Accepting other exact stable pins does not select a floating
   compiler, modify MSRV, or relax nightly compatibility jobs.
3. Scope: use a new branch/worktree from main. The user's request authorizes the
   version update and contract correction; no Renovate branch mutation, closing
   the old PR, or main merge is needed.

## Plan self-reviews

1. Regression: the real 1.98.1 manifest must fail before replacing the equality.
   Fixture positives and negatives prevent either another hard-coded patch or
   a test that accepts arbitrary channel strings.
2. Preservation: keep profile/components/keys and artifact version equality.
   Include a multi-digit patch and suffix/whitespace cases to avoid partial
   regular-expression matches.
3. Verification: the failure occurs in Python's workflow contract suite, but
   an actual wheel smoke run also checks the packaged executable. Run relevant
   Rust quality gates under the new compiler; do not infer local success from
   the original PR's CI results.

## Execution and final reviews

Baseline: all 28 original workflow tests passed. Updating only the manifest to
1.98.1 reproduced the CI assertion failure at the original line 416. The four
new/updated toolchain tests then passed after the validator change.

### Implementation self-reviews

1. Source of truth: inspected the manifest and all remaining `1.98.0` references
   in tests/workflows/current development guide. Remaining literals are example
   fixtures only. Removed a second exact version assertion from the documentation
   contract after both the full suite and independent reviewer detected it.
2. Acceptance: the regex uses ASCII digits and absolute start/end anchors, with
   canonical zero handling in each component. It cannot match only a version
   prefix or accept a trailing newline. Installation/build gates still decide
   whether a numeric release exists and is compatible.
3. Preservation: reviewed the moved declaration assertions against the old code:
   exact keys, minimal profile and component multiplicity remain unchanged.
   No workflow, MSRV, wheel selection or package metadata check changed.

### Test self-reviews

1. Regression: the actual manifest update failed before the fix. The fixture
   matrix uses the same validator as the repository declaration and includes
   patch/minor updates and multi-digit patch values, not just the new release.
2. Rejection boundaries: floating/nightly, incomplete, wildcard, prerelease,
   build metadata, target suffix, leading zeros, surrounding whitespace and
   non-ASCII digits are rejected. Profile/component changes and extra declaration
   keys still fail. Wheel identity tests remain part of the full Python suite.
3. Environment and evidence: the first full run caught the second hard-coded
   documentation assertion. Four separate Lean guard cases could not inspect
   processes under the sandbox (`ps`: operation not permitted). After fixing the
   documentation assertion, the unchanged suite passed with normal OS access:
   95 tests, zero failures. The independent reviewer reran all 31 workflow tests
   successfully after the correction. No tests were skipped to obtain this result.

Setup notes: the toolchain installation succeeded, while rustup's subsequent
self-update failed; `rustc --version` confirms 1.98.1. An initial offline Cargo
check could not find newly merged main dependencies; `cargo fetch --locked`
downloaded clap_complete 4.6.11 and rustix 1.1.5 without changing Cargo.lock.
The subsequent CI Clippy command passed on Rust 1.98.1.

### Final verification

On macOS arm64, Rust 1.98.1 and CPython 3.14.7:

- `python -m unittest discover -s tests -p 'test_*.py' -v`: 95 passed.
  Log: `/tmp/hoimin-pr-578-python-final.log`.
- `cargo test --offline --workspace --all-features`: exit 0; 2,195 passed,
  zero failed, 22 ignored in 90 unfiltered summaries. Two successful
  subprocess-only summaries each add one passed test and are excluded from
  that count. Log: `/tmp/hoimin-pr-578-rust-tests.log`.
- `cargo clippy --offline --workspace --all-targets --all-features -- -D warnings`:
  exit 0. Explicit vendored parser `--lib --no-deps` Clippy also passed.
  Logs: `/tmp/hoimin-pr-578-clippy-final.log` and
  `/tmp/hoimin-pr-578-parser-clippy.log`.
- Workspace and vendored-parser `cargo fmt -- --check`, and `git diff --check`:
  exit 0.
- `maturin build --release --locked --offline --out target/wheels`: exit 0,
  producing a fresh macOS arm64 wheel. `python tests/wheel_smoke.py`: exit 0,
  including isolated installation, CLI invocation and actual mutation execution.
  Logs: `/tmp/hoimin-pr-578-wheel-build.log` and
  `/tmp/hoimin-pr-578-wheel-smoke.log` (empty on success).
- OKF checks: 25 concept YAML headers and four reserved indexes, changed catalog
  links and unique IDs, the doc-023 footnote and amended source SHA-256 passed.
- Independent review reran all 31 workflow tests after the documentation fix;
  no Critical, Important or Minor findings remain. Future release availability
  stays the installation/build gates' responsibility.

Cargo used two build jobs; Rust tests used two test threads. No Rust production
code, workflow or dependency lockfile changed. Local MSRV/nightly and native
Linux/Windows execution were not repeated; their existing CI gates remain.
