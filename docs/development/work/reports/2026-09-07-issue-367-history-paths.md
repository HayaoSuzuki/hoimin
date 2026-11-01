# Issue #367: preserve recent Git paths

## Scope

Issue: https://github.com/tokyogas-tech/hoimin/issues/367

Branch: `fix/issue-367-history-paths`, from main `8aab791`.
Worktree: `.worktrees/issue-367-history-paths`.

This batch addresses independent bugs #367, #360, and #356. Each branch has its
own implementation, tests, and report. #367 touches Python discovery and its
test fixtures; the other branches touch Rust CLI parsing and worker existence
checks. The open P2 #338 already has its production fix in main. We prioritized
these existing bugs over #369's CI expansion.

## Cause and change

The history probe used `git log --name-only --format=` and split its output by
lines. Git C-quotes filenames containing non-ASCII bytes, quotes, or backslashes.
The `.rs` suffix filter dropped those quoted records, so users lost both recent
fallback candidates and their `recent_change` ranking reason.

The probe now requests `-z` and uses the existing `_nul_paths` parser. The probe
retains its first-parent, 20-commit bound and timeout. We removed `_line_paths`.
No source comments or new parser abstraction are needed.

The existing normalization also replaced literal POSIX backslashes with `/`.
`Path(value).as_posix()` now normalizes native separators while preserving POSIX
filename characters. Repository-relative path validation, excluded directories,
deduplication, and Markdown-safe reporting checks remain in place.

## Regression evidence

The original discovery suite passed 16 tests. The new integration regression
creates a temporary Git repository with `core.quotepath=true` and commits each
filename twice. It covers Japanese text and spaces on supported platforms, plus
quotes and literal backslashes on POSIX. It checks newest-first deduplication,
fallback candidate selection from clean HEAD history, and the ranking reason.

Before the fix, only the space-containing filename reached `recent_paths`.
Adding `-z` alone still changed the literal backslash into a directory separator.
After native normalization, all 17 discovery tests passed. The broader workflow
fixture now expects the NUL history command as well.

## Verification

Commands ran on macOS with Python 3.14.7 from the existing development environment:

```sh
python -m unittest tests.test_focused_mutation_discovery -q
python -m unittest discover -s tests -p 'test_focused_mutation_*.py' -q
python -m unittest discover -s tests -p 'test_*.py' -q
ty check tools/focused_mutation_support/discovery.py tests/test_focused_mutation_discovery.py
git diff --check
```

The focused-tool suite passed 793 tests with 56 platform/optional skips.
The full Python suite passed 850 tests with the same 56 skips. The sandbox
blocked `ps` during the first full run, producing four process-monitor test
failures. We confirmed that restriction, reran the five monitor tests with
process-observation permission, then reran the full suite with that permission.
Both reruns passed. The targeted type check and diff whitespace check passed.
Independent review found no blocking issue and reran 139 discovery/reporting
tests with one skip.

For mutation coverage, we planned `_normalize_path` at lines 38-49 and verified
candidate `m1_0b976f561970ddb8bcff78fe2e011acd1462aff7765a17a43ee8182a64527b10`
(`or` to `and` in the absolute/parent-path rejection). The baseline exited 0;
the mutant test command exited 1. Verification reported one killed mutant,
zero survivors, a complete run, and exit 0.

We used one worker, an 8 GiB workspace limit, and a 10 GiB free-space reserve,
with explicit best-effort memory approval on macOS. The report recorded clean
execution-workspace cleanup. Exit traps removed the exact temporary plan/report
directories. This targeted check does not establish a full-file mutation score.

The new real-Git regression has not run on Windows in this delivery. It omits
quote/backslash filename fixtures there because Windows disallows those names.
We did not poll Actions or dispatch additional workflows.
