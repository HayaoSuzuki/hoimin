# Typed results and contracts review

The approved [design](../superpowers/specs/2026-10-12-typed-results-and-contracts-design.md)
and [plan](../superpowers/plans/2026-10-12-typed-results-and-contracts.md) each
record five separate self-review passes. Implementation uses the existing
`refactor/typed-results-and-contracts` branch. A final independent review follows
implementation and validation.

## Python implementation self-reviews

1. Checked failure typing: expected grammar and conversion failures return
   `Failure[ValueError]`; adapters raise the contained error. No broad catch or
   global deal state change was introduced.
2. Checked grammar: canonical stable versions and stable-or-dev validation remain
   distinct. Unicode digits, leading zeroes, trailing newlines, and conversion
   limits have behavioral coverage. Validation does not acquire an integer limit.
3. Checked compatibility: existing messages, `SEMVER`, numeric ordering and
   callers remain intact. The focused release/provenance suite passed 129 tests
   with 10 platform skips on Windows.
4. Checked dependency setup: internal-tooling is pinned and included in dev and
   provenance. All four pre-build entry points install it before use. A fresh
   Python 3.14.6 environment ran help and set-version against copied manifests;
   provenance-only imports also passed. No tag or publication operation ran.
5. Checked strict typing and contracts: the incorrect-tuple sensitivity test
   raises `deal.PostContractError`. returns constructors expose `Any` on their
   unused generic side; a minimal reproduction ruled out annotation-only fixes.
   Narrow casts at these immutable construction boundaries preserve concrete
   outward types without suppressing ty. Repository-wide Ruff, format and ty
   checks passed, as did 172 workflow tests. Frozen uv audits found no known
   vulnerabilities/adverse statuses on Linux, macOS and Windows (78 packages).

New Python tests first failed because the interfaces were missing (5 failures,
3 passes); all eight passed after implementation. Bootstrap regressions first
failed in all four jobs and then passed. Installed returns 0.29.0 uses BSD-3-Clause
and deal 4.24.6 uses MIT. Neither was added as a product runtime dependency.

## Execution decisions

- Reused the existing clean work branch in this session rather than making a
  second checkout. Concurrent manual branch edits would require reconciliation.
- Used narrow constructor casts after reproducing the upstream generic typing
  limitation. A future returns typing change may make these casts unnecessary.

Further Rust, full-suite and independent-review evidence will be recorded here
before completion.
