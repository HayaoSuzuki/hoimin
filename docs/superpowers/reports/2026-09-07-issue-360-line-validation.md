# Issue #360: validate CLI line selections

## Scope

We fixed [#360](https://github.com/tokyogas-tech/hoimin/issues/360) in
`.worktrees/issue-360-line-validation` on branch
`fix/issue-360-line-validation`, based on `8aab791`. The change covers
`--line` parsing and its CLI regression tests. It does not change core target
resolution, schemas, Python code, or runtime behavior for valid selectors.

## Cause and fix

`parse_line_selection` already used `rsplit_once(':')`, so a path could contain
colons. It parsed both range bounds as `u32`, then built a `LineSelection`
without checking the path or range invariants. As a result, `:5-10`,
`src/a.py:0-5`, and `src/a.py:10-5` reached core target resolution. The core
fallback error formats `LineRange` with its Rust debug representation.

The CLI parser now rejects an empty path, a zero start, and a start greater
than the end. Each failure uses this diagnostic:

```text
invalid --line: VALUE; expected PATH:START-END with 1-based START <= END
```

The parser still splits on the final colon. A regression test checks
`C:/repo/pkg/a.py:4-7` and confirms the path remains
`C:/repo/pkg/a.py`. A boundary table covers single-line selector `1`, the full
range from `1` to `u32::MAX`, and the single-line selector at `u32::MAX`.

## TDD evidence

The baseline `cli_config` suite passed 46 tests. We then added one parser test
for each invalid input, a path-preservation test, the boundary table, and three
Unix tests that launch the compiled `hoimin` binary. Before the production
change, the three parser tests failed because `parse_config_from` returned
valid `RunConfig` values containing the bad selections.

The binary tests run `hoimin run` against a temporary Python project. They pass
a shell command that would create a marker if Hoimin started the test command.
They require exit code 2, empty stdout, an absent marker, and the exact stderr
diagnostic. We reverted the production change for a second red check. All three
binary tests failed because the old path emitted run reports on stdout. After
we restored the parser change, the binary tests passed 3 tests and the full
`cli_config` test file passed 54 tests.

## Verification

| Command | Result |
| --- | --- |
| `cargo test -p hoimin-cli --test cli_config --quiet` | 54 passed |
| `cargo test --workspace --lib --all-features --quiet` | 530 library tests run: 521 passed, 9 ignored; core library: 9 passed |
| `cargo test -p hoimin-core --test target_policy --quiet` | 27 passed |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Exit 0 |
| `cargo fmt --all -- --check` | Exit 0 |
| `git diff --check` | Exit 0 |

Rust builds used the isolated target directory
`/tmp/hoimin-issue-360-target`. We did not run the full integration workspace
suite for this parser-only fix. We did not run Linux or Windows tests.

Independent review found no blocking issue. The coordinator reran all 54
`cli_config` tests and the diff whitespace check before committing.
