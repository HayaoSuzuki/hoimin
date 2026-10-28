---
type: Decision
title: Slice/division operators and survivor-directed selection evaluation
description: Default-on implementation, bounded comparison evidence and survivor-deferral limits.
status: draft
sources:
- id: implementation
  resource: ../superpowers/reports/issues-1121-1122-1124/implementation.md
  revision: 1efe26f5e193540649cba9473034843f26824723
  working_tree: untracked
  sha256: e840bd8d5dec6883137dab7e31f29057357668205fc3e2af19119dd300727a65
- id: implementation-review
  resource: ../superpowers/reports/issues-1121-1122-1124/implementation-review.md
  revision: 1efe26f5e193540649cba9473034843f26824723
  working_tree: untracked
  sha256: 59941239e675bfde876b851c42676c71f0c11a9be80f10819c908767727a5266
- id: implementation-code
  resource: ../../crates/hoimin-python/src/analyzer/rust.rs
  revision: 1efe26f5e193540649cba9473034843f26824723
  working_tree: modified
  sha256: bd6d2cb47be1118a9d79146eb66a4ca2da4836c8b88793a70858c26a8295a333
- id: operator-catalog
  resource: ../../crates/hoimin-core/src/config.rs
  revision: 1efe26f5e193540649cba9473034843f26824723
  working_tree: modified
  sha256: 94edd06b6fcf0466042a5491fb95102cac4ef51745e4a770bf9e4136faec4664
- id: report
  resource: ../superpowers/reports/issues-1121-1122-1124/README.md
  revision: 1efe26f5e193540649cba9473034843f26824723
  working_tree: modified
  sha256: b52f23a7821511deeb1ff895d2db4b09379112e40043c30db7b8c6da78a58b02
- id: experiment
  resource: ../superpowers/reports/issues-1121-1122-1124/experiment.py
  revision: 7c2a79168c8698f85cc90d7efd6bf16a183e08b3
  working_tree: untracked
  sha256: ebb13e5af121930bf84aec51ebfa368807cff315aac1fcffc1547bad0d8734c8
- id: python314
  resource: ../superpowers/reports/issues-1121-1122-1124/results-python314.json
  revision: 7c2a79168c8698f85cc90d7efd6bf16a183e08b3
  working_tree: untracked
  sha256: 44c94771fa5790cd376a94d6ac743e90dd848e835a87edad5111024730c6cf41
- id: python315
  resource: ../superpowers/reports/issues-1121-1122-1124/results-python315.json
  revision: 7c2a79168c8698f85cc90d7efd6bf16a183e08b3
  working_tree: untracked
  sha256: 54c4047b3b930d529fffcd843a821850bab11f63ebba8f7aa80ab56cf156f698
---

# Decisions and product policy

The 2026-10-28 evaluation led to narrowly scoped implementations of slice-bound
omission/movement (#1121) and true/floor division exchange (#1124). Following the
maintainer's [product direction](product-direction.md), adopted operators should
normally be enabled by default and removable with `--exclude-operators`.
The subsequent implementation adds `binary_div_floor`,
`structure_slice_bound_omit` and `structure_slice_bound_move` to the default
runtime set. Both slice IDs belong to `structure_ops`; each can be excluded
individually. The current catalog contains 94 IDs and 75 defaults.[^implementation][^operator-catalog]

Slices require load context, one dimension, no step value and simple name/integer
bounds. Omission removes one complete grouped bound; movement preserves trivia
and moves the colon. Division changes a binary operator token while preserving
existing multiplication/modulo candidates. All reuse existing selection,
cancellation, payload accounting and bounded admission. No scheduler or saved
schema change accompanies these operators.[^implementation-code][^implementation]

Variable-bound slices expose range requirements outside the current numeric
neighbor pool. Literal slices overlap existing neighbors, and omission can
produce equivalent behavior. True/floor exchange exposes fractional and
negative-input requirements that multiplication/modulo mutants do not expose
on these examples. Unsupported overloads raise runtime errors and are not counted
as useful missing-assertion leads.[^report][^python314][^python315]

Do not add survivor-triggered scheduling (#1122) on the present evidence. With
the same 12-candidate pool, line suppression executes seven candidates but
misses an independent requirement on the same line. Exact-span suppression
executes ten and retains both useful requirements. Existing line diversity
already finds both; neither new strategy increases useful requirements at
ceilings 4, 8 or 12.[^python314][^python315]

# Evidence and limits

The adapter uses current `plan` and public line-diverse preview ordering, direct
weak/strong controls, and real `verify` calls. Adaptive replays use completed CLI
outcomes, paying a new baseline per chosen ID. Their traces keep unexecuted and
deferred IDs separate from verification verdicts. Per-call completeness covers
only selected IDs and does not verify the entire pool.[^experiment]

The nine independent small examples ran on macOS arm64 with CPython 3.14.9 and
3.15.0b1. These are limited empirical results, not real-project efficacy,
universal equivalence, Linux/Windows results, hard macOS memory guarantees or a
Ruff operator implementation. The new implementation has separate
Rust/CLI/CPython checks recorded with their actual host and interpreter scope;
the old JSON observations are not rerun-product evidence.[^implementation-review]
Revisit the selection decision only with evidence that its useful test
improvements justify additional report/resume/concurrency contracts.[^report]

[^implementation]: [Current operator implementation and scope](../superpowers/reports/issues-1121-1122-1124/implementation.md).
[^implementation-review]: [Implementation self-review and validation](../superpowers/reports/issues-1121-1122-1124/implementation-review.md).
[^implementation-code]: [Static candidate generation](../../crates/hoimin-python/src/analyzer/rust.rs).
[^operator-catalog]: [Typed operator catalog, families and defaults](../../crates/hoimin-core/src/config.rs).
[^report]: [Research report, decisions and minimum follow-up scope](../superpowers/reports/issues-1121-1122-1124/README.md).
[^experiment]: [Disposable experiment adapter](../superpowers/reports/issues-1121-1122-1124/experiment.py).
[^python314]: [Python 3.14.9 execution evidence](../superpowers/reports/issues-1121-1122-1124/results-python314.json).
[^python315]: [Python 3.15.0b1 execution evidence](../superpowers/reports/issues-1121-1122-1124/results-python315.json).
