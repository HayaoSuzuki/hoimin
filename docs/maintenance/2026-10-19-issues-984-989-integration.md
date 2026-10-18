# Issues 984–989: integration and review record

This branch restores a worker after a successful baseline before dispatching any mutant, rejects nonregular plan/report inputs, fixes duplicate elif visitation, bounds redundant equivalent finally transfers, avoids a generic JSON tree during schema routing, and defines the existing 54 default operators directly.

The implementation starts from `5dd992af89cc834800e5f4e6e6692cbf62900ddc`. One isolated worktree and one Cargo owner were used, with two build jobs and two test threads for process-heavy suites. Debug information and incremental compilation were disabled. Python package installation used a dedicated scratch environment; no packages were installed into the shared environment. Main checkout Lean artifacts were not modified. The isolated target reached approximately 13 GiB while retaining the normal and contracts builds; cleanup is performed after verification, with evidence retained separately.

## Task evidence and review

| Issue | Evidence | Independent task review |
| --- | --- | --- |
| #985 | [Descriptor admission](2026-10-18-issue-985.md) | Compliant and approved |
| #986 | [Single visitation](2026-10-18-issue-986-visitation.md) | Compliant and approved |
| #989 | [Current defaults](2026-10-19-issue-989-default-selection.md) | Compliant and approved |
| #988 | [Schema probe](2026-10-19-issue-988-schema-probe.md) | Compliant and approved |
| #984 | [Baseline restoration](2026-10-19-issue-984-baseline-restoration.md) | Compliant and approved |
| #987 | [Finally transfer work](2026-10-19-issue-987-finally-work.md) | Compliant; one Important finding fixed and re-review approved |

Each task records five distinct self-review passes for requirements/design, planning, implementation and verification. Tests use explicit arrange/act/assert phases, separately generated parameter cases or bounded properties, and independent expectations. Independent task review is separate from those self-reviews.

The first #987 review found an observable residual-import regression: first-occurrence normalization changed a return-state sequence A/B/A into A/B. An unreachable annotation could then lose import provenance. The fix preserves stable last-occurrence order; its reproduction, residual-state checks and differential coverage are recorded with the task. Scoped re-review accepted the fix and found no new breakage. No task findings remain open or parked.

## Integration verification

The final production code is commit `7bf4cd3b6b9537539911a8c43ee29529bf92a230`; the subsequent integration commit changes documentation only. Cargo-capable checks use the isolated target and two build jobs. Full subprocess suites use two test threads.

| Check | Result |
| --- | --- |
| `cargo test --workspace --locked --offline -- --test-threads=2` | Passed: 3,473 top-level cases; 39 existing ignored; 161 summaries |
| Core `--features contracts` | Passed: 503 cases; two existing ignored |
| CLI `--features contracts` | Passed: 2,965 cases; 37 existing ignored |
| CLI reference example tests and `--check` | Passed: seven tests and generated reference matches |
| `python tools/performance_shapes.py gate` | All 35 registered gates passed; #987 work/state gates additionally run in the ordinary analyzer suite |
| Release elif regressions | Passed: six library cases and two public plan cases |
| `maturin build --release --locked --offline` and `tests/wheel_smoke.py` | Passed: installed macOS arm64 wheel, CLI mutation execution and original-file hash preservation |

The wheel was `hoimin-0.13.0-py3-none-macosx_11_0_arm64.whl`, SHA256 `af85eba768e955117729039204dbbb8dc552bd2838de5fc9efef06e1adec136b`, tested on macOS15.7.7 arm64 with Python3.14.7. Its disposable environment and caches were separate from the shared project environment.

The workspace log prints two additional successes from nested child tests; they are excluded from the 3,473 top-level total. The configured pre-change baseline was 3,304 passed and 39 ignored. Workspace and contracts results are separate configurations, not additive counts of unique test cases.

The four normal commit hooks passed on the final Rust source: workspace and vendored-parser formatting, workspace Clippy with all targets/features and warnings denied, and vendored-parser Clippy. The #984 Lean model/proofs and generator build passed; regeneration/check covered 22 strict state-machine cases. Later analyzer-only changes did not alter that model. Relevant #987 Rust flow-oracle comparisons passed 24 cases after its review fix.

Python static checks passed: Ruff formatting (55 files), Ruff lint, and ty with the dedicated locked dev/fuzz environment. The shared environment lacked the optional hypothesmith dependency; only an owned scratch environment was installed. Full pytest ran 1,071 cases: 1,057 passed, eight optional workflow-tool checks skipped, and six failed because the sandbox prevented process inspection or writes to the default uv cache. A scoped rerun with process access and the owned uv cache passed all eight selected cases, covering every failure and two adjacent controls. Across these runs, 1,063 cases passed and eight remained skipped; this was not a single all-green invocation.

