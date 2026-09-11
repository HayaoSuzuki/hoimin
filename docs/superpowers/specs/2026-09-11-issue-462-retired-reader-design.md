# Issue 462: Retired bounded-reader disposition

Issue: https://github.com/tokyogas-tech/hoimin/issues/462

## Problem and current applicability

The issue describes the former Python development tool's `read_bounded_regular` and `read_bounded_regular_tail`. Their POSIX implementation opened a path with `O_RDONLY | O_NOFOLLOW` before checking the descriptor with `fstat`. A FIFO without a writer could therefore block before regular-file validation, including a zero-capacity tail read. The source-discovery caller synchronously used that helper. Inspection of the parent of removal commit `2f27e2ae2060b5ca9da423622f0d5aaa2d505d2e` confirms these control flows; this review did not execute the retired FIFO reproduction.

The current base `4adf809` already contains that removal commit. It deleted `tools/focused_mutation.py`, the entire `tools/focused_mutation_support` package, and associated tests. The affected reader, tail reader, JSON wrapper and discovery caller have no current entry point. `docs/development.md` explicitly states that cargo-mutants integration was removed because its resource consumption was unsuitable and must not be installed or invoked for repository checks.

## Decision and implementation

Issue 462 is resolved by the prior feature removal. This change records its disposition and evidence in the existing OKF architecture concept and design index. No runtime patch is applicable to the current tree. Restoring the retired helper merely to add `O_NONBLOCK` would restore an unsupported workflow and contradict the current development contract.

This disposition is specific to the removed Python development tools. It does not claim FIFO safety for every Rust runtime file operation, does not change the Rust worker-tree contract from issue 144, and does not prove Windows behavior. If a bounded descriptor reader is intentionally reintroduced, it needs a new design for nonblocking open, no-follow descriptor validation, deadlines and descriptor cleanup, with real special-file tests. Existing historical reports remain historical evidence.

## Verification on the current base

- `git merge-base --is-ancestor 2f27e2ae2060b5ca9da423622f0d5aaa2d505d2e HEAD` returned 0.
- `git ls-tree -r --name-only HEAD tools` returned no files. The three paths above are absent both from the checkout and from `HEAD` (`git cat-file -e` rejected each).
- Searching active configuration, CI, tests and skills for `focused_mutation` and `read_bounded_regular` found no callers.
- A bounded subprocess importing the two old helpers failed immediately with `ModuleNotFoundError: No module named 'tools'`; it did not call a FIFO reader.
- `python3 -m unittest discover -s tests -p test_skills.py -v`: three current development-skill contract tests passed.

These checks establish removal and current workflow consistency, not a corrected implementation of the retired readers. No new Lean model or mutation campaign is warranted for absent runtime code.

## Design self-review

1. Root cause: inspected both historical open-before-fstat branches and the synchronous discovery call; the reported bug applies to that historical implementation.
2. Applicability: checked commit ancestry, tracked paths and active callers; the removal is already in the current base rather than merely proposed elsewhere.
3. Scope: separated removed Python tooling from live Rust file handling and future reader design. The decision follows the existing explicit development policy.
