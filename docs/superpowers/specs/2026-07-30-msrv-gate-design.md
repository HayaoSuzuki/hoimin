# MSRV Gate Design

## Goal

Make `workspace.package.rust-version` an executable compatibility contract.

## Design

CI gains an Ubuntu `msrv` job after quality. It installs exactly Rust 1.85 and
runs `cargo +1.85 check --workspace --all-targets --all-features --locked`.
Stable quality and test jobs remain unchanged so current diagnostics and
platform behavior continue to be covered.

The committed lockfile is part of the MSRV contract. Dependency updates must
keep the locked graph compilable on the declared compiler. If an update cannot
do so, maintainers either select the newest compatible dependency release or
raise the MSRV deliberately in the manifest, CI job, workflow contract test,
and development guide in one pull request.

The workflow contract test parses `Cargo.toml` and compares its rust-version to
the toolchain and cargo command in the `msrv` job. This prevents a silent drift
between documentation and execution.

