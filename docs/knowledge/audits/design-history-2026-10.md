---
type: Audit
title: October 2026 design-audit history
description: Archived design reviews, resolved findings and conditions for reconsidering
  earlier decisions.
status: draft
catalog_revision: fd5b68febcbbb7405d0ab4d0127b0c306b3ca5a4
sources:
- id: archive
  resource: ../../maintenance/2026-10-21-audit-archive-934-964/README.md
  revision: fd5b68febcbbb7405d0ab4d0127b0c306b3ca5a4
  working_tree: untracked
  sha256: 3d220ce09ee5d125fb4f8f1c8592026f9f6a098ded597c9466294353d647307c
- id: manifest
  resource: ../../maintenance/2026-10-21-audit-archive-934-964/manifest.json
  revision: fd5b68febcbbb7405d0ab4d0127b0c306b3ca5a4
  working_tree: untracked
  sha256: 53eea5b8578410916e9c5fca6bbfa78684738134c6a22dd0bb423cce1c226099
- id: imports
  resource: ../../../crates/hoimin-python/src/analyzer/rust.rs
  revision: fd5b68febcbbb7405d0ab4d0127b0c306b3ca5a4
  working_tree: clean
  sha256: 38c8753ee29f41504dab88b815296a7425b5c4495add87ec0a6ea8223e5aa912
- id: finalization
  resource: ../../../crates/hoimin-cli/src/shell/finalization.rs
  revision: fd5b68febcbbb7405d0ab4d0127b0c306b3ca5a4
  working_tree: clean
  sha256: 37ad268c0f2b63999caa0223623d60bd3d001ec7bbde6912157eb658e42ba369
- id: sequence
  resource: ../../../crates/hoimin-core/src/report.rs
  revision: fd5b68febcbbb7405d0ab4d0127b0c306b3ca5a4
  working_tree: clean
  sha256: c1213c52126df94d78243bb7f0fb66e1505f9f6a365be99162053536686d75ee
- id: tracked
  resource: ../../../crates/hoimin-cli/src/target/git/tracked.rs
  revision: fd5b68febcbbb7405d0ab4d0127b0c306b3ca5a4
  working_tree: clean
  sha256: 6f530052362f3dc7ff11717feab2bdcdcd265bd4d83e2d08bbf0dc2f40cb6b5c
- id: linux
  resource: ../../../crates/hoimin-cli/src/resource/linux.rs
  revision: fd5b68febcbbb7405d0ab4d0127b0c306b3ca5a4
  working_tree: clean
  sha256: 476ccf7f1dfe90507af7618ea6f5dbe2d5c092bd0a5349e3ff4839cba6b8b854
- id: payloads
  resource: ../../../crates/hoimin-cli/src/artifacts/run_report.rs
  revision: fd5b68febcbbb7405d0ab4d0127b0c306b3ca5a4
  working_tree: clean
  sha256: f6947f2d3ce54be207c85e92c64ebe22d6d4ecbee939a02e7ae425c976ae56cf
---

# Why retain these records

Eight local audit branches from 2026-10-16 and 2026-10-17 contained nine
documentation commits not present in `main` at
`fd5b68febcbbb7405d0ab4d0127b0c306b3ca5a4`. The source archive preserves every
parent-to-commit diff, including changes to existing concepts, and identifies
each review's target revision. This page is a synthesis checked against that
baseline on 2026-10-21, not a new whole-repository audit.[^archive][^manifest]

The records are useful for the reasoning behind adopted changes and rejected
alternatives. In particular, they distinguish duplicated ownership from
independent observations that happen to use similar fields. Their old open-item
lists and numerical design scores are not current status or comparable quality
measurements. All 20 issues raised by these eight audits were closed as completed
when their GitHub disposition was checked on 2026-10-21.[^manifest]

# Findings and disposition

The table groups findings by the source archive's branch labels. Issue and PR
links identify the implemented follow-ups; closure is not a claim that their tests
were rerun for this documentation change.[^manifest]

