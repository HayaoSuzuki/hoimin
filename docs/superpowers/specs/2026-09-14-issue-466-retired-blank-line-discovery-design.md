# Issue 466: Retired blank-line discovery disposition

Issue: https://github.com/tokyogas-tech/hoimin/issues/466

## Problem and current applicability

Issue 466 reported nearly quadratic discovery time in the former focused-mutation Python tool. At commit `12c2d79`, the tool's multiline `_FUNCTION` regular expression began with `^\s*`. Because `\s` includes newlines, a run of blank lines could be reconsidered from successive line starts before the expression failed. The reported 1,000/2,000/4,000/8,000 blank-line inputs took 18.253/74.752/300.341/1,206.129 ms, while the timeout callback was never reached. Historical source also confirms that `_FUNCTION.findall(source)` completed before candidate-limit checks inside the result loop, so those checks could not bound the regex scan.

The current base `165a2d2` contains removal commit `2f27e2ae2060b5ca9da423622f0d5aaa2d505d2e`. That commit deleted `tools/focused_mutation_support/discovery.py`, the focused-mutation command, supporting modules and their tests. The current Git tree has no focused-mutation discovery path, and `docs/development.md` prohibits installing or invoking cargo-mutants for repository checks.

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
