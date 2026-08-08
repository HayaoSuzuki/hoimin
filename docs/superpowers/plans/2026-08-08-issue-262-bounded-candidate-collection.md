# Issue 262 Bounded Candidate Collection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bound per-file retained mutation candidates proportionally to
`max_candidates` without changing the analyzer's exact emitted prefix or
diagnostics.

**Architecture:** Each of the three deterministic candidate producers keeps a
bounded stable prefix of `max_candidates + 1` eligible unique candidates. The
existing producer-order merge, cross-producer deduplication, stable sort, and
final truncation remain the public semantic boundary.

**Tech Stack:** Rust 1.88+, `BinaryHeap`, bounded `HashSet`, Ruff Python AST and
parser, existing analyzer unit/integration tests.

## Global Constraints

- Retain at most `max_candidates.saturating_add(1)` candidates and identities
  per producer.
- Preserve candidate descriptors, IDs, ordering, deduplication identity,
  diagnostics, profile behavior, and cancellation semantics.
- Apply focused-profile filtering before a candidate consumes prefix capacity.
- A zero remaining limit retains one eligible candidate only for overflow
  detection and emits no candidate.
- Do not add dependencies, CLI options, schema fields, spool changes, or
  resource-backend changes.
- Behavior commits do not use `[skip ci]`; pure documentation commits do.

---

### Task 1: Add a bounded stable producer prefix

**Files:**

- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`
- Test: `crates/hoimin-cli/src/analyzer/rust_tests.rs`

**Interfaces:**

- Produces: `CandidatePrefix::new(max_candidates)`,
  `CandidatePrefix::push(candidate)`, and `CandidatePrefix::finish()`.
- Produces: `ProducerPrefix { candidates, overflowed, retained_peak }` for the
  producer integrations in Task 2.
- Consumes: the existing candidate ordering `span.start`, then `operator`, with
  producer-local emission order as the stable tie breaker.

- [ ] **Step 1: Write failing bounded-prefix tests**

  Add unit tests that construct candidates out of input order and with exact
  duplicates. Assert capacity `k + 1`, retained identity count, stable tie
  order, earlier-candidate replacement, overflow detection, `k = 0`, and
  `usize::MAX` saturation. A representative assertion is:

  ```rust
  let mut prefix = CandidatePrefix::new(2);
  for candidate in candidates_with_starts([9, 1, 5, 3, 1]) {
      prefix.push(candidate);
  }
  let result = prefix.finish();
  assert_eq!(candidate_starts(&result.candidates), vec![1, 3, 5]);
  assert!(result.overflowed);
  assert_eq!(result.retained_peak, 3);
  ```

- [ ] **Step 2: Run the new tests and record RED**

  Run:

  ```bash
  cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::candidate_prefix -- --nocapture
  ```

  Expected: compile failure because `CandidatePrefix` and `ProducerPrefix` do
  not exist.

- [ ] **Step 3: Implement the bounded prefix**

  Add an internal retained entry with an owned candidate and emission sequence.
  Implement `Ord` so the heap maximum is the latest candidate under:

  ```rust
  candidate.span.start
      .cmp(&other.candidate.span.start)
      .then_with(|| candidate.operator.cmp(&other.candidate.operator))
      .then_with(|| emission_sequence.cmp(&other.emission_sequence))
  ```

  Keep only identities for heap-resident candidates:

  ```rust
  type CandidateIdentity = (u64, String, String);
  // (span.start, replacement, operator)
  ```

  On a full prefix, skip an exact retained duplicate; otherwise set
  `overflowed = true` and replace the heap maximum only when the new entry
  sorts earlier. Remove an evicted identity before inserting the replacement.
  `finish()` must return ascending stable order using the heap's sorted output.

- [ ] **Step 4: Run focused tests and static checks**

  Run:

  ```bash
  cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::candidate_prefix -- --nocapture
  cargo fmt --all -- --check
  cargo clippy -p hoimin-cli --all-targets --all-features -- -D warnings
  git diff --check
  ```

  Expected: all pass and no production call site changes yet.

- [ ] **Step 5: Commit Task 1**

  ```bash
  git add crates/hoimin-cli/src/analyzer/rust.rs \
    crates/hoimin-cli/src/analyzer/rust_tests.rs
  git commit -m "perf: add bounded candidate prefixes"
  ```

---

### Task 2: Bound all candidate producers and preserve output

**Files:**

- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`
- Test: `crates/hoimin-cli/src/analyzer/rust_tests.rs`
- Test: `crates/hoimin-cli/tests/rust_analyzer.rs`

**Interfaces:**

- Consumes: `CandidatePrefix` and `ProducerPrefix` from Task 1.
- Produces: token, AST, and type-annotation producer prefixes with unchanged
  `AnalyzerOutput` semantics.
- Produces under `cfg(test)`: local `CandidateRetentionStats` carried by
  `AnalyzerOutput`, containing the three producer peaks and merged peak.

