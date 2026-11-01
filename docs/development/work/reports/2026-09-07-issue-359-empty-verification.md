# Issue #359: reject empty top-N verification

## Selection and scope

We selected [#359](https://github.com/tokyogas-tech/hoimin/issues/359) from the
open bug list because automation can interpret its exit code 0 as evidence that
verification succeeded, although it tested no mutants. The P2 issue #338's
`oom_kill` classification and regression coverage already exist on main; the
other listed bugs have priority P3. We did not change those issues.

The user authorized autonomous delivery through PR creation and requested one
worktree per issue, including committed documentation. This work uses
`.worktrees/issue-359-empty-verification`, branch
`fix/issue-359-empty-verification`, based on main `539e8e1`. Previous issue
worktrees and unrelated files on main remain untouched.

## Cause and contract

`select_top_candidate_ids` caps the requested count at the retained candidate
count. For an empty plan, strict and diverse policies both return an empty
vector. `resolve_verify_selection` checked the upper budget but accepted that
empty vector, unlike the explicit-ID path. The CLI then ran a baseline and
reported a successful verification with no mutants.

We added the empty-selection guard to `resolve_verify_selection`, before source
rediscovery and runtime preparation. Both policies now return
`plan.candidate.invalid` with a message naming `--top` and the lack of retained
candidates. The CLI exits 2, writes the diagnostic to stderr, and emits no run
report or events. It does not run the baseline or rewrite the plan.

Empty plans remain valid outputs of `plan`. Nonempty selections retain their
saved rank, diverse ordering, budget checks, and truncation behavior, including
selecting all retained candidates when N exceeds their count. We updated README
and public API documentation to state the empty-plan exception.

This is a selection-validation change. It introduces no format/schema changes,
new flags, dependency changes, or state-machine behavior.

## Regression evidence

The baseline plan integration suite passed 36 tests. We then added two tests
that generate a genuine, nontruncated plan from a comment-only Python source.
They invoke `run_with_io` with `--top 5`, one test per selection policy, and
exercise JSON and JSONL output. A test command writes a marker outside the
project so we can detect baseline execution without affecting source validation.

Before the guard, both tests failed with actual exit code 0 versus expected 2.
After the guard, the plan suite passed 38 tests. The new tests also check the
diagnostic, empty stdout, absent baseline marker, and byte-for-byte unchanged
manifest. Existing tests cover nonempty strict/diverse selection, a one-candidate
truncated plan, requests larger than the retained set, and runtime execution.

Independent code review found no blocking issue in the guard, tests, or README.

## Validation results

| Check | Result |
| --- | --- |
| macOS plan integration tests, default features | 38 passed |
| macOS `cargo test --workspace --all-features --quiet` | Exit 0; existing ignored tests remain ignored |
| Linux plan integration tests, contracts enabled | 37 passed; one additional test is macOS-only |
| macOS and Linux workspace Clippy, all targets/features, warnings denied | Exit 0 |
| Formatting and whitespace checks | Exit 0 |

We used Rust 1.98 on both systems. macOS used the frozen project environment
with Python 3.14.7. Linux checks ran in a `rust:1.98-bookworm` aarch64 container
as UID/GID `501:20`, with read-only source and a separate writable build mount.
The Linux fixture interpreter was `/usr/bin/python3` (3.11.2), exposed through
a separate read-only `.venv/bin/python` symlink mount; these plan fixtures use
only the Python standard library. We did not run the full Linux workspace suite,
Windows tests, or the standalone Python suite for this Rust-only change.

## Reproduction

From the issue worktree:

```sh
uv sync --frozen
cargo test -p hoimin-cli --test plan rejects_empty_plan_before_baseline --quiet
cargo test -p hoimin-cli --test plan --quiet
cargo test --workspace --all-features --quiet
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
git diff --check
```

The two new regression names are
`verify_top_strict_rejects_empty_plan_before_baseline` and
`verify_top_diverse_rejects_empty_plan_before_baseline` in
`crates/hoimin-cli/tests/plan.rs`.

We limited GitHub calls to issue/PR discovery and publishing. We did not poll
Actions or dispatch additional workflows.
