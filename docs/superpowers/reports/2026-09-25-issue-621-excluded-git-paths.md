# Issue 621: scoped Git path validation

Design and plan, including three actual reviews each, were committed before implementation as `0d52cc4`, above issue 612 head `3b456ff`. This change admits names using the already resolved eligible path set before portable-path validation. Patch old/new headers, both names in binary rename records, and untracked/unborn records share the same private scope rule. Selected unsupported paths still fail normal discovery; standalone Git resolution stays strict.

## RED and GREEN

The public plan matrix reproduced all six original failures: tracked, untracked and unborn-indexed states, each with an excluded literal-backslash .txt or .py name. Plain plans already succeeded with one boolean candidate; changed plans failed in Git path validation. A separate real-Git rename fixture reproduced rejection of an excluded unsupported old name despite a valid selected destination. The fixture asserts Git actually reports the rename and a binary numstat entry.

After implementation, all five new tests pass. The six matrix cases compare complete candidate objects after removing ranking-only fields. Additional tests cover selected invalid-path rejection in both plain and changed plan, an excluded binary name plus renamed selected destination, slash/backslash collision through the public target resolver, and an empty resolved scope. Existing target handler and all three Lean-generated changed-selection public suites pass (61 tests), including CR/CRLF, context, plan/run/verify and codec coverage. All 20 Git module tests pass, including five new scope/framing/rename/error-boundary tests and existing parser property tests.

Artifacts under `/tmp/hoimin-batch-604-632/`: `621-red-public.log`, `621-red-corrected.log`, `621-green-public.log`, `621-focused.log`, `621-parser-final.log`. The first three logs preserve preparation failures as well as real regressions; final focused/parser logs are the passing evidence.

## Implementation reviews

1. **Identity and platform:** read the actual core key implementation; Windows normalizes backslash separators as well as case. The scope checks raw backslashes first, preventing an unsupported literal name from becoming a selected slash name. Ordinary valid spellings retain the established platform comparison. None remains unscoped and validates every path; Some(empty) admits none. New unit controls pin both cases and platform-dependent case behavior. Windows execution is left to CI; local results are macOS.
2. **Ingress and callers:** audited both diff-base and HEAD branches, old/new patch headers, plain and rename binary numstat, and indexed/untracked collectors. All name ingresses use the helper. Numeric line restrictions do not affect membership. Current-file reading, explicit intersections and 612 coordinate conversion retain their original flow; no Git command/pathspec optimization is included.
3. **Parser and error boundary:** filtered paths still advance the existing patch state machine, so an added body line that resembles a +++ header cannot create a selected file. Binary rename names are filtered independently in either direction. Structural, quote-escape and UTF-8 errors remain errors, even with an empty scope. This deliberately fixes portable-name validation ordering only; it makes no broader claim about arbitrary non-UTF-8 Git names.

## Test reviews and preparation corrections

1. **Regression premises:** the issue matrix compares ordinary and changed public plans on exact same bytes and fresh real repositories. Selected invalid-path rejection initially expected the Git-layer wording; ordinary discovery correctly rejects earlier with `target path cannot be represented portably`. Corrected that assertion without changing production behavior or weakening exit 2.
2. **Scope and independence:** the initial collision fixture used source discovery, which correctly diagnosed the literal name before Git. Switching the fixture to exact-file CLI selection then encountered the existing `--changed requires --source` contract. The final collision test uses the public TargetHandler API with exact-file selection, whose ordinary discovery already excludes the unrelated name; the CLI requirement remains unchanged. Empty-scope CLI coverage is a separate valid source/exclude case. The original six-case RED and rename RED are independent of these setup corrections.
3. **Isolation and negative controls:** fresh directories, controlled Git config and 20-second kill-on-drop Git deadlines avoid environment leakage; public CLI invocations have a 30-second deadline. Candidate comparison ignores only ranking fields. Existing unscoped backslash rejection tests remain intact, and scoped parser tests preserve malformed/encoding error behavior. An initial unit-test compile error assumed TargetSlice implemented Default; the fixture now initializes its actual fields explicitly. This was a test preparation error, not a semantic failure.

## Lean applicability

The risk is correspondence between real Git name decoding, normal filesystem selection, and validation order. A fresh Lean admission predicate would restate the helper without validating those interfaces. Existing changed-target, changed-context and changed-lines Lean-generated public adapters were run unchanged; the new oracle is real Git/public plan plus scoped parser controls. No new Lean theorem, corpus, resource-bound claim or Python production behavior was added, and no Lean process was started for 621. Python mutation testing is not applicable.

## Independent review

Root reviewed production `git.rs` and `git/paths.rs` independently and found no blockers. The review checked all four name ingress classes, decode/filter/validate ordering, Windows separator collisions, independent binary rename sides, preserved malformed-input errors and patch body/header state. Scoped selected-path rejection remains the responsibility of prior discovery, as exercised by the public negative controls.

## Final validation before parent integration

`cargo test --locked --workspace` passed 2,396 tests, zero failures, with 22 ignored across 107 result entries. Exact workspace Clippy (`--workspace --all-targets --all-features -- -D warnings`) and vendor Clippy (`--locked -p littrs-ruff-python-parser --lib --no-deps -- -D warnings`) passed, as did workspace/vendor formatting and `git diff --check`. Logs: `621-workspace.log`, `621-clippy.log`, `621-clippy-vendor.log`.

The unpublished branch then rebased cleanly onto the updated 612 parent `256aa32`, including main `195773a`. Final integration passed all 20 Git module tests and 68 related public/target/oracle tests, including the newly merged glob-selection oracle. Both exact Clippy commands, both formatting gates and whitespace checks passed again. The parent merge also passed the 40 Python CI registration tests. No production conflict or unresolved concern required repeating the full workspace suite. Evidence: `621-integrated-*.log`.
