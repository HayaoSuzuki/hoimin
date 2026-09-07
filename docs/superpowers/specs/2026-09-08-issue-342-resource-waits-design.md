# Resource wait isolation (#342)

OS resource waits must not hold the backend-wide registry mutex or run on a Tokio dispatcher thread. Linux launcher attachment already waits outside the registry lock; its spawn/admission transaction is unchanged.

Linux termination reserves a per-root cleanup gate, snapshots accounting under the registry lock, and hides the cleaning root from counter reads. Physical cleanup runs unlocked. Successful cleanup removes the registration; failure restores its previous accounting eligibility. Closing the run prevents new admission before acquiring root gates outside the registry lock. All captured gates remain owned through recursive run-directory cleanup. A separate close gate serializes close retries.

Windows classification retains an owned process handle, waits outside the registry lock, and then drains queued notifications under the lock. Consumed exit notifications take precedence over the signaled-process fallback. The fallback keeps the registration's PID-reuse barrier until the delayed notification arrives.

The process lifecycle transfers its unique supervisor into blocking operations for classification, termination and quiescence checks. No borrowed supervisor crosses that boundary. Cancellation of an awaiting future must neither destroy an in-flight supervisor nor acknowledge cleanup; destruction also belongs to the blocking executor. Spawn and attachment keep their existing admission guards and error precedence.

Constraints: Rust 1.88; no added dependencies; preserve accounting-primary and process-primary errors, retryability, cleanup ownership, and late-notification generation safety. Source comments remain limited to safety or non-obvious contracts.

Verification combines deterministic ownership/progress tests, existing process/resource regressions on macOS, Linux-specific regressions, and Windows CI. Model reasoning is not a claim that the kernel implementation is proved. No wall-clock speedup benchmark is required: a paused root must permit unrelated registry operations.
