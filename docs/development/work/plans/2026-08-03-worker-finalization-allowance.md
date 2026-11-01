# Worker finalization allowance rollback

Issue: #148

## Problem

`materialize_worker` charges every copied byte before constructing the final
`WorkerWorkspace`. If `WorkerRoot::open` fails, no `WorkerWorkspace` exists to
release that charge in `Drop`, so a retry can exhaust an otherwise sufficient
copy grant.

## Design

Open `WorkerRoot` immediately after creating the empty worker directory and
before copying or charging any bytes. Once copying begins, final
`WorkerWorkspace` construction is infallible because it receives the already
opened root. This removes the post-copy failure window instead of trying to
roll back accounting before confirming that a copied directory was deleted.

## Verification

- A unit test injects a root-open error and verifies that no bytes were charged
  and the empty temporary workspace was removed.
- Existing workspace tests verify that successful workers retain and later
  release their charge through their normal lifecycle.
