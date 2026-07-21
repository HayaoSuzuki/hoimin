# Rust Workspace Mutation Testing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `cargo-mutants` で hoimin の全 Rust ワークスペースを検査し、等価または実行不能と根拠付きで除外したもの以外の survivor と timeout をなくす。

> **2026-07-21 scope update (user-approved):** The original full-workspace
> run enumerates too many mutants for the available iteration budget. Apply
> this plan only to the highest-impact contracts: `ParsedCommand::try_from`,
> `raw_config`, and `parse_bytes` in `crates/hoimin-cli/src/cli.rs`;
> cancellation and `ProcessHandler::run` in
> `crates/hoimin-cli/src/process/mod.rs`; `RunState::accept_completion` and
> `RunState::schedule_read_or_finalize` in `crates/hoimin-core/src/machine.rs`;
> and `changed_is_normalized` / `targets_are_normalized` in
> `crates/hoimin-core/src/target.rs`. Keep `test_workspace = true`, and run
> the selected 78 mutants with `--workspace`, one `--file` flag for each of
> those four paths, the exact regex recorded in Task 2, and `--jobs 4`.
> The acceptance criterion is therefore limited to this selected set; no
> conclusion is made about unselected workspace modules.

The reproducible priority command is:

```console
cargo mutants --workspace --jobs 4 \
  --file crates/hoimin-core/src/machine.rs \
  --file crates/hoimin-core/src/target.rs \
  --file crates/hoimin-cli/src/cli.rs \
  --file crates/hoimin-cli/src/process/mod.rs \
  --re '(TryFrom<Command> for ParsedCommand>::try_from|parse_bytes|raw_config|ProcessHandler::run|ProcessStartGate::cancel|ProcessCancellation::cancel|RunState::accept_completion|RunState::schedule_read_or_finalize|targets_are_normalized|changed_is_normalized)'
```

**Architecture:** `.cargo/mutants.toml` が全 workspace mutant と全 workspace test の実行契約を所有する。ローカルの `mutants.out` は発見・反復用の一時成果物とし、結果を起点に既存の crate test を振る舞い単位で強化する。除外は完全な mutant 名にアンカーした `exclude_re` だけに限定する。

**Tech Stack:** Rust 2024（MSRV 1.85）、Cargo workspace、cargo-mutants、既存の Rust integration/unit tests、Maturin、uv。

## Global Constraints

- 対象は `hoimin-core` と `hoimin-cli` を含む workspace 全体。実行時は workspace root から `--workspace` を指定する。
- 各 mutant は `test_workspace = true` により全 workspace test で検証する。
- cargo-mutants の scratch-copy 既定動作を使い、`--in-place` と `in_place = true` は使わない。
- CI は変更しない。開発者がローカルで明示的に実行する品質手順とする。
- public CLI、JSON schema、公開 API を mutation test の都合だけで変更しない。
- survivor はテストを追加して kill する。等価または安全に実行不能な **一つの mutant** だけを、理由コメント付きの完全一致 `exclude_re` で除外する。
- broad な path/trait/function 除外、未説明の除外、catch-all 正規表現は禁止する。
- 最終 run の基準は baseline 成功、tool error なし、`missed.txt` と `timeout.txt` が空であること。`unviable.txt` は記録するが失敗条件ではない。
- `.idea/` と `tests/fixtures/projects/basic/uv.lock` は既存のユーザー未追跡ファイルであり、本作業では変更もステージも行わない。

---

## File Structure

- Create: `.cargo/mutants.toml` — workspace 全体に適用する cargo-mutants の test 選択と、レビュー済みの mutant 単位除外を保持する。
- Modify: `.gitignore` — cargo-mutants のローカル診断出力を Git 管理から除外する。
- Modify: `docs/development.md` — インストール、通常 run、反復 run、結果分類、最終確認を記載する。
- Modify: `crates/hoimin-core/tests/*.rs` — Task 2 の survivor manifest が `hoimin-core` の public behavior に対応付けた既存 test file だけを更新する。
- Modify: `crates/hoimin-cli/src/**` の既存 unit-test module または `crates/hoimin-cli/tests/*.rs` — Task 2 の survivor manifest が `hoimin-cli` の public behavior に対応付けた既存 test file だけを更新する。

