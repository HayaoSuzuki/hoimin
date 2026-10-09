# Seeded verification sampling

`hoimin verify PLAN --sample N --seed S` requires positive N and an explicit u64
seed (including zero). The mode conflicts with candidate/top, offset and ranking
policy. Seed requires sample. No implicit entropy or default seed.

Population: the saved, validated candidate vector in rank order, across all score
tiers. Reject truncated or empty plans before extraction. Select min(N, population)
and reject if that actual count exceeds max_mutants; never silently cut the sample
to the execution budget. All existing plan/source/config validation still runs.

Algorithm `splitmix64_fisher_yates_v1`: initialize u64 state to seed. Each word adds
0x9e3779b97f4a7c15 with wrapping arithmetic, mixes xor-right-30 times
0xbf58476d1ce4e5b9, xor-right-27 times 0x94d049bb133111eb, then xor-right-31.
For bound b, reject words below (2^64 mod b), return word mod b. Initialize indices
0..population, and for i=0..actual swap i with i+draw(population-i). Return the prefix
in swap order. No hash iteration, native byte interpretation or rand crate version
enters the contract. Index storage is O(population); ID cloning is O(actual).

The bounded draw removes modulo bias for uniform independent words; SplitMix64 is
a deterministic pseudorandom generator, not a cryptographic generator or a proof
that all samples are equiprobable over the finite seed space. No statistical
confidence interval or real-defect guarantee is part of the feature.

Report metadata: mode=sample, policy=splitmix64_fisher_yates_v1,
scope=sampled_candidates, requested, selected, plan_truncated=false, and optional
sampling={population, seed, selected_ids}. Selected IDs retain dispatch order even
if execution is interrupted or workers finish out of order. Existing reports omit
sampling and deserialize with None. Update current JSON/event/preview schemas;
historical schemas stay historical. Complete means the selected run is complete;
timeouts, cancellation and not-run keep existing outcome semantics. Human output
explicitly identifies sample-only completion and score. Scheduling order is
reproducible; OS timing and parallel completion order are not promised.

Reuse the ordered verification path, plus its budget projection for sample mode.
Preview performs the same validation/selection without running test commands.
Progress retains candidate-ID/config comparability checks. Preserve the same plan,
N and seed for repeats; after changing tracked inputs regenerate the plan and
confirm the ordered selected IDs, or replay saved IDs with --candidate (which
uses its existing discovery order). Do not bypass fingerprint checks.

## Five design reviews

1. Determinism: specified word width, wrapping, rejection direction, vector order
   and forward shuffle; algorithm version includes all of these choices.
2. Boundaries: zero rejected by CLI, empty/truncated rejected, over-request clamped
   to population, budget checked against actual count before allocating indices.
3. Outcome honesty: selection IDs live in start and summary metadata; interrupted
   samples remain auditable, and out-of-sample IDs cannot become killed.
4. Compatibility: optional metadata preserves old reports; new enum values require
   updated consumers. Plan format and default ranking behavior stay compatible.
5. Validation scope: model proofs concern model invariants; generated corpus checks
   exercised Rust correspondence. Fixed-seed frequency review is descriptive, not
   a randomly failing CI assertion or proof of generator quality.
