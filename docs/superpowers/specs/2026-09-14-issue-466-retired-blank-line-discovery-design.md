# Issue 466: Retired blank-line discovery disposition

Issue: https://github.com/tokyogas-tech/hoimin/issues/466

## Problem and current applicability

Issue 466 reported nearly quadratic discovery time in the former focused-mutation Python tool. At commit `12c2d79`, the tool's multiline `_FUNCTION` regular expression began with `^\s*`. Because `\s` includes newlines, a run of blank lines could be reconsidered from successive line starts before the expression failed. The reported 1,000/2,000/4,000/8,000 blank-line inputs took 18.253/74.752/300.341/1,206.129 ms, while the timeout callback was never reached. Historical source also confirms that `_FUNCTION.findall(source)` completed before candidate-limit checks inside the result loop, so those checks could not bound the regex scan.

The original disposition base `165a2d2` contains removal commit `2f27e2ae2060b5ca9da423622f0d5aaa2d505d2e`. That commit deleted `tools/focused_mutation_support/discovery.py`, the focused-mutation command, supporting modules and their tests. The current Git tree has no focused-mutation discovery path, and `docs/development.md` prohibits installing or invoking cargo-mutants for repository checks.

## Decision

Issue 466 has been superseded by removal of the affected integration. This PR records the issue-specific evidence in the repository's design history. It does not restore the regex, add a replacement parser, or close the issue directly. A reviewer can use the recorded ancestry, absence and workflow evidence when deciding whether to close it.

The active hoimin analyzer is Rust code that parses Python source through Ruff. It is not the removed Python tool that scanned Rust declarations for cargo-mutants. Consequently, neither the issue's blank-line corpus nor its historical timing ratios describe a reachable current command. If Rust mutation discovery is intentionally reintroduced, its design must separately specify syntax coverage, timeout observation points and deterministic scaling checks that do not rely on fixed wall-clock thresholds.

## Verification on the current base

- `git merge-base --is-ancestor 2f27e2a HEAD` returned success.
- `git ls-tree -r --name-only HEAD` contains no focused-mutation or discovery module.
- A live-source search found no `focused_mutation`, `discover_candidates`, `FUNCTION_PATTERN` or `_FUNCTION` references outside historical documentation; the unrelated `OPERATOR_FUNCTION_PAIRS` constant does not implement Rust discovery.
- Historical inspection at `2f27e2a^` found `_FUNCTION.findall(source)` before the candidate-limit loop and confirmed the newline-consuming prefix.
- A bounded import of the removed module fails immediately, while the current development-skill contract tests pass.

These checks prove current unreachability and policy consistency. They do not prove a corrected regex, linear-time behavior for a replacement, or the historical millisecond measurements on today's environment.

## Design self-review

1. Causality: the historical source supports both parts of the report—the newline-consuming prefix and the late candidate-limit check—without presenting a new benchmark as though the removed code still ran.
2. Applicability: ancestry, tracked-tree absence, live caller search and current development policy jointly establish supersession; deletion of a single file is not the only evidence.
3. Scope: the retired Rust-source scanner is distinguished from the current Python-source analyzer, and any future reintroduction gets explicit parser, budget and scale-test requirements.

## Revalidation at 8b33167 (2026-09-14)

PR #527 already recorded this disposition and is an ancestor of the current
base `8b33167`. The follow-up changes only the evidence and acceptance mapping;
there is no production implementation to modify. Current `tools/` contains the
unrelated performance-shapes harness, so absence checks must name the retired
package rather than assume that the entire tools directory is absent.

| Issue acceptance condition | Current disposition |
| --- | --- |
| Near-linear scanning, including multiline declarations | Superseded by removal; no replacement scanner or syntax compatibility claim |
| Observe the time budget during syntax discovery | Superseded by removal of the discovery command and callback path |
| Preserve ordinary Rust and blank-line results | Retired functionality; current Python analyzer is a different interface |
| Correctness and operation/scaling regression checks | Not applicable to an absent implementation; ancestry, caller absence and policy are checked instead |

The issue can be closed as obsolete because the affected integration has been
removed. This is not evidence that its historical regex was repaired. A future
reintroduction must satisfy all four original conditions before reuse.
Fresh commands, results and three review passes per stage are recorded in the
[follow-up report](../reports/2026-09-14-issue-466-revalidation.md).
