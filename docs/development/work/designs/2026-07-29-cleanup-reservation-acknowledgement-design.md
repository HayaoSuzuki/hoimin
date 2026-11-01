# Cleanup Reservation Acknowledgement Design

Cleanup completion is a contract acknowledgement, not merely a notification. The pending cleanup
effect retains the reservation IDs it asked the executor to release. Before retiring that pending
effect, the machine compares the acknowledgement with the request as sets while separately rejecting
duplicates. A mismatch returns a typed machine error without mutating pending effects, grants, budget
accounting, or completion state.

The comparison belongs in `accept_completion`, before its commit point, so every cleanup transition
gets the same validation and an invalid event remains retryable with a correct acknowledgement.

