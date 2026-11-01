# Issue #283 Windows Shutdown Oracle CFG Design

## Problem

The strict shutdown correspondence adapter is Unix-only, but its supporting
imports, result types, and process fixture helpers are compiled on Windows.
Windows clippy therefore reports them as unused or dead code under
`-D warnings`, blocking the quality matrix before later jobs run.

## Decision

Apply `#[cfg(unix)]` at the narrow ownership boundary: imports and definitions
used only by `oracle_correspondence` and its real signal/process fixture. Keep
corpus deserialization, validation, strict projection, and panic/teardown tests
cross-platform. Do not add blanket warning suppression and do not pretend the
Unix signal fixture runs on Windows.

## Verification

The failing Windows CI command is the RED evidence. Local Unix clippy and
shutdown oracle tests must remain green, and the PR quality matrix must prove
the Windows configuration is warning-free.
