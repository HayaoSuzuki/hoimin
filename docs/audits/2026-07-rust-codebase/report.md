# Final Rust Codebase Audit Report

## Executive summary

Audit in progress.

## Quality-gate baseline

The executable baseline was recorded on `2026-07-23` on macOS `15.7.7`
(`Darwin 24.6.0`, `arm64`). Formatting, Clippy with warnings denied, workspace
tests, core and CLI contract suites, Python tests, release build, wheel build,
wheel smoke, and the core dependency-purity assertion all completed with exit
status `0`. Complete local output is retained under
`.audit/rust-codebase/quality-gates/`; the exact command-to-log mapping is in
the audit README.

Sandboxed `uv` initially could not use `~/.cache/uv`, and the wheel smoke's
nested `uvx` initially could not write `~/.local/share/uv/tools`. Workspace
local cache reruns and an unrestricted wheel-smoke rerun passed; the initial
diagnostics remain in the evidence directory as execution-environment
limitations. Windows behavior and delegated Linux cgroup behavior were not
executed on this macOS host and remain `limited`, not passed.

The duplicate dependency report shows parallel versions in the random-number
and supporting dependency families. Baseline review did not establish a
concrete compatibility, binary-size, or security burden, so no finding was
opened from duplication alone.

## Findings by severity

Audit in progress.

## Cross-cutting risk themes

Audit in progress.

## Areas with no actionable findings

Audit in progress.

## Coverage limitations

Audit in progress.

## Remediation order

Audit in progress.
