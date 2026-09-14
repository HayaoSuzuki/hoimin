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

## Dependency integration stage

1. Committed initial codec implementation as `98bff45`, then merged `enhancement/issue-476` (merge `98969d7`). Both issue documentation sets and source inventories were preserved by the merge.
2. Added a public Latin-1 `calc:café` definition fixture first. It failed with `plan.target.resolve` at the inherited `String::from_utf8` boundary. Replaced that read with `hoimin_core::decode_python_source`, passing `.text()` to the existing definition collector and retaining path-specific errors.
3. Re-read all production `from_utf8` and mutation `as_bytes()` consumers. Remaining uses are codec implementation, compatibility error-precedence fallback, or stable-ID framing; the symbol reader now shares codec rules. Combined plan tests passed (63 plus one ignored), and all four encoding CLI tests passed, including actual CPython execution and exact original-byte preservation through plan/run/verify. The parent PR dependency remains explicit.

## Verification stage

1. Combined production gates exited 0 on macOS: `cargo test --workspace`, `cargo test -p hoimin-core --features contracts`, `cargo test -p hoimin-cli --features contracts -- --test-threads=2`, workspace/all-target/all-feature Clippy, locked parser Clippy, workspace fmt and parser fmt. The default workspace run contains 78 harness summaries (1,847 passes including child harnesses, 19 ignored); these aggregates are not unique-test counts. CLI contracts used two test threads proactively because the earlier #476 host run had a disk-monitor timing failure; no #480 contracts failure occurred.
2. Added a final independent CPython 3.14.7 comparison table after the full suites: twelve accepted/rejected cookie/BOM cases cover second-line eligibility, blank/comment/code first lines, strings, trailing/third-line comments, an ordinary matching comment, BOM conflicts and ASCII mismatch. The complete five-test CLI encoding suite and all-target/all-feature Clippy passed again. No production code changed for that table. The parent agent separately reviewed offset mapping, validator precedence, writeback and symbol integration and reported no blocker.
3. Rechecked evidence scope: tests execute CPython in isolated worker fixtures and capture raw bytes, but do not claim installed-wheel, Linux/Windows native backend or codec-wide Lean proof coverage. Python package/wheel gates were not rerun for this Rust codec change. Unknown codecs share a deliberate unsupported-or-unknown diagnosis; arbitrary Python codec registry compatibility is not claimed.

## Publication stage reviews

1. Reviewed the PR draft against final diff and dependency: base `enhancement/issue-476`, dependency #536, all #480 codec acceptance points implemented. Normal run's baseline-before-analysis order and the earlier symbol-resolution decoder are described separately.
2. Reviewed source inventory and working tree: include shared core decoder, validator, analyzer conversion, worker encoding, symbol adaptation, six core encoding tests and five CLI encoding tests, README/development contracts and this issue's documents. Exclude the temporary `.venv` symlink and dedicated build output.
3. Validated 19 OKF pages with a YAML parser, ten issue-480 source hashes/citations, 651 local link destinations in touched pages and reachability of all 19 pages. Six clean source entries were additionally compared with `git show` at their recorded commits. Reviewed the Japanese contract paragraphs separately and preserved earlier issue provenance instead of refreshing unrelated historical hashes.
