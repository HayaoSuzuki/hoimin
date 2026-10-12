---
type: Decision
title: Second-order mutation adoption
description: Bounded evidence and the first-order comparator for composite mutation proposals.
status: draft
sources:
- id: assessment
  resource: ../superpowers/reports/issue-763-764/README.md
  revision: 3c3a7d6
  working_tree: modified
  sha256: da0624a1c113ab60bc5622a0e4a5d45ebcaa0c86330f4954c9255b3acb353a85
---

# Second-order mutation adoption

The 2026-10-12 assessment declines #763's composite execution infrastructure and
#764's search subsystem for now. It compared 16 selected first-order edits and
59 nonoverlapping pairs across proofreadtools's length bound and hoimin's path
classification. The first-order subset was fully detected, while one pair
survived and exposed the missing `CONTRIBUTING.md` classification assertion.
The existing first-order `string_literal_empty` operator exposes the same gap;
isolated complete-file tests confirmed both survive before, and fail only the
new case afterward. One useful regression case is retained, with no demonstrated
gap exclusive to second-order mutation.[^assessment]

This does not establish that higher-order mutation is generally ineffective.
Subjects are small, operator families are incomplete, broader validation partly
overlaps the original input space, and exhaustive ordering of these finite
spaces does not measure large-scale equal-budget search value. Observed SSHOM
relationships depend on the fixed case set and did not all persist on broader
validation.[^assessment]

Prefer broader existing first-order operators and specification-based test
improvements before adding composite IDs, manifests, workspace restoration and
resume contracts. Reconsider when a concrete, reproducible gap survives a
reasonable first-order comparison and its incremental benefit justifies those
maintenance and execution costs. The dedicated #762 kill-matrix proposal remains
outside this work.[^assessment]

[^assessment]: [Second-order assessment](../superpowers/reports/issue-763-764/README.md), literature, experiment limits, comparator, timings and retained evidence.
