---
type: Contract
title: Disk failure representation
description: Required disk stop payloads, secondary evidence, and saved failure format.
status: draft
sources:
- id: disk-monitor
  resource: ../../../crates/hoimin-cli/src/workspace/disk.rs
  revision: 31245fbf096967b36bc15523067080b506f88fe9
  working_tree: modified
  sha256: 63793e1f410978f06ffffdcde140ead0b3003eb498cf341303056e79299c575f
- id: disk-shell
  resource: ../../../crates/hoimin-cli/src/shell.rs
  revision: 31245fbf096967b36bc15523067080b506f88fe9
  working_tree: modified
  sha256: 18818d2e6f03a92f857add3ca92d191017af9f20a4ab46d446655f88cbec05df
- id: disk-failure
  resource: ../../../crates/hoimin-core/src/disk.rs
  revision: 31245fbf096967b36bc15523067080b506f88fe9
  working_tree: modified
  sha256: 582eaaa3b7bf721bd5ae5fb904cabb0fd1f934b9bbc23b2bfced1bf5326e8650
- id: disk-report
  resource: ../../../crates/hoimin-core/src/report.rs
  revision: 31245fbf096967b36bc15523067080b506f88fe9
  working_tree: modified
  sha256: 7884984d3eaa62d0bb0534c8682f9ab867e2fe747bdcfc823191b456ddeb2775
- id: issue-900-design
  resource: ../../superpowers/reports/issue-900/design.md
  working_tree: untracked
  sha256: 7c10b5c8a7898a374e985e773d4c758478b02e7927d9fd2e7b3873684fe49d50
---

# Disk failure representation

This reference explains the disk stop representation for maintainers changing policy, monitor, and report code.

`DiskFailure` contains a `DiskFailureReason` and independent `secondary` diagnostics. `WorkspaceSizeExceeded` and `FilesystemReserveReached` require a `DiskObservation`; `MeasurementFailed` and `ProcessFailed` require a message. The failure code derives from the reason. `DiskStopReason` supplies payload-free labels for secondary observations and formal oracle projections.[^disk-failure]

`DiskFailureReason::to_secondary` preserves the complete observation, including measurement duration. Lifecycle and monitor handling keep the first stop and deduplicate only complete matching evidence. A simultaneous reserve and size violation selects reserve first and records size as secondary evidence. Cleanup diagnostics keep their independent codes and messages.[^disk-failure][^disk-monitor]

`DiskStopReport::from` projects the typed payload to report fields. Observation reasons populate both byte counts; error reasons populate the message. The report format retains its existing fields and existing projection-based matching.[^disk-report][^disk-shell]

## Saved failure format

The failure JSON uses a payload-bearing `reason` object. For example:

```json
{"reason":{"measurement_failed":"measurement unavailable"},"secondary":[]}
```

The former independent `code`, `observation`, and `message` fields are rejected. Old saved failure events need regeneration; no legacy conversion or missing-payload defaults are provided. This change does not alter the separate disk report JSON shape.[^issue-900-design]

## Contract verification

Use the [disk failure checks](../../development.md#disk-failure-contracts) to check typed payloads and existing formal corpus projections. The Lean lifecycle model uses payload-free stop labels; its transition relation and corpus remain unchanged.

[^disk-failure]: [Typed disk policy and lifecycle](../../../crates/hoimin-core/src/disk.rs).
[^disk-report]: [Disk report projection](../../../crates/hoimin-core/src/report.rs).
[^issue-900-design]: [Issue #900 design and wire decision](../../superpowers/reports/issue-900/design.md).

[^disk-monitor]: [Monitor first-stop and complete-evidence matching](../../../crates/hoimin-cli/src/workspace/disk.rs).
[^disk-shell]: [Shell report evidence merging](../../../crates/hoimin-cli/src/shell.rs).
