# Issue 465 disposition plan and execution

Spec: ../specs/2026-09-11-issue-465-retired-discovery-design.md

## Plan and execution

- [x] Create issue-specific worktree from origin/main.
- [x] Inspect historical declaration matching, enumeration and inventory filtering.
- [x] Verify current removal ancestry and absence of both discovery paths and command caller.
- [x] Record prior removal as the resolving implementation; update design, plan, OKF concept and source index.
- [x] Verify old import absence, active references and current development-skill contracts.
- [x] Review source/index evidence and PR scope before publication.

## Plan self-review

1. Establish applicability before proposing parser changes; the implicated integration is already absent.
2. Check downstream inventory filtering as well as declaration matching, covering the whole reported selection loss.
3. Preserve the current prohibition on cargo-mutants and avoid a new parser for a removed feature.

## OKF self-review

1. Consult architecture migration history and the current Rust test workflow; distinguish source language from implementation language.
2. Add issue-specific evidence with an actual source hash without rewriting historical revisions or claiming universal syntax coverage.
3. Check metadata, footnotes, local links, root reachability, complete design indexing and displayed entry count.

## Implementation self-review

1. Historical code accounts for nested generic delimiters, function arrows, raw identifiers and same-line attributes being missed.
2. Both regex enumeration and inventory filtering are removed from current tracked code; there is no dangling command to patch.
3. Publication diff consists only of documentation; no unsupported tool, parser or dependency is restored.

## Test self-review

1. Git and checkout absence checks agree, and the deletion is an ancestor of the current base.
2. Old discovery import failed immediately in a bounded subprocess; this confirms removal and is not represented as successful Rust parsing.
3. Three existing skill contract tests passed; neither cargo-mutants nor an unrelated Lean proof was run.

## PR self-review

1. Attribute resolution to removal commit 2f27e2a and describe this PR as evidence/documentation.
2. Separate inspected historical behavior from executed current checks and retain the Python/Rust analyzer distinction.
3. Recheck source hash/index count, whitespace and final diff; independent review precedes publication.