The strict documentation-site build and emitted-link validation passed. The first attempt used an external output directory and was rejected by the builder's deliberate cleanup-path guard; the successful build used the newly created worktree-owned `site/` directory.

## Decisions and their costs

1. The previously reviewed Issues were binding specifications, and the user had already authorized implementation through PR creation. Work proceeded without a repeated approval gate. If that interpretation was wrong, the changes remain reversible on the feature branch.
2. Implementation agents ran sequentially, with independent task reviews; read-only design investigation could overlap. Cargo had one owner. This adds coordination overhead but avoids competing builds and edits.
3. Finally reuse is local to one apply_finally call, with one slot and exact same-category normalization. Both optimizations exclude class contexts and observable hooks; hits restore residual imports and annotation recording remains separate. A mistaken equivalence assumption can change annotation semantics, so category, callback and residual controls are required. No general polynomial or RSS guarantee follows.
4. Removing the internal all_legacy API requires no independent version-floor bump under the repository's internal Rust API policy. Source consumers must update their calls. The defaults and saved subsets retain their behavior.
5. Multiply-invalid plan documents need not retain the old DOM's last-duplicate-wins diagnostic priority. They remain rejected, and current valid plans retain strict decoding. Consumers that depend on a particular invalid-input message may observe a change.
6. Baseline restoration also invalidates pre-fix incomplete-session results with fingerprint epoch 13 and a 0.13.0 minimum package version. Published 0.12.0 used epoch 12; a fresh baseline alone would still reuse stored false verdicts. Old incompatible incomplete history requires a new session path or an explicit fresh run. Databases are preserved, with no adapters; other format versions are unchanged.
7. Deep generated fixtures use Python AST validation after a bytecode-compilation attempt over several depths failed with MemoryError. The exact failing case and peak RSS were not captured; an earlier depth20 attribution was not established. The expensive attempt was not repeated. Rust depth20 work/state checks still apply, while successful Python bytecode compilation within that deep family is evidenced only for the depth12/13 release fixtures.
8. The residual-import review finding required stable last-occurrence normalization instead of first-occurrence normalization. Internal vector order is subordinate to preserving the last contributor to each output category and subsequent residual imports. If this reasoning is wrong, annotation provenance or candidate behavior could regress; the fix requires direct, generated and disabled-reference comparisons.

## Integration self-review

1. Scope and interfaces: compared all six task diffs with the approved Issue boundaries. Descriptor admission and schema decoding use the same opened bytes; the restoration phase reuses the owned reset operation and gates dispatch independently of output acknowledgement. No new compatibility adapter or dependency upgrade was introduced.
2. Compatibility and historical provenance: parsed all five package manifests/lockfiles and confirmed 0.13.0. Compared current fingerprint13 and unchanged plan10/report8/session5 constants with the migration notice. Historical epoch11/12 evidence remains labeled as historical. A draft metrics field name that did not exist was corrected to the recorded baseline process duration.
3. Semantic and regression independence: checked the #987 review reproduction and fix evidence, including direct residual facts and full callback/candidate comparisons. The operation gates measure actual work, independently of semantic state-set normalization. The #988 allocator sensitivity test and #984 historical-session sensitivity test demonstrate that their regressions fail when the old behavior is restored.
4. Execution and limits: inspected the logs and distinguished setup failures, genuine RED tests, successful reruns and platform exclusions. Corrected baseline totals from 3,306 printed successes to 3,304 top-level cases because two nested child summaries were duplicated. Recorded the Python bytecode MemoryError without attributing it to an unobserved exact depth or claiming a peak RSS. Cargo remains serialized and the scratch output is isolated.
5. Documents and deliverables: reconciled canonical guides, C4 and six knowledge concepts against committed evidence. Checked 49 knowledge Markdown structures, 15 new source hashes, unique references, 404 local links, the three new anchors and existing inbound links. Existing concept indexes already reach the updated pages. The strict site build passed; the PR draft states the migration and separates local checks from pending platform CI.

## Limits

Local execution was on macOS. Linux cgroup and native Windows behavior require their platform jobs. Lean correspondence covers the modeled lifecycle transitions, not filesystem or operating-system correctness. Candidate allocation measurements are not a process RSS bound. Finally work bounds apply to the measured homogeneous input family; distinct environments and class contexts can require more work. Cancellation remains cooperative.
