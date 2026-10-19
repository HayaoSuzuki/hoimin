#!/bin/bash
# Build the pinned Creusot tools after Why3 is available.
set -euo pipefail
cd /opt/creusot/src/creusot/*
# Cargo.lock checksums pin registry dependencies; --locked prohibits re-resolution.
cargo build --locked --release --bin cargo-creusot --bin creusot-rustc --bin prelude-generator --jobs 2
install -m755 target/release/cargo-creusot target/release/creusot-rustc /opt/creusot/rust/bin/
mkdir -p /opt/creusot/data/share/why3find/packages/creusot/creusot /opt/creusot/data/toolchains
ln -s /opt/creusot/rust /opt/creusot/data/toolchains/nightly-2026-08-03
ln -s /opt/creusot/opam-switch/_opam/bin/why3 /opt/creusot/data/bin/why3
ln -s /opt/creusot/opam-switch/_opam/bin/why3find /opt/creusot/data/bin/why3find
