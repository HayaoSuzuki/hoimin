# Rejected sampling compatibility regression trial

This bounded follow-up to the [#805/#807 assessment](../issue-805/README.md)
evaluated fixed known-answer checks, then rejected them as redundant. No sampler
or production test changes remain. Baseline:
`9debe7ef30ee0968f85e3a7c21282c973fafd548`; research parent: `36c0e84`.

## Design and execution plan

The `splitmix64_fisher_yates_v1` name promises a stable ordering. The sampler unit
count/range/uniqueness/prefix property alone does not enforce that version promise:
consistent PRNG drift can satisfy all four. The initial trial added three fixed
full permutations for zero, 42 and the maximum u64 seed, using population 8.
Calculate expected values independently with Python integer arithmetic and
explicit 64-bit masking. Preserve the rejection-boundary test; these small
vectors do not meaningfully exercise rare rejected draws.

Fixed examples are intentional here: randomly choosing compatibility vectors
would leave some versions/seed boundaries untested on individual runs.
Use separate AAA tests without conditional branches. No new product dependencies,
CLI fields, schemas, defaults or release version changes are required.

1. Derive the three vectors from the documented SplitMix64/forward Fisher-Yates
   steps; compare the Rust results only after fixing expected values.
2. Exercise the new assertions against a temporary source copy with the increment
   changed by one. Show that the old property passes but the vectors fail.
3. Run actual sampler unit tests through the isolated research crate, followed
   by strict Clippy and formatting. Avoid building the entire CLI for three pure
   sampler tests; CI will compile them in their normal package context.
4. Remove only the verified dedicated build directory via `cargo clean`, record
   disk space, update the knowledge/catalog references and obtain independent review.

### Design self-review, five passes

1. Checked the public versioned reproducibility statement and the unit property.
   This initial pass missed the integration oracle; independent review corrected it.
2. Selected complete ordered permutations, detecting ordering/PRNG drift rather
   than only membership.
3. Selected zero and full-width seeds around the explicitly wrapping arithmetic.
4. Kept generative properties and low-word rejection checks independent.
5. Kept the change test-only with three cheap fixed cases and no external ingress.

### Plan self-review, five passes

1. Fixed reference values before observing the Rust output.
2. Planned an artificial PRNG drift to distinguish the regression from the
   existing properties; it is not a real product bug.
3. Limited fault injection to a disposable copy, never the production source.
4. Selected the actual imported unit module for execution, avoiding duplicate
   hand-transcribed test bodies.
5. Required strict lint, scoped cleanup, source metadata and independent review.

## Independent review and final ruling

The independent reviewer found `sample_public_cli_matches_every_lean_oracle_case`
at `crates/hoimin-cli/tests/sampling.rs:270`. It checks all 360 committed
`formal/HoiminOracle/corpus/sampling.jsonl` cases through the public CLI, including
the three identical full permutations below and seed 1. The initial claim of a
repository-wide missing compatibility check was incorrect.

Ruling: remove the three additional unit tests and temporary serde_json probe
dependency. The assertions would be cheap focused redundancy, but they add no
new compatibility coverage. Under the requested test cost/benefit policy, retain
existing integration/reference checks rather than duplicate these vectors.
Production code and existing tests are unchanged in the final branch.

## Trial results (before removing redundant tests)

The Python calculation fixed these full permutations before the Rust comparison:

| Seed | Population/count | Ordered indices |
| --- | --- | --- |
| 0 | 8/8 | 7, 2, 3, 0, 1, 5, 4, 6 |
| 42 | 8/8 | 5, 6, 2, 7, 1, 0, 3, 4 |
| 18446744073709551615 | 8/8 | 0, 2, 3, 5, 6, 4, 7, 1 |

Reference calculation (original assessment code, not a formal independent spec):

```python
MASK = (1 << 64) - 1

def reference(seed):
    state = seed
    pool = list(range(8))
    for pos in range(8):
        bound = len(pool) - pos
        while True:
            state = (state + 0x9e3779b97f4a7c15) & MASK
            word = state
            word = ((word ^ (word >> 30)) * 0xbf58476d1ce4e5b9) & MASK
            word = ((word ^ (word >> 27)) * 0x94d049bb133111eb) & MASK
            word ^= word >> 31
            if word >= (1 << 64) % bound:
                break
        choice = pos + word % bound
        pool[pos], pool[choice] = pool[choice], pool[pos]
    return pool
```

In the disposable copy, the SplitMix64 increment was changed from
`0x9e37_79b9_7f4a_7c15` to `0x9e37_79b9_7f4a_7c16`. The pre-existing generative
property passed (1/1); all three new compatibility tests failed with output
mismatches, not compilation errors. This is an artificial version-drift example,
not a previously occurring product bug or a measurement of fuzzing effectiveness.

Against the unchanged implementation, the actual imported sampler unit module
passed **5 tests**, with the pre-existing descriptive frequency inspection
**ignored**. That includes the three fixed vectors, the rejection-boundary test
and the existing generative property. Runtime was about 0.03 seconds after build.
`cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` passed for the
standalone probe. The entire CLI integration suite was not rerun locally.

```powershell
cargo test --locked --manifest-path docs/superpowers/reports/issue-805/probe/Cargo.toml --target-dir .superpowers/issue-805-target -j 1
cargo clippy --locked --all-targets --manifest-path docs/superpowers/reports/issue-805/probe/Cargo.toml --target-dir .superpowers/issue-805-target -j 1 -- -D warnings
```

During the trial, a test-only serde_json dependency enabled the imported
pre-existing test module to compile. It and the resulting lockfile changes were
removed with the redundant tests. The final probe retains its original 67
dependency packages and is not wired into CI or release artifacts. The two
commands above describe the temporary test-enabled trial configuration; they
are not supported reproduction commands for the final probe. Reproduce the
retained random adapters with the commands in the #805 assessment instead.

### Implementation self-review, five passes

1. Verified the product algorithm and constants remain unchanged.
2. Compared all three literals to the independently evaluated Python calculation.
3. Checked each new test has arrangement, one call and a literal-output assertion,
   with no branches or helper oracle that could silently change with the sampler.
4. Checked test-only serde_json stays in the standalone research workspace and
   exists only to compile the imported pre-existing test module.
5. After independent review, matched all proposed vectors to the existing Lean
   corpus and removed the redundant implementation and test-only dependency.

### Test self-review, five passes

1. Confirmed the artificial drift still passes the old property, demonstrating
   the specific missing compatibility check.
2. Confirmed all three new tests fail on that drift with concrete output diffs.
3. Confirmed all five normal sampler tests pass on the real unchanged algorithm.
4. Confirmed the ignored frequency inspection was not executed or counted as a pass.
5. Confirmed strict lint/format checks and recorded the trial execution scope;
   no new repository coverage is claimed. Existing full CLI tests were read, not rerun.
