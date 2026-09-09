# Issue 447: Bound fingerprint file retention

## Contract and baseline

main58817cfa76bcb7ce47e3bd7c1bb1109ee917df19 retains the bytes of every exact fingerprint file until all exact reads finish. Replace that aggregate retention with hashes, preserving resolve's public API, error categories, sorted output and read validation. User authorizes autonomous execution in .worktrees/issue-447-fingerprint-memory, branch perf/issue-447-fingerprint-memory, with three self-reviews each for design and plan. Implementation follows the #446 PR.

The existing CLI measurement, same16MiB files and three samples per condition, shows exact-file peak RSS growing from30.8MB for1file to148.3MB for8files; equivalent glob input stays around31MB. Hashes and candidates match. This is retained-input memory, not an observed OOM or a claim about process resource limits.

## Alternatives and decision

1. Add memory-limit failures or restrict the number of fingerprint files: changes valid-input behavior without removing unnecessary retention. Rejected.
2. Rewrite root-relative reading into a streaming hashing API: potentially reduces even single-file memory but broadens the security-sensitive reading surface unnecessarily for this defect. Defer that independent concern.
3. Hash each exact file immediately using bytes from the existing safe read, and keep only a digest in the existing ordered map. Selected.

Use BTreeMap<Utf8PathBuf, Option<blake3::Hash>>. Glob matches insert None as today. Each exact input normalizes and reads through read_root_relative, calculates its digest, then inserts Some(hash); its temporary bytes drop before the next file is read. In the final ordered traversal, reuse Some(hash) or read/hash a None entry, then emit the unchanged FingerprintInputFile { path, hash: digest.to_hex().to_string() }.

Keep exact inputs in their existing iteration order and do not skip repeated reads solely because the normalized path was already selected. This preserves both error precedence and last-observed exact contents. Glob/exact overlap still uses the exact read's digest. No process-global cache or retention across rechecks.

## Bounds and compatibility

Peak source-byte retention becomes O(largest selected file), plus O(selected path/hash metadata), instead of O(total exact file bytes). Existing per-file read allocation remains. No new dependency, unsafe code, public API, schema or fingerprint algorithm changes. Rust MSRV1.88. Root-relative read checks, symlink-parent rejection, non-regular file rejection and errors remain in the same path. Source comments only when needed; variable names should distinguish digest from bytes.

## Validation

Preserve existing fingerprint input, CLI configuration, plan/verify and workspace tests. Add binary contents, normalized exact aliases, duplicates, overlap with glob and differently ordered exact selections, comparing complete expected path/hash records and repeated calls after file updates. Existing unreadable/missing/symlink tests remain essential. The performance RED is the reproduced CLI RSS growth; compatibility tests are intentionally expected to pass before and after because public behavior does not change.

Run the same actual CLI memory experiment on before/after binaries, 1/4/8files of16MiB, three paired samples; compare fingerprint records/candidates under the same fixture root. Report RSS measurement conditions and medians without CI time/RSS thresholds. Do not add an unsafe allocator just to enforce a platform-specific RSS expectation. Run full workspace/all-features, MSRV/all-targets/all-features, Clippy warnings denied, fmt/diff, independent task and final reviews, then PR.

## Design self-review 1 — lifetime and error order

Traced resolve's two phases. Exact bytes drop at each loop iteration after hashing, while glob reads remain in ordered final traversal. Hashing is infallible, so moving it earlier does not alter which path error is returned. Repeated exact entries must still read again; deduplicating reads would change observed updates and error ordering.

## Design self-review 2 — memory and safety scope

An Option<blake3::Hash> is fixed-size metadata and cannot retain the read buffer. The reader still bounds lifetime to one whole file, so the claim is O(max file size), not constant-byte streaming. No cache survives resolve/recheck, and the root-relative reader is unchanged. An unsafe allocation tracker is unnecessary for the accepted reproducible memory evidence.

## Design self-review 3 — output and integration

Confirmed BLAKE3 digest-to-hex output is identical whether computed during exact read or final map traversal. Exact/glob overlap retains the most recent exact digest and does not add a second read. Public callers and recheck compare the same sorted FingerprintInputFile records. The design addresses aggregate retention without expanding to unrelated session caching or source reading changes. No unresolved requirement remains.
