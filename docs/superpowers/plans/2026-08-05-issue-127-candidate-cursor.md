# Issue #127 Candidate Cursor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace per-candidate backward sequence derivation with an explicit offset-and-sequence cursor carried by the run state machine.

**Architecture:** `hoimin-core` defines a serializable `CandidateCursor` and transports it through `RunState`, `ReadCandidate`, and `CandidateLoaded`. `CandidateStore` consumes that cursor to validate only the current record and returns the next cursor, eliminating `expected_sequence_at` without changing the spool format.

**Tech Stack:** Rust 2024, Serde, line-delimited JSON candidate spool, Cargo test, Clippy, rustfmt.

## Global Constraints

- Preserve candidate order, record-boundary validation, corruption errors, and declared-record-count validation.
- Keep `RunState::candidate_offset()` as a compatibility projection of the cursor offset.
- Do not change the spool encoding, candidate schema, CLI surface, spool lifetime, async I/O scheduling, worker materialization, or line indexing.
- Use checked arithmetic for cursor advancement and reject expected sequence zero.
- Prove the previous record is not parsed with a deterministic corruption regression, not an elapsed-time assertion.

---

### Task 1: Define the candidate cursor value type

**Files:**
- Modify: `crates/hoimin-core/src/model.rs`
- Test: `crates/hoimin-core/src/model.rs`

**Interfaces:**
- Consumes: no new dependencies beyond the existing Serde derives in `model.rs`.
- Produces: `CandidateCursor { pub offset: u64, pub expected_sequence: u64 }` and `CandidateCursor::START`.

- [ ] **Step 1: Write the failing cursor contract test**

Add a `#[cfg(test)]` unit-test module in `model.rs` that describes the public initial cursor:

```rust
#[cfg(test)]
mod tests {
    use super::CandidateCursor;

    #[test]
    fn candidate_cursor_starts_at_first_record() {
        assert_eq!(
            CandidateCursor::START,
            CandidateCursor {
                offset: 0,
                expected_sequence: 1,
            }
        );
    }
}
```

- [ ] **Step 2: Run the test to verify RED**

Run: `cargo test -p hoimin-core candidate_cursor_starts_at_first_record --lib`

Expected: compilation fails because `CandidateCursor` does not exist.

- [ ] **Step 3: Implement the minimal cursor type**

Add immediately before `CandidateSpoolRef`:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CandidateCursor {
    pub offset: u64,
    pub expected_sequence: u64,
}

impl CandidateCursor {
    pub const START: Self = Self {
        offset: 0,
        expected_sequence: 1,
    };
}
```

- [ ] **Step 4: Run GREEN and the core library tests**

Run: `cargo test -p hoimin-core candidate_cursor_starts_at_first_record --lib`

Run: `cargo test -p hoimin-core --lib`

Expected: both commands pass.

- [ ] **Step 5: Commit the cursor value type**

```bash
git add crates/hoimin-core/src/model.rs
git commit -m "feat(core): define candidate replay cursor"
```

### Task 2: Carry the cursor through effects and run state

**Files:**
- Modify: `crates/hoimin-core/src/effect.rs`
- Modify: `crates/hoimin-core/src/event.rs`
- Modify: `crates/hoimin-core/src/machine.rs`
- Test: `crates/hoimin-core/tests/machine.rs`
- Modify: `crates/hoimin-cli/src/shell.rs`

**Interfaces:**
- Consumes: `CandidateCursor` from Task 1 and the existing `CandidateStore::replay_one(reference, offset)` API.
- Produces: `ReadCandidate { cursor: CandidateCursor }`, `CandidateLoaded { next_cursor: CandidateCursor }`, cursor-owned `RunState`, and unchanged `RunState::candidate_offset() -> u64`.

- [ ] **Step 1: Write failing state-machine cursor tests**

In `crates/hoimin-core/tests/machine.rs`, extend the candidate-read assertions to require the initial read effect to contain `CandidateCursor::START`. Change the existing accepted-completion assertion to use:

```rust
let next_cursor = CandidateCursor {
    offset: 17,
    expected_sequence: 2,
};
let (state, _effects) = transition(
    state,
    RunEvent::CandidateLoaded(CandidateLoaded {
        id: read_id,
        worker,
        candidate: Some(fixture_candidate(1)),
        next_cursor,
    }),
)
.unwrap();
assert_eq!(state.candidate_offset(), 17);
```

In `cancellation_flushes_active_and_remaining_candidates_as_not_run`, inspect each subsequent `ReadCandidate` and assert its cursor equals the `next_cursor` accepted from the preceding completion. This covers propagation of both fields without requiring a new public state accessor.

- [ ] **Step 2: Run the focused test to verify RED**

Run: `cargo test -p hoimin-core --test machine one_active_candidate_is_applied_classified_and_reported -- --exact`

Expected: compilation fails because `ReadCandidate::cursor` and `CandidateLoaded::next_cursor` do not exist.

- [ ] **Step 3: Replace transport offsets with cursors**

Change the request and completion fields:

```rust
pub struct ReadCandidate {
    pub id: EffectId,
    pub worker: u32,
    pub spool: CandidateSpoolRef,
    pub cursor: CandidateCursor,
}

