# Plan output and parallel-run tuning documentation

## Purpose

Resolve issues #14 and #16 by aligning the bundled mutation-testing skill with the
existing `hoimin plan` CLI contract and by giving users a practical procedure for
tuning parallel mutation runs. Both changes are documentation-only: command behavior,
defaults, output schemas, and resource enforcement remain unchanged.

## Plan output contract

`hoimin plan` intentionally has no `--format` option. It always writes one JSON plan
manifest to standard output and reserves standard error for diagnostics. The
`hoimin-mutation-testing` skill must say this explicitly immediately after its plan
example and must tell agents not to pass `--format` to `plan`.

The repository carries identical Codex and Claude copies of this skill. Update both
`.agents/skills/hoimin-mutation-testing/SKILL.md` and
`.claude/skills/hoimin-mutation-testing/SKILL.md` with the same wording. Preserve the
existing `verify --format json` example because `verify` does support selectable output
formats.

Extend `tests/test_skills.py` so the explicit JSON-only plan contract is a required
block. The existing byte-for-byte mirror assertion continues to prevent the two skill
copies from drifting. Existing CLI coverage continues to assert that `plan` rejects
report and session options, including `--format`; no new CLI behavior is introduced.

## Parallel-run tuning guidance

Add a concise `Tuning parallel runs` section in `README.md` immediately after the
resource-limit explanation. Keep the defaults and their safety rationale unchanged.
The section describes this procedure:

1. Start with `--jobs 1` and a focused test command. Record baseline elapsed time and
   estimate the memory used by one test worker.
2. Increase `--jobs` gradually. Explain that `--max-memory` is one run-wide limit shared
   by the analyzer, baseline, and all concurrent workers; it is not multiplied by the
   job count.
3. When adding concurrency, choose an explicit `--mutant-timeout` with enough headroom
   for contention instead of assuming that the single-worker baseline-derived `auto`
   value will remain sufficient.
4. If results contain `out_of_memory`, lower `--jobs` or raise `--max-memory`. If they
   contain `timeout`, lower `--jobs` or raise `--mutant-timeout`.

Include two concrete examples. A small target may retain
`--jobs 1 --max-memory 1GiB --mutant-timeout auto`. For a focused baseline taking about
14 seconds, show the tested parallel settings
`--jobs 4 --max-memory 4GiB --mutant-timeout 2m`. The complete command template uses
native argv after `--` and no shell-specific syntax, so it is valid on both Windows and
Unix.

The 14-second example is illustrative rather than a universal sizing formula. The text
must tell users to measure their own suite and increase concurrency progressively.

## Scope and compatibility

The implementation changes only these files:

- `.agents/skills/hoimin-mutation-testing/SKILL.md`
- `.claude/skills/hoimin-mutation-testing/SKILL.md`
- `tests/test_skills.py`
- `README.md`

Do not add `--format` to `hoimin plan`, change resource defaults, alter automatic timeout
calculation, or modify public JSON schemas. Do not duplicate the parallel-tuning guide
in `docs/development.md`; keeping one user-facing source avoids documentation drift.

## Verification

Run the focused Python skill-contract test and the relevant Rust documentation/CLI
tests. Confirm that the two skill files remain byte-identical, every documented command
parses under the existing CLI, and `plan --format json` is still rejected. Finish with
the repository quality checks appropriate for documentation and test-only changes,
including `cargo test --workspace` and a whitespace check.
