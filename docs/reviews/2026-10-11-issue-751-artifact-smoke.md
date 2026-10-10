# Issue #751 packaged artifact smoke

Base: `79b6bde29b05e9904c8efff59e02d0c45377d7f7`.
Branch: `ci/issue-751-artifact-smoke`.

## Design and implementation plan

1. Reuse wheel fixture/execution assertions for an explicit executable path.
2. Read only the known four-file release archive; copy its regular executable to
   a temporary directory, without a general archive extraction interface.
3. Add read-only matrix jobs consuming verified-release, outside the build tree.
4. Run the existing wheel smoke in pinned manylinux2014 with CPython 3.14 and
   auditwheel ABI evidence; preserve existing host wheel checks on all OSes.
5. Record artifact identity/runtime evidence and gate attestation/publication on
   smoke success. Check focused failures, existing release tests and strict lint.

## Design self-reviews

1. Smoke consumes validated bytes and cannot fall back to a build/PATH executable.
2. Existing wheel fixtures are reused, not a new synthetic mutation workload.
3. Linux standalone Ubuntu 22.04 and wheel glibc 2.17 runtime claims stay distinct.
4. Three existing OS targets only; the Linux ABI job fills the missing boundary.
5. Publishing credentials remain isolated; smoke jobs have contents:read only.

## Plan self-reviews

1. Known regular archive members avoid extract-all path/link handling entirely.
2. Absolute executable/Python paths and external temporary directories prevent checkout imports.
3. Digest/name/version/OS evidence is retained for success and failure diagnosis.
4. manylinux image is digest-pinned; interpreter and glibc are checked, not assumed.
5. Existing publication regression checks will enforce the added success dependency.

## Implementation self-reviews

1. Known-member checks precede copying bytes; no member path or link is extracted.
2. The shared fixture checks exact version, help, JSON mutation result and source integrity.
3. Host wheel evidence is added to existing execution, without repeating it on every archive job.
4. Linux separately records Ubuntu standalone symbols and executes the wheel at verified glibc 2.17.
5. Artifact/evidence names stay separate; attestation and its downstream publisher require both smoke successes.

## Verification self-reviews

1. Reject missing binaries, wrong versions, duplicate/unexpected members and links.
2. Test publication dependency removal as a failure, preserving OIDC separation.
3. Run existing wheel/release contracts, strict workflow lint and Python type checks.
4. Verify actual Windows archive execution locally and require hosted preview for Linux/macOS.
5. Check OKF/hash/link integrity, disk cleanup and independent review; no large mutation campaign for test helpers.

## Results

Related tests (archive/wheel helpers, CI publication contracts, release tooling):
255 passed, 11 skipped on Windows. The Unix venv symlink regression is one
platform skip and must run in Linux CI. Initial contract failures were updated
for evidence upload and added smoke gates; a duplicate YAML env in the hostile
token fixture was corrected so it tests an actual injected token. Overlapping
early test runs exceeded an existing 60-second Lean shell fixture timeout; the
fixture passed alone and the final serial suite passed without changing it.

Strict actionlint/ShellCheck/zizmor, repository Ruff format/check, and repository
ty passed. Supplementary mypy passed for the three smoke modules; a broader
mypy invocation lacks existing PyYAML stubs (ty is the repository CI type gate).
OKF 36 pages, changed local links/source hashes and whitespace passed.
No production Python or Rust mutation engine code changed; these are test/CI
helpers, checked directly without mutating test modules or adding a large campaign.

Published Windows v0.3.8 archive and wheel both passed the shared outside-checkout
mutation fixture on Python 3.14.6. Name, digest and runtime evidence is committed
in `2026-10-11-issue-751-windows-evidence.json`; no package version floor changed.
Independent review found an interpreter-symlink bug, fixed with lexical absolute
Python paths and a Unix regression. Follow-up review found no remaining issues.
`cargo clean` found no retained outputs; disk free space was 176.3 GiB.
Hosted Linux/macOS/glibc-floor preview remains to be checked after pushing this PR.

First hosted preview (`38074451400`) exposed Rust preview `-dev.N` versus Python
metadata `.devN` spelling in the newly strict version assertion. A failing
regression reproduced it locally; comparison now uses normalized version values
while still checking the command identity and rejecting another version.
The fix is limited to the shared test helper. Existing artifact identity,
absolute-path execution, rejection tests and public version floor remain unchanged.
Fix verification: 110 focused tests passed, one Unix-only skip; repository Ruff
and ty, three-module mypy, OKF links/hashes and whitespace passed. Independent
follow-up review accepted the normalization and found no issues.
