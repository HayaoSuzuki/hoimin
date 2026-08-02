# Root-level mutation candidate path design

## Problem

The focused-mutation workflow indexes the second component of every candidate
path as a workspace package. A root-level path has no second component, causing
an uncaught `IndexError`; the final checkpoint then persists a record still
marked `running` even though the workflow has crashed.

## Design

Centralize workspace-package extraction in a helper that returns a package only
for `crates/<package>/...` paths. Package-scoped pending updates and the main
candidate loop use the same helper.

When a candidate is outside a workspace member, mark only that candidate
`not_run` with reason `outside_workspace_member`, checkpoint the decision, and
continue with other candidates. This avoids treating an inventory-shape issue
as a tool outage and guarantees the workflow reaches a terminal state without
running a baseline or mutation command for the invalid candidate.

## Tests

A workflow-level regression injects a root-level inventory candidate and asserts
that the workflow completes, the candidate is persisted as `not_run` with the
explicit reason, and no baseline or mutation command is launched. Existing
workflow tests continue to cover valid workspace-member paths.