pub struct CandidateLoaded {
    pub id: EffectId,
    pub worker: u32,
    pub candidate: Option<MutationCandidate>,
    pub next_cursor: CandidateCursor,
}
```

Import `CandidateCursor` in both modules. Replace `RunState::candidate_offset` with `candidate_cursor`, initialize it with `CandidateCursor::START`, pass it from `read_next_candidate`, and assign `value.next_cursor` only in the accepted `CandidateLoaded` transition. Keep:

```rust
pub fn candidate_offset(&self) -> u64 {
    self.candidate_cursor.offset
}
```

- [ ] **Step 4: Adapt the shell bridge without changing store behavior yet**

Call the old store API with `request.cursor.offset`. For a returned candidate, construct the completion cursor with the returned byte offset and `candidate.sequence.checked_add(1)`. For EOF, return the request cursor unchanged. Map sequence overflow to an `EffectFailed` carrying the original effect ID and the existing `candidate.replay` failure code.

```rust
let Some(expected_sequence) = candidate.sequence.checked_add(1) else {
    return RunEvent::EffectFailed(EffectFailed::other(
        request.id,
        "candidate.replay",
        "candidate sequence overflow",
    ));
};
let next_cursor = CandidateCursor {
    offset: next_offset,
    expected_sequence,
};
```

- [ ] **Step 5: Update all typed fixtures and run GREEN**

Replace each `ReadCandidate { offset }` expectation with `cursor`, and each `CandidateLoaded { next_offset }` fixture with `next_cursor`. Where a fixture represents candidate sequence `n`, use expected sequence `n + 1` in the completion cursor. EOF fixtures reuse the requested cursor.

Run: `cargo test -p hoimin-core --test machine`

Run: `cargo test -p hoimin-cli shell::tests --lib`

Expected: both commands pass and `candidate_offset()` assertions remain unchanged.

- [ ] **Step 6: Commit cursor propagation**

```bash
git add crates/hoimin-core/src/effect.rs crates/hoimin-core/src/event.rs crates/hoimin-core/src/machine.rs crates/hoimin-core/tests/machine.rs crates/hoimin-cli/src/shell.rs
git commit -m "refactor(core): propagate candidate replay cursor"
```

### Task 3: Replay directly from the explicit cursor

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/store.rs`
- Test: `crates/hoimin-cli/src/analyzer/store.rs`
- Test: `crates/hoimin-cli/tests/analyzer_handler.rs`
- Modify: `crates/hoimin-cli/src/shell.rs`

**Interfaces:**
- Consumes: `CandidateCursor` propagated by Task 2.
- Produces: `CandidateStore::replay_one(reference, cursor) -> Result<Option<(MutationCandidate, CandidateCursor)>, StoreError>` with no backward record read or `expected_sequence_at` helper.

- [ ] **Step 1: Write the failing previous-record independence regression**

Create a two-record spool, capture the offset returned after record one, construct the corresponding cursor, overwrite bytes in the first JSON body while preserving its length and trailing newline, then express the desired cursor-based replay API:

```rust
let (first, second_offset) = CandidateStore::replay_one(&spool.0, CandidateCursor::START.offset)
    .unwrap()
    .unwrap();
assert_eq!(first.sequence, 1);
let second_cursor = CandidateCursor {
    offset: second_offset,
    expected_sequence: 2,
};

let mut bytes = std::fs::read(&spool.0.token).unwrap();
bytes[..usize::try_from(second_cursor.offset - 1).unwrap()].fill(b'!');
std::fs::write(&spool.0.token, bytes).unwrap();

let (second, _) = CandidateStore::replay_one(&spool.0, second_cursor)
    .unwrap()
    .unwrap();
assert_eq!(second.sequence, 2);
```

Name the test `replay_does_not_parse_the_previous_record`.

- [ ] **Step 2: Run the regression to verify RED**

Run: `cargo test -p hoimin-cli replay_does_not_parse_the_previous_record --lib`

