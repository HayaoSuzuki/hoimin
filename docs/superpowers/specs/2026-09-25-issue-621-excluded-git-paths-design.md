# Excluded Git path validation design

## Intent and acceptance

Issue 621 reports changed-only plan failing because excluded data filenames contain literal backslashes. Ordinary target discovery already succeeds and supplies eligible paths. Tracked changes, untracked files, and indexed files before HEAD must ignore such unrelated names, while selected unsupported Python paths and the standalone unscoped Git API retain their rejection. This is a bounded ordering fix, not a relaxation of portable paths.

## Alternatives and chosen design

1. Filter Git commands with pathspecs. This also reduces work, but changes rename/pathspec behavior and belongs to issue 620; it cannot replace correct local validation ordering.
2. Filter only non-Python extensions. This fixes the reported .txt example but leaves explicitly excluded .py filenames able to fail selection.
3. Chosen: use the already resolved eligible path set before portable-path validation in every Git name ingress: patch old/new headers, binary numstat including both rename names, and NUL-separated untracked/unborn records. Decode Git quoting first, remove a/b prefixes, recognize /dev/null, then determine scope membership and only validate admitted paths. Keep structural parse errors and UTF-8 decoding errors unchanged; non-UTF-8 names were explicitly outside the issue reproduction and this fix does not broaden that contract.

A private scope helper owns optional platform comparison keys. None means unscoped and preserves validation of every path. Some(empty) ignores all names. Eligible targets come from normal discovery and are portable; a scoped raw Git name containing a literal backslash cannot equal an eligible portable name. Reject that comparison before Windows key normalization can turn a backslash into a separator. For other names, use existing logical_path_equality_key to preserve Windows case behavior and exact Unix spelling. Do not consult explicit line ranges during path admission.

Patch framing remains stateful even when both names are out of scope; body lines must never become headers. An excluded rename source must not reject an admitted destination, and binary destination exclusion must still work when the source is excluded. Retain all existing hunk/deletion/context/coordinate behavior from 612/632. Do not read excluded paths or change Git subprocess invocation in this issue.

## Validation and model boundary

Use real Git and public plan fixtures for three states × excluded .txt/.py names, with plain/changed candidate equality and selected invalid-path negative controls. Include tracked binary numstat, rename from an excluded unsupported spelling into a valid selected file, empty selection, and a valid path whose slash spelling would collide with a literal-backslash name. Existing unscoped rejection and parser framing/property tests must pass. Malformed quoted paths and invalid UTF-8 remain errors.

This change is a finite admission predicate plus existing parser plumbing. A new Lean generator would restate the predicate without independently exercising Git quoting or filesystem discovery. Existing changed-target/context/line Lean public adapters provide integration coverage; real Git and strict parser tests are the direct oracle for this issue. No new Lean model or resource lease is needed.

## Design reviews before code

1. Scope completeness: extension-only filtering misses excluded .py and binary numstat. The chosen helper covers every name ingress and both sides of rename records.
2. Identity safety: logical_path_equality_key normalizes backslashes on Windows. Filtering must reject raw backslash membership before deriving that key, so unrelated names cannot alias selected slash paths; portable validation itself stays unchanged.
3. State and error boundary: an out-of-scope patch path is not permission to skip parser framing. Continue parsing structural records/hunks and preserve UTF-8/escape errors; only portable validation moves after eligibility. Selected unsupported paths are rejected by normal discovery before Git. Standalone calls remain strict.
