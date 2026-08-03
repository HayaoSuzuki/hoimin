# Worker finalization allowance rollback

Issue: #148

## Problem

`materialize_worker` charges every copied byte before constructing the final
`WorkerWorkspace`. If `WorkerRoot::open` fails, no `WorkerWorkspace` exists to
release that charge in `Drop`, so a retry can exhaust an otherwise sufficient
copy grant.

## Design

Route the final constructor result through one transaction boundary. A success
transfers responsibility for the charge to `WorkerWorkspace`; an error releases
the complete charge immediately. The owned temporary directory continues to
clean itself up while unwinding the failed constructor.

## Verification

- A unit test charges an allowance, injects a finalization error, and verifies
  that the charge returns to zero while preserving the original error.
- Existing workspace tests verify that successful workers retain and later
  release their charge through their normal lifecycle.
