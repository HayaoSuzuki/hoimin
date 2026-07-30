# MSRV Gate Design

## Goal

Make `workspace.package.rust-version` an executable compatibility contract.

## Design

CI gains an Ubuntu `msrv` job after quality. The current Ruff parser dependency
graph requires stabilized let-chains, so the declared MSRV is raised to Rust
1.88. The job installs exactly the manifest version and runs a locked
all-target, all-feature workspace check.
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
