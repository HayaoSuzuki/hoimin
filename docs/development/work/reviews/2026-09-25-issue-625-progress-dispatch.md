# Issue 625 review and verification record

Design and plan were committed as `f1b57b3` before implementation. Each artifact records three self-reviews. Branch base is `d4aa1e1`; scope is report format/schema probing only.

## Implementation self-reviews

1. **Speculative evidence versus validation.** Traced every early-stop result into read_report. Event evidence still invokes the complete first-event decoder, lifecycle reader and document validator; document evidence still invokes the complete v2/v3 typed decoder and existing validator. The stop error text is never matched or interpreted; only captured header evidence is used. Duplicate fields, unknown fields, trailing tokens and incomplete values after the probe remain the full parser's responsibility.
2. **Ordering, escaping and schema eras.** Reviewed each top-level discriminator and nested-value skip. serde decodes escaped property names and IgnoredAny consumes complete nested values. A document key before schema skips its value once; a schema before the document key stops before its value. Reordered valid event headers still expose string kind plus u32 schema. Pretty input falls back to assembled bytes; unsupported versions and legacy JSONL continue through existing rejection paths. No configuration type or schema validation was weakened.
3. **Allocation, I/O and dependency behavior.** Probe consumes existing first-line/document buffers and owns only small keys/header evidence. No file is reread, no complete serde Value tree is added, no new dependency or unsafe code is introduced, and JSONL event-history retention is unchanged. serde_json completes its map error path after deliberate stop but does not scan the remaining body; the counted-reader gate verifies this behavior. Removed the now-unused Deserialize import after removing is_event. No new Lean semantics or process was needed.

## Test self-reviews

1. **Deterministic performance RED.** Wrote the counted-reader tests before early stopping and exercised a full-map baseline probe reproducing the prior serde identification traversal. The 256-KiB body required 262,190 bytes and failed the less-than-128 bound. The escaped-key incomplete-body case also failed until probing stopped before its value. This gate measures actual parser consumption, not source text or wall-clock thresholds.
2. **Compatibility and adversarial suffixes.** Added all top-level document rotations in compact/pretty form for original-v2/current-v2/current-v3, comparing complete returned mutant values. Opaque configuration contains misleading nested kind/schema fields. Rotated JSONL headers preserve dispatch. Typed public reads must reject duplicate schema/run/kind, unknown fields, truncated documents and extra JSON after a valid prefix. Independent review suggested extending suffix coverage to JSONL; added duplicate schema/kind/run_id, unknown fields, truncation and trailing JSON to its first event as well.
3. **Public measurement and resource controls.** Preserved a release-linked baseline public read_report executable before optimization and one unedited 24-survivor public run artifact. Both executables time reading/parsing/validation before destroying returned candidates, with two warmups and 31 samples per round. Serialized mutant BLAKE3 digests are compared per encoding across all runs; reordered integration fixtures additionally compare complete returned values. Existing history, JSONL-history and comparison-body heap gates retain their original bounds.

## Independent review

The analyzer agent reviewed the complete diff, including dispatch.rs, read-only against d4aa1e1. No blockers found: capture/stop handling, final validation, v2/v3 and JSONL routing, nested/escaped keys and deterministic consumption gate were checked. Its optional JSONL malformed-suffix coverage suggestion was implemented. No reviewer build or code modification was needed.

## Verification

- RED: 1 probe unit passed and 2 failed; primary allocation-independent scan evidence was 262,190 bytes consumed for a 262,144-byte body.
- Initial GREEN: 3 probe units; progress 78; Lean progress consumers 7; existing progress heap gates 3.
- Final probe gate: 30 bytes consumed for both 32-byte and 262,144-byte bodies; all 3 probe units passed.
- Final public dispatch regressions, including reviewer-suggested JSONL suffix cases: 3 passed.
- `cargo test --offline --locked --workspace`: exit 0; aggregated 2301 passed / 22 ignored across 98 result groups, including subprocess test groups.
- Exact CI `cargo clippy --workspace --all-targets --all-features -- -D warnings`: exit 0 after the final test changes.
- Exact CI `cargo clippy --locked -p littrs-ruff-python-parser --lib --no-deps -- -D warnings`: exit 0.
- Workspace/vendor parser fmt checks and `git diff --check`: exit 0.
- Logs: `/tmp/hoimin-batch-604-632/625-{red,unit-final,dispatch-final,focused,workspace,clippy-final,parser-clippy}.log`. No Python source changed and no Lean process ran.

## Public release reader measurement

On macOS 15.7.7 arm64 with Rust 1.98.1, built the same release profile (thin LTO) before and after. The fixture was generated once by public `hoimin run` over 24 list literals containing 262,144-character strings, with `/usr/bin/true`, `collection_list_tuple`, 24 mutants and explicit best-effort memory. It produced 24 survivors, a complete report and expected run exit 1. No generated report was edited. The JSON file is 12,614,019 bytes; SHA-256 is `90c2c1dee2288d977cfe14aa999acdcbb7308db7e1bd2ef31b341b1320b8111e`.

The standalone probes call the public linked `read_report`; no copy of production parsing code is used. Each invocation performs two warmups and 31 measured reads. Three rounds randomize old/new and JSON/JSONL ordering, giving 93 samples per condition. Timers include file read, parsing and validation and stop before destruction/serialization of returned mutants.

| Input | Before median | After median |
| --- | ---: | ---: |
| Compact JSON | 9.354625 ms | 7.597375 ms |
| JSONL control | 5.477708 ms | 5.157084 ms |

For this compact JSON fixture, measured median time fell 18.78%. Per-round JSON medians were 9.205/9.294/10.059 ms before and 7.886/7.838/7.224 ms after. This is a local warm-file measurement during other development activity, not a guaranteed CLI speedup or ordinary-project result. The JSONL control remains streaming; its timing variation is not attributed entirely to dispatch. No RSS or memory reduction is claimed.

Serialized-mutant BLAKE3 digest matched across every JSON read and both executables: `1902b1e9ef605025d1896d977c8e884fa19424698225b3d134a2d3062b9972c3`. JSONL had its own consistent digest (its independently generated run metadata differs). Reproducible probes, generation script, unchanged report files and all raw samples are in `/tmp/hoimin-batch-604-632/625-benchmark/` in the implementation environment. Durable deterministic traversal and semantic regressions are committed with the implementation.

## Integration with merged issue 610

Merged origin/main bf09c91 into the published branch without rewriting commits. The only conflict was the import/module block in `input.rs`: retained both the dispatch module and duplicate fingerprint reader imports. Git placed dispatch into the shared `read_buffered_report` automatically, so ordinary public reads and fingerprinted CLI reads both use the same typed parser; raw input hashing still wraps all file bytes and no report reread was introduced.

After integration: progress input/duplicate unit tests 8, public progress 81, existing Lean consumers 7, and three progress heap tests passed. Both exact CI clippy commands passed; workspace and vendor parser fmt and diff checks passed. An initial vendor fmt command used an incorrect directory name; rerunning the exact CI path `vendor/ruff_python_parser/Cargo.toml` passed. No Lean process was launched.
