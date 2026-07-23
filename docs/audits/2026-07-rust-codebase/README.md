# Rust Codebase Audit — July 2026

Branch: `audit/rust-codebase-2026-07`
Base commit: `4a93b2720ee4be3ef9c0664b4c8b6116776eabc9`

## Status

`in_progress`

## Evidence conventions

- Command evidence records the command, host platform, exit code, and output path.
- Code evidence names exact files and line numbers at the base commit.
- `confirmed bug` requires a reproduction or a violated executable contract.
- `high-risk design` requires a concrete failure path and an unenforced invariant.
- `maintainability` requires a proposed boundary and characterization strategy.
- Platform behavior not executed locally is marked `limited`, never `pass`.

## Finding identifiers

Use `RUST-AUDIT-NNN`, assigned monotonically when a lead first enters the findings ledger.
Rejected leads keep their identifiers so later rows and issue links never shift.

## Artifacts

- [Coverage matrix](coverage.md)
- [Findings ledger](findings.md)
- [Issue map](issues.md)
- [Final report](report.md)