各 test file の正確なパスと assertion は、変更前の baseline run が出力する mutant diff から決める。推測でテストを増やさないため、Task 3 は baseline の各 `missed` を具体的な TDD task に展開してから source/test を編集する。

### Task 1: Commit the repeatable cargo-mutants workflow

**Files:**
- Create: `.cargo/mutants.toml`
- Modify: `.gitignore`
- Modify: `docs/development.md`

**Interfaces:**
- Consumes: workspace root `Cargo.toml` and the existing Rust quality-gate commands in `docs/development.md`.
- Produces: `cargo mutants --workspace` as the canonical full-workspace command; `cargo mutants --workspace --iterate` as the local acceleration command.

- [ ] **Step 1: Add the minimal checked-in cargo-mutants configuration**

Create `.cargo/mutants.toml` with exactly the following initial content. Do not add `exclude_re` before an actual baseline identifies a reviewed exception.

```toml
# Run all workspace tests against each mutant, including cross-crate tests.
test_workspace = true
```

- [ ] **Step 2: Ignore only cargo-mutants' generated local reports**

Append this exact line to `.gitignore`:

```gitignore
/mutants.out*
```

Keep the leading slash so an unrelated nested directory with this name is not silently ignored.

- [ ] **Step 3: Document installation and the normal/iterative workflow**

Append a `## Rust mutation testing` section to `docs/development.md` after the existing analyzer-specific mutation command. Include this exact command sequence and acceptance rule:

```console
cargo install --locked cargo-mutants

# Discover all remaining outcomes.
cargo mutants --workspace

# While adding tests, reuse caught and unviable outcomes from the prior run.
cargo mutants --workspace --iterate

# Required final check: do not use --iterate here.
cargo mutants --workspace
```

State that `mutants.out/missed.txt` requires a behavior test unless the exact mutant is equivalent; `timeout.txt`, a failed baseline, and tool errors must be resolved; `unviable.txt` is inconclusive; and each allowed exception is an anchored complete-name `exclude_re` with a TOML reason comment. State explicitly that this workflow is local and does not run in CI.

- [ ] **Step 4: Install cargo-mutants outside the repository**

Run:

```console
cargo install --locked cargo-mutants
```

Expected: `cargo mutants --version` prints an installed version. This installs
the developer tool into Cargo's binary directory and must not alter
`Cargo.toml`, `Cargo.lock`, or the workspace source tree.

- [ ] **Step 5: Validate the configured workspace selection without mutating source**

Run:

```console
cargo mutants --workspace --list-files
```

Expected: source files from both `crates/hoimin-core` and `crates/hoimin-cli` are listed; no source file is edited and no `--in-place` option is present.

- [ ] **Step 6: Run the pre-mutation quality checks**

Run:

