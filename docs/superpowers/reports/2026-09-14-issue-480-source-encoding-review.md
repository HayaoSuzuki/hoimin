# Issue #480: source encoding review

Base: 8b33167. Worktree issue-480, macOS. User authorized autonomous design, implementation and PR. Three self-reviews per stage are distinct checks below; no subagents were dispatched.

## OKF stage

1. Read analyzer, selection, overview, development and OKF workflow with their source references. Existing source-span contract assumes UTF-8; this change requires an explicit original-byte/decoded-text distinction.
2. Traced discovery, direct run, candidate validator and workspace mutation. Found three independent UTF-8 assumptions: decode, original-span compare, replacement write. All require coordinated changes; decoding alone is insufficient.
3. Reviewed source provenance and prior BOM/newline/index decisions. Preserve their UTF-8 guarantees and original-byte hashes, but do not claim existing Lean UTF-8 corpora prove Latin-1 behavior. Add spec/report to their respective inventories.

## Design stage

1. Compared cookie rules against Python 3.14 lexical documentation and CPython tokenizer helpers. Found second-line blank/comment handling, standalone-comment restriction and case-sensitive `coding` spelling; declared them explicitly.
2. Checked BOM normalization separately from codec aliases. CPython permits `utf8` without BOM but requires tokenizer-normalized `utf-8` with BOM. Avoid accepting every UTF-8 alias blindly in BOM mode.
3. Reviewed end-to-end identity and writeback. Sparse mapping is needed for Latin-1, and replacements can include copied non-ASCII strings. Encode original/replacement using the source codec and test non-ASCII replacement bytes. Parent requires a dependent PR on #536 to eliminate its UTF-8-only symbol-read gap.

## Plan stage

1. Acceptance trace: cookie tests alone omit writeback and verify rediscovery. Added public three-command tests and worker non-ASCII replacement assertions.
2. Interface check: shared decoder lives in core for validator and symbol selection; context reuses it to avoid candidate-by-candidate source decoding. Existing IDs frame raw hash/span plus Unicode replacement, so no ID schema change is needed.
3. Reviewed resource and scope: no codec library or Python runtime in production, sparse mappings instead of full offset tables, dedicated no-debug Cargo target. Unsupported/unknown codec distinction remains a combined diagnostic because no complete codec registry is present.

## Implementation stage

1. Core boundary review: initial candidate fixture had a one-byte hand-count error; corrected the fixture before the true red run, which then failed on absent Latin-1 validation. Six codec/validator tests now cover every raw and decoded boundary, rejected interior UTF-8 offsets, nonrepresentable replacements and wrong decoded-span/column inputs. All 18 existing candidate-policy tests retain error precedence and UTF-8 IDs.
2. Consumer review: the public Latin-1 test failed on UTF-8 decode before implementation, then passed plan/run/verify with three identical IDs, raw hashes/spans and four exact worker byte sequences per execution. Found a diagnostic fixture incorrectly assuming pre-baseline analysis; normal run actually performs baseline first. Changed its command to an independent successful baseline and asserted structured run diagnostics, preserving lifecycle rather than adding an all-file pre-baseline scan.
3. Writeback/error review: replacement can include accented text copied from the AST source, so the collection replacement fixture checks its actual Latin-1 bytes and Python value. Common validation already checks encoded original bytes; removed redundant worker rechecking and reused the context. Clippy found public `expect` panic documentation and test casts; replaced them with checked error/cast handling, kept explicit expected line numbers and retained gate evidence. Dependency integration is checked after merging #476.

Initial focused results: core candidate policy 18 passed, new encoding 6 passed; analyzer handler 40 passed; plan 55 passed and one existing ignored; new public encoding suite 3 passed. These are pre-integration results; combined gates follow separately.