| Historical review | Findings | Closing PRs |
| --- | --- | --- |
| Initial follow-up | [#929](https://github.com/HayaoSuzuki/hoimin/issues/929): producerless diagnostics; [#930](https://github.com/HayaoSuzuki/hoimin/issues/930): typed token operators; [#931](https://github.com/HayaoSuzuki/hoimin/issues/931): one candidate owner | [#932](https://github.com/HayaoSuzuki/hoimin/pull/932), [#933](https://github.com/HayaoSuzuki/hoimin/pull/933), [#934](https://github.com/HayaoSuzuki/hoimin/pull/934) |
| After #934 | [#935](https://github.com/HayaoSuzuki/hoimin/issues/935): exclusive import bindings; [#936](https://github.com/HayaoSuzuki/hoimin/issues/936): one fingerprint-input list | [#937](https://github.com/HayaoSuzuki/hoimin/pull/937), [#938](https://github.com/HayaoSuzuki/hoimin/pull/938) |
| After #938 | [#939](https://github.com/HayaoSuzuki/hoimin/issues/939): duplicate JSON keys; [#940](https://github.com/HayaoSuzuki/hoimin/issues/940): synthetic outcomes; [#941](https://github.com/HayaoSuzuki/hoimin/issues/941): shared stop transitions; [#942](https://github.com/HayaoSuzuki/hoimin/issues/942): workspace registry ownership; [#943](https://github.com/HayaoSuzuki/hoimin/issues/943): unused counters/state | [#945](https://github.com/HayaoSuzuki/hoimin/pull/945), [#944](https://github.com/HayaoSuzuki/hoimin/pull/944), [#946](https://github.com/HayaoSuzuki/hoimin/pull/946), [#947](https://github.com/HayaoSuzuki/hoimin/pull/947), [#948](https://github.com/HayaoSuzuki/hoimin/pull/948) |
| After #948 | [#949](https://github.com/HayaoSuzuki/hoimin/issues/949): one normalized root; [#950](https://github.com/HayaoSuzuki/hoimin/issues/950): typed candidate operators; [#951](https://github.com/HayaoSuzuki/hoimin/issues/951): dispatch identity from requests | [#953](https://github.com/HayaoSuzuki/hoimin/pull/953), [#952](https://github.com/HayaoSuzuki/hoimin/pull/952), [#954](https://github.com/HayaoSuzuki/hoimin/pull/954) |
| After #954 | [#955](https://github.com/HayaoSuzuki/hoimin/issues/955): output continuations in pending effects; [#956](https://github.com/HayaoSuzuki/hoimin/issues/956): one cleanup classification | [#957](https://github.com/HayaoSuzuki/hoimin/pull/957), [#958](https://github.com/HayaoSuzuki/hoimin/pull/958) |
| After #958 | [#959](https://github.com/HayaoSuzuki/hoimin/issues/959): one offset-based Top selection; [#960](https://github.com/HayaoSuzuki/hoimin/issues/960): collector sink/error state | [#961](https://github.com/HayaoSuzuki/hoimin/pull/961), [#962](https://github.com/HayaoSuzuki/hoimin/pull/962) |
| After #962 | [#963](https://github.com/HayaoSuzuki/hoimin/issues/963): remove unused disk statistics and derive the global minimum | [#964](https://github.com/HayaoSuzuki/hoimin/pull/964) |
| After #964, including follow-up | [#966](https://github.com/HayaoSuzuki/hoimin/issues/966): remove duplicate bound allowance; [#967](https://github.com/HayaoSuzuki/hoimin/issues/967): remove inert Windows violation state | [#968](https://github.com/HayaoSuzuki/hoimin/pull/968), [#970](https://github.com/HayaoSuzuki/hoimin/pull/970) |

# Decisions worth carrying forward

Use types to express alternatives that actually exclude each other. The import
binding change replaced parallel categories with one binding per local name;
the baseline implementation has `KnownBinding::{Direct, Module, TypeVar}` in a
single map. This removes a maintained consistency rule rather than merely
renaming fields.[^archive][^imports]

Independent observations need not become one lifecycle enum. Reaping a process,
draining its pipes, joining a monitor and attempting cleanup answer different
questions. The historical audits retained these distinctions. At the comparison
baseline, `ExecutionFinalization` groups lifecycle, measurement, cleanup and
end-availability ownership while preserving their separate evidence. Ownership
can be centralized without conflating the facts being observed.[^archive][^finalization]

Likewise, `ReportSequence` uses its active set for unfinished `(ID, sequence)`
pairs and its seen map for retained identity history. Combining the collections
would require preserving the distinction and error precedence, not just reducing
the field count. The same-revision post964 follow-up records that tradeoff; the
baseline source still has both collections and their separate consumers.
This is rationale to revisit when access patterns change, not a ban on another
representation.[^archive][^sequence]

Removing compatibility requirements does not remove every current fallback or
validation boundary. `DiffPlan::Legacy` is a live Git-discovery path for cases
such as rename/copy/unmerged entries and long pathspecs, rather than a saved-file
migration adapter. The old audit's recommendation to retain it must be evaluated
against those current cases. Similarly, deleting the inert Windows violation
carrier does not justify deleting Linux's `RootSignal`: the Linux source writes
new violation bits and consumes them during classification.[^archive][^tracked][^linux]

# Earlier decisions that changed later

Historical decisions remain conditional. These later completed changes illustrate
why an old rejection should not become a permanent design rule:[^manifest]

| Earlier assessment | Later change |
| --- | --- |
| Keep `all_legacy` because it has live default/fuzz callers. | [#989](https://github.com/HayaoSuzuki/hoimin/issues/989), [PR #990](https://github.com/HayaoSuzuki/hoimin/pull/990) removed that historical operator-cohort API and defined the current default directly. |
| Retain independent shell cleanup and end-observation facts. | [#1051](https://github.com/HayaoSuzuki/hoimin/issues/1051), [PR #1057](https://github.com/HayaoSuzuki/hoimin/pull/1057) centralized their owner; the facts remain distinct in `ExecutionFinalization`. |
| Additional report payload types would add projection layers without demonstrated benefit. | [#1055](https://github.com/HayaoSuzuki/hoimin/issues/1055), [PR #1058](https://github.com/HayaoSuzuki/hoimin/pull/1058) addressed concrete buffering/retention costs and introduced `ReportPayloads` conversion before structural validation. |
| Current schema validation is useful, independently of migration support. | [#1056](https://github.com/HayaoSuzuki/hoimin/issues/1056), [PR #1058](https://github.com/HayaoSuzuki/hoimin/pull/1058) removed a later old-version diagnostic probe/reparse. Retaining validation does not require preserving every historical diagnostic route. |

The selected baseline checks here covered `ExecutionFinalization` and
`ReportPayloads` against their current source. The other later changes are
recorded by their GitHub disposition; this consolidation does not newly certify
every changed implementation path.[^finalization][^payloads][^manifest]

The old references to [#743](https://github.com/HayaoSuzuki/hoimin/issues/743)
(conditional language separation), [#806](https://github.com/HayaoSuzuki/hoimin/issues/806)
(fuzz-tool feasibility) and [#570](https://github.com/HayaoSuzuki/hoimin/issues/570)
(dependency dashboard) are background work. They were open on 2026-10-21 and are
not unresolved findings from these eight audits.[^manifest]

# Evidence limits and recheck triggers

Most original evidence is static inspection. The post938 record additionally
reports a bounded macOS duplicate-JSON-key reproduction with a recorded binary
revision and input/outcome comparison. Its temporary files are not archived,
and the reproduction was not rerun here. Original self-review statements and
documentation checks remain historical claims at their recorded revisions.
No fresh implementation tests, benchmarks, native Windows/Linux checks or Lean
proofs were run for this consolidation.[^archive]

New validation covers patch hashes, reconstruction of the nine original Git
trees, documentation structure/references, selected source checks and the dated
GitHub status snapshot. It establishes preservation and scope, not execution
correctness or a measured speed/memory improvement. Read the
[Lean evidence guide](lean-evidence.md) before interpreting model results.
Formal proof sources remain in their established locations.[^archive]

Recheck the reasoning when an observation gains or loses a consumer, identity or
error precedence changes, a fallback's supported cases disappear, or measurement
shows a concrete cost. Read the source archive's manifest before using an old
source path: later language separation moved analyzer files into `hoimin-python`.
Use current Issues and implementation for new work rather than restoring old
pending lists or treating old scores as current assessments.[^archive][^imports]

[^archive]: [Source archive and reconstruction procedure](../../maintenance/2026-10-21-audit-archive-934-964/README.md).
[^manifest]: [Original revisions, source digests and dated issue snapshot](../../maintenance/2026-10-21-audit-archive-934-964/manifest.json).
[^imports]: [Current import bindings](../../../crates/hoimin-python/src/analyzer/rust.rs); comparison revision and digest recorded above.
[^finalization]: [Current finalization ownership](../../../crates/hoimin-cli/src/shell/finalization.rs); comparison revision and digest recorded above.
[^sequence]: [Current report-sequence checks](../../../crates/hoimin-core/src/report.rs); comparison revision and digest recorded above.
[^tracked]: [Current Git diff planning](../../../crates/hoimin-cli/src/target/git/tracked.rs); comparison revision and digest recorded above.
[^linux]: [Linux violation observations](../../../crates/hoimin-cli/src/resource/linux.rs); comparison revision and digest recorded above.
[^payloads]: [Current report payload conversion](../../../crates/hoimin-cli/src/artifacts/run_report.rs); comparison revision and digest recorded above.