```console
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: all three commands exit 0. Fix a pre-existing baseline failure before interpreting any mutation outcome.

- [ ] **Step 7: Commit the workflow foundation**

```console
git add .cargo/mutants.toml .gitignore docs/development.md
git commit -m "docs: add Rust mutation testing workflow"
```

Expected: the commit contains only the configuration, ignore rule, and development documentation; `.idea/`, `tests/fixtures/projects/basic/uv.lock`, and `mutants.out*` are absent from the staged diff.

### Task 2: Measure and make a deterministic survivor manifest

**Files:**
- Read: `mutants.out/missed.txt`
- Read: `mutants.out/timeout.txt`
- Read: `mutants.out/unviable.txt`
- Read: `mutants.out/outcomes.json`
- Read: `mutants.out/diff/*`
- Read: `mutants.out/logs/*`
- Modify: no tracked file in this discovery task

**Interfaces:**
- Consumes: Task 1's `.cargo/mutants.toml` and a passing `cargo test --workspace` baseline.
- Produces: a local survivor manifest with one row for every `missed` and `timeout` outcome: complete mutant name, owning source path, diff/log path, classification, selected existing test path, and the assertion that will kill it or the exact exclusion rationale.

- [ ] **Step 1: Start from a clean cargo-mutants output directory**

Run:

```console
rm -rf mutants.out mutants.out.old
```

Expected: no prior iteration result can be reused. This deletes only the two ignored, repository-root diagnostic directories named explicitly above.

- [ ] **Step 2: Run the unfiltered workspace baseline**

Run:

```console
cargo mutants --workspace
```

Expected: cargo-mutants reports a successful unmutated baseline and writes `mutants.out/outcomes.json`, `missed.txt`, `timeout.txt`, `unviable.txt`, `diff/`, and `logs/`. Do not pass `--iterate`, a file filter, an exclude filter, or `--in-place`.

- [ ] **Step 3: Enumerate every actionable outcome before editing tests**

Run:

```console
test -f mutants.out/missed.txt && sed -n '1,240p' mutants.out/missed.txt
test -f mutants.out/timeout.txt && sed -n '1,240p' mutants.out/timeout.txt
```

Expected: each printed line becomes exactly one manifest row. If either file is empty, record that fact; do not infer a survivor from `unviable.txt`.

- [ ] **Step 4: Inspect the mutation before choosing a test layer**

For each manifest row, locate its entry in `mutants.out/outcomes.json`, then open its referenced diff and log. Choose a test file that already owns the changed behavior: a core policy test under `crates/hoimin-core/tests/`, a CLI unit-test module for private helpers, or an existing CLI integration test under `crates/hoimin-cli/tests/` for process/report/CLI contracts.

Expected: every `missed` row is classified as `test_gap`, `testability_boundary`, or `equivalent`; every timeout is classified as `test_gap`, `testability_boundary`, or `operationally_untestable`. No row is left unclassified.

- [ ] **Step 5: Convert the manifest into concrete, reviewable TDD subtasks before source edits**

For every `test_gap` or `testability_boundary` row, amend this plan immediately below Task 3 with one subtask containing: the exact source and test file paths; the exact test function name; the original input/fixture; the observable assertion; the command that fails with the manually applied mutant; the smallest production change if a testability boundary is required; the passing test command; and a commit command.

For every exception row, amend Task 4 with the exact emitted complete mutant name, its anchored regular expression, the TOML comment text explaining equivalence or operational unsafety, and the `cargo mutants --workspace --list` command that proves only that mutant is filtered.

Expected: no Rust source or test file is edited until the survivor-specific subtask includes a falsifiable assertion or a reviewed, exact exclusion reason.

### Task 3: Execute each manifest-derived test-gap subtask with TDD

**Files:**
- Modify: the exact existing source and test files recorded by Task 2, grouped only when they exercise the same public behavior.
- Test: the exact existing test target recorded by Task 2 for each mutant group.

**Interfaces:**
- Consumes: the concrete subtask fields written in Task 2 Step 5 and the owning module's existing public behavior.
- Produces: tests that fail under the recorded mutant, pass on the original implementation, and remove the mutant from `missed.txt` on the next run.

- [ ] **Step 1: Add one behavior-focused failing test for the first concrete mutant group**

Implement the exact `#[test]` or existing test-harness case defined in the first Task 2 subtask. Assert the observable value, diagnostic, report field, filesystem effect, or exit status changed by the recorded diff. Do not assert private implementation structure and do not add a broad snapshot that cannot identify the mutated contract.

- [ ] **Step 2: Prove the new assertion kills the recorded mutant**

Apply the recorded diff only in a disposable scratch copy, then run the exact test command specified by the subtask.

Expected: the test command exits nonzero because of the new assertion. Restore the scratch copy before continuing; never hand-edit the developer checkout to emulate a mutant.

- [ ] **Step 3: Make the minimal behavior-preserving testability change only if the subtask requires it**

Apply only the source refactor written in the subtask. Preserve CLI arguments, JSON/schema fields, error classifications, and externally observable behavior. If the test directly observes the existing contract, skip this step and change only the test.

- [ ] **Step 4: Run the owning test target on the original implementation**

Run the exact original-code test command from the subtask, followed by:

```console
cargo test --workspace
```

Expected: all tests pass. If the focused test passes under the recorded mutant, revise the assertion rather than accepting the survivor.

- [ ] **Step 5: Repeat Steps 1–4 until the manifest has no `test_gap` or `testability_boundary` rows**

Run after each coherent mutant group:

```console
cargo mutants --workspace --iterate
```

Expected: cargo-mutants skips prior caught/unviable work and refreshes `missed.txt` and `timeout.txt`. Any newly reported actionable mutant is appended to the manifest and expanded into its own concrete subtask before code is edited.

- [ ] **Step 6: Commit each coherent behavior improvement**

For each group sharing one contract, stage the exact source and test paths
written in its concrete Task 2 subtask; do not use globs or include unrelated
paths. Commit with `test: cover ` followed by that subtask's observable
behavior (for example, `test: cover normalized run configuration`).

Expected: every commit contains a test and, only when necessary, its minimal testability refactor. Do not combine unrelated survivor fixes or stage `.idea/` or `tests/fixtures/projects/basic/uv.lock`.

#### Priority-scope manifest (2026-07-21)

The user-approved priority run selected 78 mutants and completed with 33
missed, 37 caught, 8 unviable, and 0 timeout. The actionable rows form these
test-gap subtasks; no production refactor is required.

1. `crates/hoimin-cli/src/cli.rs` →
   `crates/hoimin-cli/tests/cli_config.rs`, new tests
   `line_and_symbol_are_independent_target_selectors` and
   `binary_byte_units_preserve_their_1024_multiplier`. Use a line-only run
   input, a symbol-only run input, and `--max-memory 2MiB`; assert successful
   parsing and `2 * 1024 * 1024` in the normalized limit. This kills the
   line-315 `|| → &&` mutant and both line-524 `* → +` / `* → /` mutants.
   In a disposable scratch copy, apply each corresponding
   `mutants.out/diff/crates__hoimin-cli__src__cli.rs_line_{315,524}_col_21.diff`
   before `cargo test -p hoimin-cli --test cli_config`; the assertion must
   fail. Run that command again on the original implementation, then commit
   `test: cover CLI selector and byte-unit parsing`.
2. `crates/hoimin-core/src/machine.rs:324:21: replace match guard id.0 >=
   self.next_effect_id with true in RunState::accept_completion` is
   **equivalent** under supported transitions: allocation inserts a pending
   ID, completion records it in the duplicate ledger before removal, and
   retirement records it as retired. Therefore no allocated ID can reach the
   fallback while neither pending, duplicate, nor retired; the existing
   machine tests already observe those three reachable classifications.
3. `crates/hoimin-core/src/target.rs` →
   `crates/hoimin-core/tests/target_policy.rs`, new tests
   `changed_normalization_rejects_empty_invalid_adjacent_and_overlapping_ranges`
   and `target_normalization_requires_sorted_paths_and_valid_disjoint_ranges`.
   Assert true for nonempty, positive, strictly disjoint ranges and false for
   each listed boundary case, including an empty target-line list (which is a
   valid whole-file target). These assertions cover every missed
   `changed_is_normalized` and `targets_are_normalized` row. Apply the matching
   diff from `mutants.out/diff/` in a scratch copy and run
   `cargo test -p hoimin-core --test target_policy` for RED, then rerun it on
   original code and commit `test: cover target normalization boundaries`.
4. `crates/hoimin-cli/src/process/mod.rs:61:12: delete ! in
   ProcessStartGate::cancel` is **equivalent**: after the first cancellation,
   the atomic state remains true and all callers of `cancelled()` return before
   observing any later notification; an extra notification has no supported
   observable effect. Add its anchored exclusion and the machine exclusion
   from item 2 to `.cargo/mutants.toml`; prove their narrowness with the
   selected `cargo mutants ... --list` command before committing only that
   file. The emitted names are excluded by these exact patterns:
   `^crates/hoimin-cli/src/process/mod\\.rs:61:12: delete ! in ProcessStartGate::cancel$`
   and
   `^crates/hoimin-core/src/machine\\.rs:324:21: replace match guard id\\.0 >= self\\.next_effect_id with true in RunState::accept_completion$`.

### Task 4: Add only reviewed exact-mutant exclusions

**Files:**
- Modify: `.cargo/mutants.toml`

**Interfaces:**
- Consumes: Task 2's `equivalent` or `operationally_untestable` manifest rows and their original complete cargo-mutants names.
- Produces: a list-valued `exclude_re` whose anchored patterns suppress only reviewed exceptions.

- [ ] **Step 1: Reproduce and document each proposed exception**

For each exception row, inspect the original source, mutant diff, test log, and supported input domain. Write one sentence that makes a falsifiable claim: either why all supported inputs yield the same observable behavior, or why executing this one mutant cannot terminate safely despite deterministic controls.

Expected: a test merely being inconvenient, slow, or currently absent is classified as a test gap and is not eligible for exclusion.

- [ ] **Step 2: Add the exact anchored pattern and reason comment**

Add `exclude_re` only when at least one exception is approved. Immediately
above each list element, add the exact one-sentence reason from Step 1. The
element must be a Rust-regex literal anchored by `^` and `$`, created by
escaping every regex metacharacter in that row's complete emitted mutant name.
If a second exception is approved, add its reason as the immediately preceding
comment and its pattern as a separate element in the same list. Do not replace
an existing exact pattern with a broader pattern.

- [ ] **Step 3: Prove the filter is narrow before the full run**

Run:

```console
cargo mutants --workspace --list
```

Expected: the reviewed complete mutant name is absent and neighboring mutants in the same source file/function remain listed. If the pattern hides a neighboring mutant, narrow it with additional escaped path, line, and mutation-description text.

- [ ] **Step 4: Commit reviewed exclusions separately from test changes**

```console
git add .cargo/mutants.toml
git commit -m "test: document equivalent mutants"
```

Expected: the commit contains only configuration comments and exact patterns. If no exception exists, omit this task and do not create an empty `exclude_re` key.

### Task 5: Verify the completed priority mutation-quality gate

**Files:**
- Read: `.cargo/mutants.toml`
- Read: `mutants.out/missed.txt`
- Read: `mutants.out/timeout.txt`
- Read: `mutants.out/unviable.txt`
- Modify: no tracked file unless a failure returns work to Tasks 3 or 4

**Interfaces:**
- Consumes: all Task 3 test improvements and Task 4 reviewed exclusions.
- Produces: current priority-scope mutation evidence and the existing release-quality evidence. The unfiltered full-workspace workflow remains documented in `docs/development.md`, but is outside this run's acceptance claim.

- [ ] **Step 1: Run the existing full quality gate**

Run:

```console
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
uv run maturin build --release
uv run python tests/wheel_smoke.py
```

Expected: every command exits 0. A failure returns to the task that changed the failing code; do not continue to final mutation verification with a failing ordinary suite.

- [ ] **Step 2: Delete only stale generated mutation output**

Run:

```console
rm -rf mutants.out mutants.out.old
```

Expected: the explicitly named ignored diagnostic directories are absent before the independent final run.

- [ ] **Step 3: Run a fresh priority-scope final mutation test**

Run:

```console
cargo mutants --workspace --jobs 4 \
  --file crates/hoimin-core/src/machine.rs \
  --file crates/hoimin-core/src/target.rs \
  --file crates/hoimin-cli/src/cli.rs \
  --file crates/hoimin-cli/src/process/mod.rs \
  --re '(TryFrom<Command> for ParsedCommand>::try_from|parse_bytes|raw_config|ProcessHandler::run|ProcessStartGate::cancel|ProcessCancellation::cancel|RunState::accept_completion|RunState::schedule_read_or_finalize|targets_are_normalized|changed_is_normalized)'
```

Expected: a successful baseline, no tool error, no `--iterate`, exactly the reviewed priority path/regex filters above, and no in-place mutation.

- [ ] **Step 4: Assert the final outcome files meet the acceptance contract**

Run:

```console
test ! -s mutants.out/missed.txt
test ! -s mutants.out/timeout.txt
sed -n '1,240p' mutants.out/unviable.txt
```

Expected: the first two commands exit 0 because survivor and timeout files are empty; the last command is retained as inconclusive diagnostic evidence. If either `test ! -s` command fails, return to Task 2 classification and create the missing concrete subtask before making another source edit.

- [ ] **Step 5: Inspect the final tracked diff and commits**

Run:

```console
git status --short
git log --oneline --decorate -10
```

Expected: tracked changes consist only of the planned configuration, documentation, tests, minimal testability refactors, and reviewed exact exclusions. `.idea/` and `tests/fixtures/projects/basic/uv.lock` remain untouched and `mutants.out*` is untracked/ignored.
