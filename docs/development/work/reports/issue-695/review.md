# Issue 695: review and verification record

Implemented on `feat/sampling-with-seed`. The assessment, design and plan each
contain five review passes. The ten implementation/test passes below record
findings and their resolution. No source operator, default selection policy or
plan schema changed.

## Five implementation self-reviews

1. **CLI precedence:** tested missing seed, seed without sample, zero, negative and
   overflowing seeds, top/ID conflicts, offset zero and ranking policy. Found that
   clap's `requires` did not enforce the intended dependency inside the selection
   group. Added explicit sample/offset/policy and seed/top/ID conflicts; the failing
   CLI cases now pass. Corrected the touched plan help from version 4 to version 5.
2. **Sampler arithmetic:** checked wrapping constants, right shifts, threshold
   `2^64 mod bound`, forward swap endpoints and count clamping. `bound` cannot be
   zero in the loop; the converted offset fits usize and the remaining suffix.
   Verified rejection with deliberately supplied low words, including bound 1 and
   u64::MAX. The plan's saved vector supplies the only input order.
3. **Validation and execution:** traced header/config checks, source/fingerprint
   comparison, ranking validation, rediscovery and ordered scheduling. Sample
   rejects truncation/empty/budget before allocating the index vector. Tests cover
   duplicate IDs, altered ranks and stale sources. The sample metadata reaches
   preview, start and summary, including selected IDs absent from execution.
4. **Reports and compatibility:** inspected JSON, JSONL, human output and old report
   fixtures. Existing literals supply `sampling: None`; serde omits it on output
   and defaults it on input. New schemas condition sample metadata on sample mode.
   Old consumers that reject new enums need updating. Human output states that
   score and completion apply to the sample. An independent read-only review found
   a stale “verify top budget projection” label; changed it to “verify budget
   projection”. No substantive runtime defect was reported in that review.
   The final incremental review approved the completed changes without findings.
5. **Resources and maintainability:** kept the O(population) index vector separate
   from candidate bodies and cloned selected IDs only. Extracted sample resolution
   and human selection rendering when clippy exposed overlong functions. One
   pre-existing lint failure in `lean_report_sequence_oracle::observed_error`
   required a scoped `too_many_lines` allowance for its exhaustive error mapping;
   the adapter's behavior is unchanged. No new runtime dependencies.

## Five test self-reviews

1. **Red/green and boundaries:** first CLI test failed with unknown `--sample`.
   Subsequent negative tests exposed the clap dependency issue above. The public
   suite checks actual count versus budget, population overflow, extreme seed,
   stable prefix, preview/execution agreement and dry-run without baseline effects.
2. **Independent expectations:** Lean supplies all 360 public-oracle cases. The
   adapter maps model indices to the saved plan's IDs and executes real CLI
   validation; it contains no second PRNG. Separate properties check uniqueness,
   membership, cardinality and deterministic prefixes over 4,096 generated inputs
   with `PROPTEST_RNG_SEED=695`. Distribution checks have no statistical CI gate.
3. **Unfinished runs and comparison:** actual Python timeout and failed-baseline
   tests retain provenance and incomplete outcomes. Core transition tests inject
   cancellation/deadline and observe two not-run results, zero kills and incomplete
   summary. Repeated identical samples remain comparable. A changed-sample test
   initially assumed differing IDs always produce added/removed counts; existing
   logical matching can instead mark candidates ambiguous. The corrected test
   checks changed-set evidence and `indeterminate`, preserving that contract.
4. **Adversarial inputs and schemas:** libFuzzer runs the production sampler over
   populations 0..4096, counts 0..65535 and arbitrary u64 seeds. The first 31-second
   campaign completed 999,275 inputs without a failure (466 MiB reported RSS).
   Corpus files cover empty, partial and large requests; a follow-up campaign uses
   the same seed directories as CI and completed 615,968 inputs in 11 seconds
   without failure (419 MiB reported RSS). Draft 2020-12 validation accepted ten actual
   preview/report/events and rejected seven malformed metadata variants; see
   [schema-validation.json](schema-validation.json). JSONL contains start/finish
   events as well as mutant events, so selection provenance survives interruption.
5. **CI and evidence:** verified the executable's optional leading `--`, which the
   CI generator runner supplies; added the normalization before running the exact
   `lake exe generate_sampling -- --check ...` command. Added the fuzz target to the
   existing campaign expectation after observing its failure. Also registered the
   Lean generator in the CI contract test and matched its required manifest order.
   Python production
   code did not change, so there is no new Python behavior to mutation-test.
   The normal Rust and Python suites, format and strict clippy are the completion
   checks: Rust 2,720 passed (24 ignored), Python 504 passed (3 skipped), format
   and strict clippy passed. The two new ignored measurement tests were run
   explicitly. Initial Python process-guard failures came from sandbox `ps`
   restrictions; the final run with process visibility passed. Final results are
   in [verification.json](verification.json).

## Formal boundary and correspondence

Claim: a selected ordered prefix contains no repeated candidate and preserves
population membership; its size is the clamped request. Accepted sample requests
have a nonempty complete population and fit the execution budget.

