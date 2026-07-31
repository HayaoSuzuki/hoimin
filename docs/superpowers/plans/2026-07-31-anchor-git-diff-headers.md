# Anchor Git Diff Header Parsing Implementation Plan

> **For Codex:** Use Superpowers subagent-driven development and test-driven development to execute this plan.

**Goal:** Prevent zero-context patch content beginning with `++ ` or `-- ` from being interpreted as `+++`/`---` file headers and corrupting `--changed` target selection.

**Architecture:** Turn `parse_diff` into a small section-aware parser. A `diff --git` line starts a new section and clears both paths. Only an `---` line seen before a section body may set the old path, and only the immediately expected `+++` line may set the destination path. Hunk and binary records are processed within the current section; hunk body text can no longer change section metadata.

**Tech Stack:** Rust, Git unified diff format, Tokio integration tests, Cargo.

---

## Task 1: Reproduce and fix content/header confusion

**Files:**

- Modify: `crates/hoimin-cli/src/target/git.rs`
- Modify: `crates/hoimin-cli/tests/target_handler.rs`

### Step 1: Reproduce destination-path corruption

Add a real-repository integration test with two separated edits in
`pkg/doc.py`. Make the first added source line begin with:

```text
++ b/pkg/evil.py
```

Git prefixes that source line with the added-line marker, so the patch contains
`+++ b/pkg/evil.py`. Ensure `pkg/evil.py` also exists so an erroneous target is
observable.

Assert that both changed ranges remain attributed to `pkg/doc.py` and that
`pkg/evil.py` receives no changed range.

Run:

```console
cargo test -p hoimin-cli --test target_handler \
  changed_content_cannot_replace_the_diff_destination -- --exact
```

Expected RED: the later hunk is attributed to `pkg/evil.py`.

### Step 2: Reproduce false deletion exclusion

Add a second integration test where one changed patch body contains adjacent
source lines that Git emits as:

```text
--- a/pkg/innocent.py
+++ /dev/null
```

This can be produced by replacing source text `-- a/pkg/innocent.py` with
`++ /dev/null`. Independently modify `pkg/innocent.py`.

Assert that the legitimate `pkg/innocent.py` change remains selected.

Run:

```console
cargo test -p hoimin-cli --test target_handler \
  changed_content_cannot_exclude_another_python_file -- --exact
```

Expected RED: the innocent file is removed by the corrupted exclusion set.

### Step 3: Add explicit patch-section state

In `parse_diff`, represent these states explicitly:

- outside a `diff --git` section;
- section metadata, awaiting the old `---` header;
- awaiting the new `+++` header after an old header;
- section body after both headers.

On every `diff --git ` line:

- clear `old_path` and `path`;
- enter the metadata/old-header state.

Only parse `--- ` while awaiting the old header. Only parse `+++ ` while
awaiting the new header, then enter the body state. Ignore identically prefixed
lines in the body. Process hunk headers only after the destination header is
established.

Keep binary detection working for ordinary binary sections that may not carry
text file headers. Do not change quoted-path decoding or hunk range parsing.

### Step 4: Prove RED becomes GREEN

Run both focused integration tests. Expected: PASS.

Then run:

```console
cargo test -p hoimin-cli --test target_handler
```

Expected: all target handler tests pass, including deletion, binary, rename,
quoted/non-UTF-8 body, and hostile Git configuration coverage.

### Step 5: Full verification

Run:

```console
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo test -p hoimin-cli --features contracts
git diff --check
```

Expected: every command succeeds.

### Step 6: Commit

Stage only the parser, integration tests, and this plan:

```console
git add crates/hoimin-cli/src/target/git.rs crates/hoimin-cli/tests/target_handler.rs docs/superpowers/plans/2026-07-31-anchor-git-diff-headers.md
git commit -m "fix: anchor Git patch file headers"
```

## Acceptance Checklist

- A `+++`-looking added content line cannot replace the current destination.
- A paired `---`/`+++`-looking body cannot exclude an unrelated Python file.
- Normal edits, deletion, binary handling, and renames retain their behavior.
- Invalid real header paths still return typed Git failures.
- Full workspace and contracts-enabled CLI tests pass.
