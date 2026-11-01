# Issue #442: bounded candidate spool records

## Scope and architecture

`CandidateStore::push` currently gives a `File` directly to the JSON serializer. A representative record results in 94 small Writer calls. Keep the existing preflight serialized-size count, but serialize the accepted candidate and its newline into one bounded record buffer, then call `write_all` on the file. The record is complete in memory before touching the spool. Do not buffer across push calls or change replay/cursor APIs.

The existing maximum record size is 2 MiB including newline. Counting before allocating makes the allocation explicitly bounded and preserves the existing oversize error behavior. The temporary buffer is local to one push; no complete-run candidate collection is introduced. Preserve candidate sequence, max-candidates checks, JSON bytes, synchronous per-push error reporting, and finish's flush/sync/keep ordering. Do not change public schemas or dependencies. Support Rust 1.88 and minimize source comments.

Alternatives considered: a persistent BufWriter would postpone failures and complicate finish/drop ownership; per-record BufWriter reduces syscalls but can write partial serialized content and retry writes during Drop after an error. A fully assembled bounded record has simpler failure boundaries and avoids buffered Drop I/O. Retaining the size-count pass is deliberate: removing it is not necessary to solve filesystem write amplification and would need a separate bounded serializer design.

## Failure contract

Validation/size failures must not touch the file or advance counts. A file write failure must surface as StoreError::Io before counts advance. Existing callers abort and drop the owned temporary store on I/O failure; this change does not add retry/resume of a failed push or claim transactionally atomic disk writes. There is no deferred record buffer to flush from Drop. Normal finish still reports flush/sync/persist errors. Demonstrate actual file write failure with a read-only file handle through NamedTempFile::from_parts rather than relying on permissions that privileged users can bypass.

## Validation

Preserve the existing Unicode, escaped-string, record-size/newline boundary, sequence, cursor, and replay tests. Add evidence that a validated record is submitted as a complete JSONL byte sequence and that file I/O errors preserve count and clean up owned temporary storage. Use a small private write helper only if it separates a real production responsibility and enables checking writes/short writes; do not build a general sink abstraction solely for tests.

Compare exact baseline source to final production source in release mode with 1,000 and 10,000 candidates, including create/push/finish, asserting exact spool bytes. Report three-pair medians, workload details, and ranking/test execution exclusions. Do not add wall-time thresholds to CI.

## Design self-review 1 — memory bound

An unchecked serde_json::to_vec could allocate beyond the existing record ceiling. Retained the exact counting pass and allocate only after it succeeds; maximum payload plus newline remains bounded. A local buffer avoids memory growth with the number of candidates.

## Design self-review 2 — failure timing and ownership

Persistent buffering would change when write errors appear and require new shutdown semantics. Chose one complete per-push record, direct write_all, and unchanged finish. Explicitly documented existing abort-on-I/O behavior instead of claiming atomic writes or expanding scope into recovery.

## Design self-review 3 — evidence and compatibility

Timing alone cannot verify contents or justify a complexity claim. Require exact bytes, existing boundary/replay tests, and a real read-only-handle failure. Expected gain is fewer file Writer calls, not a change in asymptotic candidate traversal. Keep the second serialization pass because it is not the measured bottleneck.