- [ ] **Step 1: Write failing end-to-end retention tests**

  Generate a source with hundreds of selected token expressions, collection
  calls/literals, and supported annotations. Analyze it once with a limit above
  its full candidate count and once with `max_candidates = 3`. Assert:

  ```rust
  assert_eq!(bounded.candidates, full.candidates[..3]);
  assert_eq!(bounded.diagnostics[0].code, CandidateLimitExceeded);
  assert!(bounded.truncated);
  assert!(bounded.retention.producer_peaks.iter().all(|peak| *peak <= 4));
  assert!(bounded.retention.merged_peak <= 12);
  ```

  Add fixtures proving that many focused-filtered arid candidates before an
  eligible candidate do not fill the prefix, and that a zero remaining limit
  returns no candidates with the unchanged overflow diagnostic. Retain the
  existing stable-tie candidate assertions.

- [ ] **Step 2: Run the retention tests and record RED**

  Run:

  ```bash
  cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::bounded_collection -- --nocapture
  ```

  Expected: compile failure because retention statistics and producer
  integration do not exist, or an assertion showing the current peak is
  proportional to all candidates rather than the limit.

- [ ] **Step 3: Move focused filtering to the producer boundary**

  Extract the existing predicate without changing its fallbacks:

  ```rust
  fn retained_by_profile(
      candidate: &AnalyzerCandidate,
      profile: MutationProfile,
      facts: &AstFacts<'_>,
  ) -> bool
  ```

  Type operators return `true`; failed span conversion remains conservative
  and returns `true`; other candidates are rejected only when their exact span
  is arid. Call this predicate immediately before every producer prefix push.

- [ ] **Step 4: Integrate token and AST producers**

  Replace the token `Vec::push` path with a `CandidatePrefix`. Change
  `AstCandidateCollector.candidates` to `CandidatePrefix`, initialize it from
  `request.max_candidates`, and make `collect` return `ProducerPrefix`.
  Preserve cancellation checks before token work and throughout AST visitor
  traversal.

- [ ] **Step 5: Integrate the type-annotation producer**

  Replace the final iterator `.collect()` with an explicit traversal that
  checks every generated candidate with `retained_by_profile` and pushes it
  into its own prefix. Return `ProducerPrefix` in the same annotation and
  replacement emission order.

- [ ] **Step 6: Merge prefixes and compute truncation**

  Concatenate token, AST, and type candidates in that order. Preserve the
  existing global identity retain and stable comparator exactly. Set:

  ```rust
  let truncated = producer_overflowed || candidates.len() > request.max_candidates;
  candidates.truncate(request.max_candidates);
  ```

  Populate test-only retention statistics from the producer high-water marks
  and the maximum combined retained length before final truncation. Invalid
  syntax returns zeroed statistics.

- [ ] **Step 7: Run analyzer and integration regressions**

  Run:

  ```bash
  cargo test -p hoimin-cli --lib analyzer::rust::rust_tests -- --nocapture
  cargo test -p hoimin-cli --test rust_analyzer
  cargo test -p hoimin-cli --test analyzer_handler
  cargo fmt --all -- --check
  cargo clippy -p hoimin-cli --all-targets --all-features -- -D warnings
  git diff --check
  ```

  Expected: exact prefix, focused filtering, cancellation, parseability,
  runtime/planning descriptor parity, and all static checks pass.

- [ ] **Step 8: Commit Task 2**

  ```bash
  git add crates/hoimin-cli/src/analyzer/rust.rs \
    crates/hoimin-cli/src/analyzer/rust_tests.rs \
    crates/hoimin-cli/tests/rust_analyzer.rs
  git commit -m "perf: bound analyzer candidate collection"
  ```

---

### Task 3: Document and verify the memory contract

**Files:**

- Modify: `README.md`
- Modify: `docs/development.md`

**Interfaces:**

- Consumes: the final bounded producer behavior from Task 2.
- Produces: user/developer documentation that distinguishes retained candidate
  bounds from parser/source memory and descendant `--max-memory` enforcement.

- [ ] **Step 1: Update the documentation**

  State that each token/AST/type producer retains at most approximately
  `max_candidates + 1` candidate records before the bounded merge. Explain that
  source text, parser tokens, AST, facts, and small per-node replacement lists
  remain proportional to source size, so `--max-candidates` is not a general
  Hoimin memory cap.

- [ ] **Step 2: Commit pure documentation**

  ```bash
  git add README.md docs/development.md
  git commit -m "docs: document bounded candidate retention [skip ci]"
  ```

- [ ] **Step 3: Run fresh full repository gates**

  Run from committed HEAD:

  ```bash
  cargo fmt --all -- --check
  cargo clippy --workspace --all-targets --all-features -- -D warnings
  cargo test --workspace
  uv run --frozen python -m unittest discover -s tests -p 'test_*.py'
  git diff --check origin/main...HEAD
  ```

  Expected: all Rust and Python tests pass; configured platform-only tests may
  retain their documented skips.

- [ ] **Step 4: Trigger CI after a docs-ending branch**

  Because the preceding commit is pure documentation with `[skip ci]`, add a
  diff-empty behavior-neutral commit before the first push:

  ```bash
  git commit --allow-empty -m "ci: validate bounded candidate collection"
  ```

The PR closes #262. Squash merge only after every repository CI job passes;
the hard-cgroup job may remain in its configured skip state.
