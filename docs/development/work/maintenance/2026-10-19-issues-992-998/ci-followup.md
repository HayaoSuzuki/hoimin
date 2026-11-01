# Native CI followup: source-distribution manifest newlines

Base: `12e80440c1daec46c0e92895b123f34ef098f913` on the existing issue branch.
The controller owns independent review, push and hosted reruns. No runtime,
packaging configuration, toolchain policy or global Git setting changes here.

## Observed failure and correction

[Manual Windows wheel smoke job 117775662558](https://github.com/HayaoSuzuki/hoimin/actions/runs/39161527773/job/117775662558)
failed during full pytest, before the wheel build. Its result was 1071 passed,
32 skipped and one failure: the source archive's `hoimin-python/Cargo.toml`
contained LF while the Windows checkout contained CRLF. The first differing
byte was index 9. Copied parser/API source cases passed. The retained local CI
log is `/tmp/hoimin-issues-992-998/ci-windows-wheel-failure.clean.log`.

Maturin's actual source archive was reproduced from an isolated local checkout
whose Python crate manifest was explicitly converted to CRLF. That reproduced
exactly one failure and 26 passes, at the same byte. The owned temporary tree,
including its pytest outputs, was removed after each run. This reproduces the
newline condition on macOS; it is not a new Windows execution claim.

The test now compares that workspace manifest's text after CRLF-to-LF
normalization. Two cases exercise LF and CRLF expected checkout text against a
real Maturin archive. Every other byte remains significant. The copied vendored
manifest, Rust sources, README and license retain byte-exact comparisons;
missing/duplicate member assertions and untracked-output exclusion remain.
The previous Task 8 results describe their recorded revision and are preserved.

## Requirements self-review: five passes

1. Confirmed the exact failing native job, test, stage and byte difference from
   its actual log; this is a test contract failure before wheel construction.
2. Limited newline equivalence to `crates/hoimin-python/Cargo.toml`, the workspace
   manifest Maturin normalizes in the observed archive.
3. Preserved raw byte checks for copied parser/API inputs, including the
   explicitly included vendored Cargo manifest that passed native CI.
4. Retained archive presence/uniqueness and build/cache/scratch exclusion checks;
   no missing-source, duplicate-member or output-admission rule was relaxed.
5. Kept the bounded Cargo environment and sole local implementation ownership;
   no global autocrlf change, compatibility adapter, push or native claim.

## Design self-review: five passes

1. Chose only CRLF-to-LF equivalence; no trimming, whitespace folding, TOML value
   omission or broad semantic comparison can hide another content change.
2. Split workspace-manifest framing from copied-file byte preservation, so its
   exception cannot silently apply to parser files.
3. Parameterized the finite LF/CRLF dimension against the actual archive; no
   normalization helper is tested against itself.
4. Reproduced with copied Cargo workspace inputs and an explicit CRLF file in
   one temporary checkout, preserving the working tree and caller environment.
5. Updated current guide/OKF claims and the inspected test source revision/hash,
   leaving historical maintenance evidence and unrelated source metadata intact.

## Implementation self-review: five passes

1. Removed only the workspace Cargo manifest from the raw-byte parameter list;
   all copied-file expectations and archive fixture behavior are unchanged.
2. The dedicated test retains the same exactly-one member and readable-file
   checks before comparing the actual archive payload.
3. Expected content comes from checkout bytes; only line framing is varied in
   setup. Actual content is independently read from Maturin's generated tarball.
4. No archive rewriting, production packaging changes, dependency changes,
   schema changes, Rust edits or Git configuration changes were introduced.
5. Compared the final diff with the failure scope and verified the refreshed
   OKF source entry identifies the actual comparison commit and modified hash.

## Tests self-review: five passes

1. Native RED is the recorded Windows failure (1071 passed / 32 skipped / one
   failure); no later Windows success is inferred from local checks.
2. Local isolated CRLF RED reproduced one failed / 26 passed at byte 9 using
   genuine Maturin output; its exact temporary checkout was cleaned.
3. Test-first LF/CRLF cases initially gave one pass / one expected CRLF failure,
   before applying newline normalization.
4. GREEN is 28 actual archive cases passed in the normal checkout and 28 passed
   in the isolated CRLF checkout. Copied-source byte and exclusion cases ran in
   both. Missing/duplicate guards remain explicitly present in both comparisons.
5. Focused Ruff formatting/lint and full ty passed. YAML structure and affected
   source/hash/link checks are separate from the factual claim review below.
   No broad Rust or source-build rerun is needed for this test-only correction.

## Commands and results

Every Cargo-bearing test invocation inherited:

```sh
export CARGO_TARGET_DIR=/tmp/hoimin-issues-992-998/target
export CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export RUST_TEST_THREADS=2 PREK_CONCURRENCY=1 CARGO_NET_OFFLINE=true
export UV_CACHE_DIR=/tmp/hoimin-issues-992-998/uv-cache
```

The isolated checkout also used `RUSTUP_TOOLCHAIN=1.99.0`, matching the repository
pin outside the original worktree. Python remains the task `.venv`.

- Isolated CRLF reproduction before correction: one failed / 26 passed;
  `ci-sdist-crlf-red.log`.
- `pytest -q tests/test_sdist.py -k workspace_manifest` before normalization:
  one failed / one passed / 27 deselected; `ci-sdist-param-red.log`.
- `pytest -q tests/test_sdist.py`: 28 passed; `ci-sdist-green.log`.
- Isolated CRLF checkout after correction: 28 passed;
  `ci-sdist-crlf-green.log`. Both isolated runs completed exact-tree cleanup.
- `ruff format --check tests/test_sdist.py`, `ruff check --no-fix
  tests/test_sdist.py`, and `ty check`: passed.

All log names above are under `/tmp/hoimin-issues-992-998/`. The reproduction
script is local evidence only; it is not a product helper. Current documentation
claims were separately read against the test and observed archive behavior.
Final documentation checks and commit/hook outcome are recorded in the local
`ci-fix-report.md` handoff. Native rerun evidence remains controller-owned.

## Controller review and original-head hosted evidence

The focused independent review covered `12e80440..1162dc74` and found no Critical, Important or Minor issues. It did not repeat the already completed whole-branch review. The original runtime head passed [Linux CI](https://github.com/HayaoSuzuki/hoimin/actions/runs/39161512147), [three-OS package builds and archive smoke](https://github.com/HayaoSuzuki/hoimin/actions/runs/39161512080), [dependency audit](https://github.com/HayaoSuzuki/hoimin/actions/runs/39161512215) and [documentation](https://github.com/HayaoSuzuki/hoimin/actions/runs/39161512246). Linux Rust job117775889077 explicitly passed invalid-UTF8 regular-file, nested-directory and symlink meter tests, plus the public CLI generated-file/cleanup test. This evidence predates the test-only newline correction; updated-head hosted outcomes will be recorded on PR1000.

# Focused CI follow-up review

Reviewed base `12e80440c1daec46c0e92895b123f34ef098f913` through head `1162dc741734327a3f31492277f0c5ba3988c62f`, restricted to the workspace-manifest newline correction and affected documentation/source metadata. Read the brief, implementer report, prepared diff, actual Git diff, complete surrounding archive tests, native failure evidence, local RED/GREEN logs and commit-hook log. The checkout was reviewed read-only; no tests were rerun and no agents were dispatched.

### Strengths

- `tests/test_sdist.py:73` isolates CRLF/LF equivalence to the generated workspace manifest. Its assertion changes only CRLF sequences, preserving all other content differences without TOML reserialization, trimming or broad whitespace normalization.
- `tests/test_sdist.py:81` preserves the exactly-one-member and readable-member guards. The copied API, vendored Cargo manifest, Rust, README and license comparisons remain byte-exact (`tests/test_sdist.py:36`), and the real-archive fixture and untracked-output exclusion check are unchanged.
- The parametrized cases compare independently read real archive content with checkout-derived expectations and retain one case per execution. The separate isolated CRLF reproduction addresses the checkout/build condition that the parametrization alone does not recreate.
- The supplied logs show the native newline mismatch at byte 9; local reproduction shows one failure and 26 passes before correction, then 28 passes in both normal and explicitly CRLF checkouts afterward. No new native Windows success is claimed.
- Current development/release guidance and the knowledge page accurately describe the limited exception. The refreshed source hash matches `tests/test_sdist.py` (`7c0957d23274130c5b4083a9421cb6a0cdd5d0f84c2585562459f9658dfd116c`), its relative link/footnote resolves, and its base-revision plus modified-working-tree provenance is consistent with this change. Historical records remain intact.

### Issues

#### Critical (Must Fix)

None.

#### Important (Should Fix)

None.

#### Minor (Nice to Have)

None.

### Recommendations

No additional changes are required by this focused review.

### Declined to judge

- The seven original issue implementations and whole-branch runtime behavior: explicitly outside this follow-up review and already independently reviewed.
- Whether the corrected revision passes a new hosted Windows run: that execution remains controller-owned and pending; the local CRLF evidence is sufficient to assess this correction but is not hosted-run evidence.
- Future Maturin behavior beyond this observed newline rewrite: no new packaging/runtime policy is introduced, and the existing exact-content assertions will expose additional transformations.

### Assessment

**Ready to merge? Yes**, for the scoped CI correction.

**Reasoning:** The change directly addresses the demonstrated CRLF/LF mismatch while preserving the required archive membership, copied-file byte integrity and exclusion guarantees. The documentation matches the implemented contract, and the recorded real-artifact RED/GREEN evidence supports the correction without implying a native rerun result.

Controller dispositions for the review scope exclusions: the completed whole-branch review remains the evidence for the unchanged runtime; a new hosted Windows execution is required and will be performed after push; future Maturin transformations remain subject to the existing exact-content regression assertions and require investigation if they fail. No finding is deferred.
