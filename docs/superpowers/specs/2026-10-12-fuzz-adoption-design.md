# Bounded fuzzing adoption assessment

Assess #805 and #807 against main `9debe7ef30ee0968f85e3a7c21282c973fafd548`.
This is a research spike, not approval for a new product execution interface.
Use original probes; do not copy HypoFuzz or Fuzzing Book implementations.

## Questions and decisions

1. #803: distinguish compressed cache size from corpus replay cost. Read a
   successful existing CI artifact before provisioning a new Linux fuzz build.
   If replay is material, retain a precisely scoped follow-up; do not claim that
   corpus merging preserves feature sets without measuring them.
2. #802: compare the existing sampling properties with a dependency-free shared
   oracle and a Bolero adapter. Use identical bounded inputs for the probe.
   Preserve independent versioned sampling regression tests. A Windows random
   engine comparison cannot establish Linux libFuzzer performance or coverage.
3. #804: inspect `ci_selection` first. If its finite classification space and
   existing examples expose no difficult search boundary, stop without adding
   `target()` or instrumentation elsewhere.
4. #807: identify an actual maintenance deficiency or explicitly record none.
   Reuse the child investigations above rather than launching a second corpus
   or property project. Do not turn research into a new user-facing feature.

## Budgets and evidence

- One worker; Python 3.14; existing resource guards remain unchanged.
- Download only the selected 26,667-byte diagnostic artifact, not corpus caches.
- Rust adapter probe: setup/build at most 10 minutes; each run at most 60 seconds;
  dedicated target directory, clean it afterwards. Stop on missing platform
  support or setup budget exhaustion; record unavailable comparisons honestly.
- No Linux toolchain installation just for this spike. WSL exists but has no
  `cargo`, `rustc`, or `clang` on the checked paths.
- Artificial oracle sensitivity checks are not discoveries of product bugs.
- Do not add mandatory fuzz jobs, dependencies, or a product version bump for
  research-only material. #806 and previously closed solver work remain separate.

## Design self-review, five separate passes

1. Scope: checked #805/#807 and their child boundaries; excluded #806 and closed
   survivor-generation/Kani experiments.
2. Safety: limited downloads and workers; avoided changing runtime guards or
   introducing product input importers.
3. Comparability: required identical probe inputs/oracles and separated random
   testing from coverage-guided testing and proof.
4. Evidence: separated cache bytes, replay estimates, feature identities,
   artificial faults, real failures, and unexecuted comparisons.
5. Maintenance: negative adoption decisions are valid; shared checks must not
   displace independent versioned sampling regressions.
