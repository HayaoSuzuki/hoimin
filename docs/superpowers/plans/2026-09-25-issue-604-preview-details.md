# Issue 604 implementation plan

**Goal:** expose each selected mutation's column, operator and before/after text in all dry-run preview formats.
**Architecture:** enrich the existing validated-manifest projection; quote mutation text in human output and bump the independent preview schema to 2.
**Spec:** docs/superpowers/specs/2026-09-25-issue-604-preview-details-design.md
**Tech stack:** Rust, serde JSON, existing Tokio CLI integration fixtures.

## Global constraints

No new execution, analysis, selection rules, dependencies, or plan/run schemas. Retain strict/diverse/offset/explicit ordering and side-effect-free dry-run. Use assigned target/batch-verify, one build job, no debug information/incremental builds. Parent coordinates review and publication; user authorization covers execution without another design approval pause.

## Review focus

- Same-line mutations have distinct 1-based columns and operator/text tuples.
- Explicit candidate order follows discovery, not argument or rank order.
- Strict/diverse and offsets retain existing saved-rank selection semantics.
- Multiline, quoted, tabbed and backslash-containing text stays one human row and round-trips in JSON/JSONL.
- Closed schema 2 documents all nine candidate fields; version-1-only clients need an explicit migration.

## Task 1: output regressions and projection

Files: `crates/hoimin-cli/tests/plan.rs`, `crates/hoimin-cli/src/plan/preview.rs`.
Interface: existing `VerifyPreview::new` receives validated `PlanManifest`; `PreviewCandidate` adds `column: u32` and `operator`, `original`, `replacement: String`. Public writer interface and selection APIs do not change.

- [ ] Extend policy/offset and explicit-ID tests to compare added fields against the same selected manifest candidate. Use same-line fixtures where possible and require schema 2.
- [ ] Add a public CLI all-format test with `return value > 0 and value < 10`, asserting columns 17, 21, 31 and matching details/order for strict, diverse/offset and explicit IDs. Retain manifest bytes, marker absence and empty temporary runtime directory checks.
- [ ] Add a public CLI multiline list-to-tuple test with tabs, quotes and backslashes. Assert exact text in machine output, two physical human lines (header plus candidate), and a literal escaped row suffix.
- [ ] Run `cargo test -p hoimin-cli --test plan verify_preview -- --test-threads=1`. Expected: new details/schema/escaped-row assertions fail on current output.
- [ ] Extend the projection:
  ```rust
  column: candidate.column,
  operator: candidate.operator.clone(),
  original: candidate.original.clone(),
  replacement: candidate.replacement.clone(),
  ```
  Set `schema_version: 2`; use `{}:{}:{} operator={} {:?} -> {:?}` in human rows with path, line, column, operator, original and replacement.
- [ ] Repeat targeted command. Expected: all preview tests pass with no execution marker or runtime directories from dry-run.

## Task 2: contract documentation and final gates

Files: `docs/json-schema/verify-preview.schema.json`, `README.md`, this plan, and `docs/superpowers/reviews/2026-09-25-issue-604-preview-details.md`.
Interface: schema `schema_version.const = 2`; candidate required/property lists match actual public output. `column` is integer minimum 1, `operator` string minimum length 1, both mutation texts strings with empty values allowed.

- [ ] Update schema and README to explain new fields, 1-based column, quoted human strings, and the version-1 migration requirement. Compare the public candidate object's keys with the schema's required and property keys in the integration test.
- [ ] Perform three separate implementation and test self-reviews and record concrete findings/fixes; design/plan reviews are below/in the spec.
- [ ] Run workspace tests, workspace/vendor formatting checks, workspace all-target/all-feature clippy and parser clippy. Expected: pass; report any environment failures by name. No Python logic changes or Lean commands are planned.
- [ ] Commit verified code/tests/schema/docs/review evidence. Parent handles independent review and publication.

## Plan self-reviews

1. Requirement coverage: a schema-only test would miss real selection/output behavior. Add public subprocess cases and retain existing rank/discovery/offset comparison tests.
2. Escaping oracle: deriving the expected escaped string using the production formatter would hide the bug. Require literal escaped output and actual physical line counts, alongside parsed JSON text equality.
3. Compatibility and resource review: the old version assertion must change alongside the schema; empty replacement strings must remain legal. No new dependency is needed for key/required-list comparison. Full workspace checks use the existing assigned cache and flags. Task interfaces match the design; no unresolved placeholder remains.
