# Issue 618: release manifest input bytes before verification

Shared saved-plan preparation retains its input JSON `Vec<u8>` through target resolution, source hashing, ranking validation and candidate rediscovery, although the parsed `Value` and `PlanManifest` own their contents. Explicitly drop the input bytes immediately after successful typed deserialization. Keep the existing schema precheck, `Value` conversion, error mapping and manifest-path protection from issue 606 unchanged.

This is a lifetime-only change. Schema/ranking versions, unknown-field handling, stale-source/fingerprint diagnostics, candidate tampering checks and selection results remain unchanged. Parse failures already unwind their input buffer. Direct typed deserialization or removing the intermediate Value are outside scope because they could alter version/error behavior and are unnecessary for this fix.

Use a deterministic allocation-lifetime regression rather than a whole-prepare heap threshold, which would depend on independent ranking optimizations. A dedicated test binary tracks the unique manifest-sized read allocation by pointer. Create a valid public plan with 24 list mutations containing 256-KiB literals. On a current-thread Tokio runtime with one blocking thread, occupy that thread and poll public prepare until its first source read is queued. At this suspension point the input buffer must have been observed and freed. Release the thread through an RAII guard, complete preparation, and compare one/multiple selected candidates with public dry-run results. This requires no production test hook or extra input reads.

## Design self-review

1. Traced ownership: `from_slice` creates an owned Value and `from_value` creates an owned manifest; subsequent consumers borrow neither the raw bytes nor their capacity. Keep the protected invocation paths alive separately.
2. Reviewed error order: place the drop after typed conversion, before header/config/selection checks. All existing parsing and validation expressions remain in their prior order, including version guidance and unknown-field rejection.
3. Reviewed regression sensitivity and cleanup: the first queued Tokio source read provides an observable boundary before hashing and rediscovery. Tracking one unique large allocation avoids dependence on RSS or later candidate clones. Require exactly one match and use a release-on-drop blocking guard so RED assertions cannot deadlock runtime shutdown.
