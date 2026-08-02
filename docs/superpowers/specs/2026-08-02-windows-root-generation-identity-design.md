# Windows Root Generation Identity Design

## Problem

The Windows run-wide Job Object tracks active and exited roots only by PID. When Windows reuses a PID, cleanup for an older root can remove the newer root or consume its exit state, losing resource-violation classification and causing notification-barrier timeouts.

## Decision

Assign every prepared Windows supervisor a fresh UUID generation ID. Store that ID with the PID and weak signal in each active entry, and store exited generation IDs rather than PIDs. Retain an owned process handle with every registered generation until its exit notification is consumed. Windows therefore cannot recycle the PID while a delayed notification still needs to be matched.

Job Object notifications still identify processes only by PID. Match an exit notification to the oldest currently registered entry for that PID, then move that entry's generation ID to the exited set. Termination and drop detach the supervisor's signal but retain a tombstone and process handle until the delayed exit is consumed. Classification removes only its own completed generation. Before accepting an aggregate active-process-zero notification, query current Job Object accounting and ignore the notification when a newer process is already active.

This preserves a newer root even when it has the same recycled PID as an older root. Notification handling remains ordered by registration, matching Windows PID reuse semantics: a newer process can receive the PID only after the older process exited.

## Verification

Fault-injection unit tests record exits for two root generations sharing one PID, retain a detached old generation until its delayed exit, and reject a stale active-process-zero notification. Existing exit, abnormal-exit, aggregate violation, timeout cleanup, and Windows integration tests must remain green.