| Premise/observation | Lean | Production/public observation | Mode |
| --- | --- | --- | --- |
| Population 0, 1, 8 | range indices | Authored Python plans with exactly these sizes | strict |
| Counts 0, 1, 3, 8, 99 | Nat count | CLI count and acceptance/rejection | strict |
| Budgets 1, 2, 8; truncated false/true | accepts | Plan config and dry-run exit 0/2 | strict |
| Seeds 0, 1, 42, u64::MAX | UInt64 state | Explicit CLI seed and ordered IDs | strict |
| Swap/prefix invariant for arbitrary arrays | Array.Perm, Nodup | No unrestricted production equivalence claim | model-only |
| 128-draw fuel | Option exhaustion | Production has no retry cap; corpus aborts on exhaustion | model-only |
| 16-word modulo example | Enumerated residues | Deliberately reduced word space | model-only |
| Rejected low words | Rust closure seam | Internal bounded-draw unit test | internal-fixture |
| First RSS monitor attempt | Monitor unavailable in sandbox | No semantic conclusion; rerun with process visibility | infrastructure-error |

Kernel-checked theorems establish swap and shuffle permutation preservation,
cardinality preservation, unique prefixes, prefix length and accepted-budget /
complete-population predicates. They do not prove the Rust implementation, PRNG
quality, uniformity over finite seeds, rejection-loop termination for all seeds,
OS scheduling, timing, filesystem integrity or test quality.

The corpus is a finite Cartesian boundary domain of 360 cases, not a proof over
every plan/seed. All exercised cases match the real public CLI. The executable
fails if rejection fuel is exhausted; it cannot silently emit a truncated draw.
Broken controls detect replacement duplicates (`[0,0]` versus `[0,1]`), ignored
truncation (population 2, count 1, budget 2), budget clipping (2,2,1), and reduced
modulo bias (16 words into three buckets: 6/5/5 before rejection, 5/5/5 afterward).
Atomicity of external effects is outside the pure sampler; the separate dry-run
and failed-validation tests check that execution does not start.

Retained limits: 20 seconds per Lean invocation, 2 GiB RSS, 250 ms sampling,
20,000 heartbeats per theorem and 128 model draw attempts. Proof-only run: 3.543 s,
667,568 KiB peak process-tree RSS. Executable build plus corpus freshness: 5.449 s,
706,352 KiB. No larger exhaustive search was attempted. There is no unresolved
semantic mismatch or owner decision.

## Fixed-seed and authored-project measurements

[frequencies.json](frequencies.json) records 65,536 seeds, population eight and
sample size three. Candidate inclusion counts range from 24,351 to 24,798, around
the nominal 24,576. Position counts range from 7,972 to 8,416, around 8,192.
This inspects one small domain and does not establish statistical independence or
representativeness. CI does not randomly accept/reject a distribution threshold.

[observations.json](observations.json) contains the source text/hashes, operators,
test programs, full-population reference outcomes, selected IDs, counts, elapsed
time and score errors for two authored Python projects. All runs use jobs=1,
`boolean_literal,binary_add_sub`, and seeds 0, 1, 42 and u64::MAX. There are four
seeds per requested size, with sizes two and four. All measurements had zero
inconclusive/timeout/error/not-run outcomes.

| Fixture | Population / full score | Full seconds | Sample | Sample seconds | Score error range |
| --- | --- | ---: | ---: | --- | --- |
| constants | 8 / 0.5 | 0.243 | 2 | 0.103–0.116 | 0 to +0.5 |
| constants | 8 / 0.5 | 0.243 | 4 | 0.142–0.150 | 0 |
| functions | 6 / 0.667 | 0.197 | 2 | 0.102–0.120 | -0.167 to +0.333 |
| functions | 6 / 0.667 | 0.197 | 4 | 0.150–0.161 | -0.167 to +0.083 |

The sample outcomes match each corresponding full-run candidate. The examples
demonstrate the cost/error tradeoff and the danger of treating a sample score of
1.0 as a population score of 1.0. They are small authored fixtures on macOS arm64,
not representative production projects or real-defect experiments. One repetition
per seed/count includes startup, baseline and execution; concurrent development
checks can affect timing. No general speedup or confidence interval is claimed.

## Reproduction

From the repository root:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
.venv/bin/python -m pytest -q
PROPTEST_CASES=4096 PROPTEST_RNG_SEED=695 cargo test -p hoimin-cli --lib ordered_sample_is_a_unique_bounded_reproducible_prefix
cargo test -p hoimin-cli --test sampling
cargo test -p hoimin-core --test machine sample_cancellation
cargo test -p hoimin-cli --test report_handler sample_public_reports_and_previews
cargo test -p hoimin-cli --test sampling sample_authored_project_evaluation -- --ignored
cargo test -p hoimin-cli --lib sample_fixed_seed_frequency_inspection -- --ignored --nocapture
cargo +nightly-2026-10-08 fuzz run sampling fuzz/corpus/sampling fuzz/seeds/sampling -- -max_total_time=30 -timeout=3 -rss_limit_mb=2048 -seed=695
```

From `formal/HoiminOracle`, use the resource guard for each command (process-tree
inspection requires OS permission):

```sh
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 250 --stats /tmp/sampling-lean.json -- lake exe generate_sampling -- --check corpus/sampling.jsonl
lake exe generate_sampling -- --sensitivity
lake exe generate_sampling -- --stats
```

To regenerate the committed oracle, replace `--check` with `--output`; the public
adapter then reruns all generated cases. Keep empirical measurements separate
from deterministic oracle expectations.
