# Include Walk Default Exclusions Implementation Plan

> **Issue:** #121 — apply default exclusions to the include-glob manifest walk

## Goal

Prevent broad or explicit workspace include globs from adding `.git`, virtual
environments, and tool caches to worker manifests, while preserving the ability
to restore ordinary gitignored files with `--include`.

## Design

Both workspace walks must enforce the same built-in directory boundary. Keep
the existing override behavior for user include/exclude globs, and add
`filter_entry(|entry| !default_excluded(entry))` to the include walk. Filtering
directories at traversal time avoids hashing or descending into protected
trees.

## Task 1: Add a failing regression test

**Files:**

- Modify: `crates/hoimin-cli/tests/workspace_handler.rs`

Add a test that:

1. Creates files beneath every directory recognized by `default_excluded`.
2. Creates a normal gitignored file outside those directories.
3. Builds a workspace with a broad include glob.
4. Verifies the gitignored file is restored by the include pass.
5. Verifies none of the built-in excluded paths enter the manifest.

Run the focused test and confirm it fails before production code changes.

## Task 2: Enforce exclusions and verify

**Files:**

- Modify: `crates/hoimin-cli/src/workspace/manifest.rs`

Apply the existing `default_excluded` predicate to the include walk. Run the
focused test, workspace handler suite, formatting, Clippy, full workspace tests,
contract-feature tests, and `git diff --check`.

Request an independent code review before creating the PR.
