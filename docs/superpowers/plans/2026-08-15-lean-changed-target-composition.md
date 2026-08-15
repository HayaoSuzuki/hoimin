# Changed-target range composition Lean audit plan

## 1. Baseline and worksheet

- [ ] Inventory Git parsing, range normalization, explicit resolution,
  intersection, analyzer filtering, and public plan seams.
- [ ] Assign every retained case strict, internal-fixture, model-only, or
  infrastructure-error before generating the corpus.

## 2. Lean semantics

- [ ] Model parsed change facts, inclusive ranges, exclusions, rename and
  untracked semantics, explicit selectors, and candidate observations.
- [ ] Prove normalization, intersection, subset, no-new-path, exclusion, rename,
  and untracked bounds properties.
- [ ] Keep proofs importable without cases, generators, or Rust fixtures.

## 3. Cases and correspondence

- [ ] Detect every required broken variant with minimized witnesses.
- [ ] Generate and freshness-check a closed typed corpus with bounded stats.
- [ ] Replay strict rows through real Git repositories and public plan output;
  keep parser-only evidence at an owned internal seam.

## 4. Repair and delivery

- [ ] For each same-premise mismatch, add a failing Rust regression first and
  make the smallest production repair.
- [ ] Write the audit report and counterexample ledger.
- [ ] Run resource-guarded Lean checks, focused and workspace Rust tests,
  formatting, strict clippy, and diff checks.
- [ ] Obtain independent review with no Critical or Important findings.
- [ ] Push, open a PR closing #313, wait for required CI, squash merge, and
  remove the dedicated worktree.
