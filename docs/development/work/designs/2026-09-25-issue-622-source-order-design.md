# Issue 622: ordered source roots in resume compatibility

The worker builds PYTHONPATH from configured source roots in order, while resume currently records only unordered source file hashes. Reversing two roots containing the same module can therefore reuse an obsolete verdict. Add ordered `source_roots` to `FingerprintInput`, populated from normalized `RunConfig.selection.sources`. Encode the count and each length-prefixed UTF-8 path in field 12 without sorting or deduplication. Keep the existing unordered source-content field and ordered import roots unchanged.

Bump fingerprint schema 8 to 9. Existing sessions remain stored but cannot match the new digest; there is no destructive database migration. Current limits, result reuse policy and report shape remain unchanged. Conservative invalidation for different root spellings or repeated roots is acceptable; only identical normalized configured order is guaranteed reusable. Preserve the existing worker import precedence.

Integrate the audit's eight Lean cases (source/import roots, AB/BA initial and resumed order) into the formal library, generator, committed corpus, CI freshness/sensitivity checks and a public run adapter using isolated SQLite databases. The model proves only finite order/verdict compatibility. Test actual baseline, statuses, execution counts, run identity and termination metadata; do not derive expected verdicts in the adapter.

## Design self-review

1. Dependency review: traced config to worker PYTHONPATH and fingerprint construction. The source hash collection is a set and cannot carry import precedence, so introduce a separate ordered field rather than change its established set semantics.
2. Compatibility review: a new field alone changes digests, but schema version is also observable in sessions. Increment to 9 and retain old database rows; leave jobs/max-output neutrality and conclusive-result reuse unchanged.
3. Boundary review: ordered length framing distinguishes root boundaries and source roots from import roots. Existing config normalization remains authoritative; avoid adding filesystem canonicalization, which would introduce new I/O and alias semantics. Keep fresh-run and same-order controls in the public adapter.
