# Standalone Wheel-Smoke Contract Design

## Context

`tests/wheel_smoke.py` consumes an existing wheel from `target/wheels`, or the
exact path named by `HOIMIN_WHEEL`. It does not build wheels. The README already
documents `maturin build` and the smoke script as separate commands, while
`docs/development.md` incorrectly says the script builds automatically.

## Decision

Keep wheel construction and smoke testing as separate operations. A standalone
smoke run without `HOIMIN_WHEEL` requires a compatible wheel already present in
`target/wheels`. This preserves the current script and CI behavior, avoids
hidden network or build work, and lets callers smoke-test an explicitly selected
artifact.

## Documentation and regression coverage

- Update `docs/development.md` to state the prerequisite explicitly.
- Keep the README command sequence unchanged and verify both documents describe
  the same contract.
- Add a subprocess test that executes a copied standalone script against an
  empty repository-shaped directory. It must fail with the existing
  `build a wheel first` diagnostic and must not create a wheel directory.

## README impact

No README text change is needed: it already shows the required build command
immediately before the standalone smoke command.
