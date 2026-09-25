# Issue 625: avoid full-body progress dispatch scans

## Goal and contract

Reduce format/schema identification work before typed report parsing. Preserve JSON documents, JSONL, legacy v2/current v3, arbitrary top-level field order, opaque normalized_config, structural validation and rejection of malformed input. Do not change comparison policy, output, schema, or retained-report memory. Measure the public read_report path on the same approximately 12-MiB real run report before and after.

## Design

Use one speculative top-level prefix probe to identify both format and schema. The visitor tracks schema_version, a string-valued kind for event input, and document keys run/baseline/mutants/summary. Stop as soon as a version and format are known; in the normal compact document order (schema_version followed by run) no candidate body is visited. Early exit uses an explicitly captured result and a serde visitor stop error; it is only dispatch evidence, never validation. Always parse and validate the complete selected document/event afterward, including duplicate fields and trailing input.

If the first physical nonblank line does not provide sufficient evidence, retain the current document read-to-end fallback and probe the assembled bytes. Valid JSONL headers are complete on one line. Late discriminator fields remain supported, although a late schema may require skipping preceding values once. Remove the existing complete Kind and ReportHeader identification parses. Keep the legacy and current typed validators unchanged.

Alternatives: assuming v3 JSON would break supported inputs; constructing a complete serde Value adds memory/decoding overhead; a hand-written JSON scanner risks escaped-key/string and nesting bugs. A small serde visitor shares the proven JSON token parser and retains strict full validation after the speculative dispatch.

## Resource and performance validation

Add a deterministic probe read-count gate with a large candidate body: the prefix-discriminated case must consume fewer than 128 bytes regardless of body length. Exercise JSONL kind/schema in both orders, escaped keys, nested misleading keys and late document schema. A full-map baseline probe must fail this gate before the early-stop implementation.

Public regressions cover v2/v3 documents with every top-level field first/last, compact and pretty encodings, reordered JSONL headers, unknown/duplicate fields and trailing malformed data, and opaque configuration. Existing progress/heap/Lean consumer tests continue to apply.

Build release once before edits, generate 24 survivors with 256-KiB literals using public run, and link an external read_report probe. Reuse the exact report file and identical timing loop (two warmups, 31 samples, three rounds) after the change. Compare serialized mutants byte-for-byte and report medians and limitations, not a guaranteed speedup for ordinary reports.

## Design self-reviews

1. Correctness boundary: a prefix is not a validated report. All early dispatch results must still go through the unchanged typed parser and validators; malformed suffix and duplicate-field tests protect this separation.
2. Field order: normal serialization exposes discriminators early, but arbitrary ordering can put large values first. The visitor must skip unknown/nested values correctly and support late schema, accepting a remaining scan rather than assuming field order.
3. Resources and scope: a serde Value representation or second read would add unrelated allocations/I/O. Probe existing buffered bytes, retain existing JSONL streaming, and measure the public release reader on an unchanged real fixture. No Lean policy change or process is needed.
