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
   channels, and missing/changed profile or components. The fixture tests check
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

Results will be recorded here after execution.