Expected: compilation fails because `CandidateStore::replay_one` still accepts a bare `u64`. This establishes the desired cursor API before the production signature changes. After compilation is restored in Step 3, the corrupted previous record makes the test a deterministic behavioral regression against any reintroduction of `expected_sequence_at`.

- [ ] **Step 3: Make replay consume and return cursors**

Change `replay_one` and `read_one_bounded` to accept `CandidateCursor`. Reject `expected_sequence == 0` as `InvalidSequence { expected: 1, actual: 0 }`. Retain the metadata length check and one-byte preceding-newline boundary check. At non-EOF, seek directly to `cursor.offset`, parse one bounded record, and validate against `cursor.expected_sequence`.

Return the next cursor with checked arithmetic:

```rust
let next_cursor = CandidateCursor {
    offset: cursor
        .offset
        .checked_add(read as u64)
        .ok_or(StoreError::InvalidOffset { offset: cursor.offset })?,
    expected_sequence: cursor.expected_sequence.checked_add(1).ok_or(
        StoreError::InvalidSequence {
            expected: cursor.expected_sequence,
            actual: cursor.expected_sequence,
        },
    )?,
};
```

At EOF, compare `cursor.expected_sequence - 1` with `reference.records`. Delete `expected_sequence_at`; retain the I/O traits still required by the boundary check and bounded current-record read.

- [ ] **Step 4: Update store, shell, and integration call sites**

Pass `request.cursor` directly from the shell and return the store's `next_cursor` unchanged. In store property tests and `analyzer_handler.rs`, begin at `CandidateCursor::START` and carry the returned cursor. For replay from recorded boundaries, store the complete cursor instead of only its offset.

- [ ] **Step 5: Add validation regressions**

Add focused cases for:

```rust
CandidateCursor { offset: 0, expected_sequence: 0 }
CandidateCursor { offset: second_offset, expected_sequence: 1 }
CandidateCursor { offset: file_len, expected_sequence: declared_records }
CandidateCursor { offset: file_len, expected_sequence: declared_records + 2 }
```

Assert respectively: invalid sequence zero, current-record sequence mismatch, `UnexpectedEof`, and excess-record `InvalidSequence`. Keep existing invalid-boundary, oversized-record, truncated-record, corrupt-current-record, and round-trip property assertions.

- [ ] **Step 6: Run focused GREEN suites**

Run: `cargo test -p hoimin-cli analyzer::store --lib`

Run: `cargo test -p hoimin-cli --test analyzer_handler`

Run: `cargo test -p hoimin-core --test machine`

Expected: all commands pass, including the previous-record corruption regression.

- [ ] **Step 7: Commit direct cursor replay**

```bash
git add crates/hoimin-cli/src/analyzer/store.rs crates/hoimin-cli/tests/analyzer_handler.rs crates/hoimin-cli/src/shell.rs
git commit -m "perf(analyzer): replay candidates from explicit cursor"
```

### Task 4: Verify the complete change and prepare the PR

**Files:**
- Modify only files required to fix verification findings within Issue #127 scope.

**Interfaces:**
- Consumes: completed cursor implementation from Tasks 1–3.
- Produces: formatted, lint-clean, fully tested branch ready for review.

- [ ] **Step 1: Format and verify formatting**

Run: `cargo fmt --all`

Run: `cargo fmt --all --check`

Expected: the check exits successfully with no output.

- [ ] **Step 2: Run the full Rust test matrix**

Run: `cargo test --workspace`

Run: `cargo test --workspace --features contracts`

Expected: all tests pass in both configurations.

- [ ] **Step 3: Run Clippy and diff checks**

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`

Run: `git diff --check origin/main...HEAD`

Expected: no warnings and no whitespace errors.

- [ ] **Step 4: Review scope and history**

Run: `git status --short`

Run: `git diff --stat origin/main...HEAD`

Run: `git log --oneline origin/main..HEAD`

Expected: only the Issue #127 design, plan, cursor transport, store implementation, and associated tests are present; the pre-existing `.venv` remains untracked and unstaged.

- [ ] **Step 5: Push, open the PR, and monitor CI**

Push `perf/issue-127-candidate-cursor`, create a PR titled `perf(analyzer): replay candidates from explicit cursor` with `Closes #127`, and monitor every required check. If CI exposes an in-scope defect, reproduce it locally with a failing test, fix it through RED/GREEN, push, and monitor again.

- [ ] **Step 6: Merge and verify repository state**

When required checks pass and the PR is mergeable, merge it using the repository's permitted merge method. Verify the PR is `MERGED`, Issue #127 is closed, and `origin/main` contains the merge result.
