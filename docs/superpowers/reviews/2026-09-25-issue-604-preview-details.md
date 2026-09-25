# Issue 604 review and verification record

Design and plan were committed as `b56a217` before test/product changes, followed by the pre-implementation coordinate correction `45a1f2a`. Base is Issue 606 commit `86ab31d`. Their three self-review passes are recorded in the design and plan. User authorization covers implementation and commits; parent handles independent review/publication.

## Implementation self-reviews

1. **Projection and coordinate contract.** Traced selected IDs to the existing validated-manifest index. All four added fields come from that candidate, preserving selection/rank/discovery and avoiding another analysis. The fixture audit exposed an incorrect initial design assumption: `SourceLocationIndex` returns 0-based Python columns. Corrected design before product code, used schema minimum 0, retained exact manifest values, and added a column-zero fixture.
2. **Serialization and human rendering.** Examined JSON/JSONL shared writer and human format. Machine output retains raw strings through serde; human uses quoted debug representations for both sides, so newline/tab/quote/backslash characters do not split rows. Empty strings require no special case or minimum-length restriction. Header and preview/runtime dispatch are unchanged. No baseline/worker/session path was added.
3. **Schema and compatibility.** Compared every candidate property with the closed schema, including required lists and coordinate minima. Version advances to 2 in output and schema; README names the required client upgrade and unchanged plan/run versions. Review found an extra leading space introduced in the README rewrite; removed it and separated the compatibility paragraph. No new dependency or schema negotiation API was added.

## Test self-reviews

1. **Ordering/provenance coverage.** Existing strict/diverse/offset tests now compare all nine candidate fields against the same selected manifest entry and exercise same-line candidates across multiple files. Explicit-ID test continues to distinguish discovery order from rank/argument/saved sequence. The new real CLI fixture independently pins columns 17/21/31 and the three operator/text tuples, then verifies all formats and selector paths.
2. **Escaping and boundary oracle.** Multiline list-to-tuple fixture has a real column-zero candidate with tabs, quotes and backslashes. Its expected human suffix is literal, not produced by the formatter under test. Moved human first for a separate RED proving the old row lacked detail; JSON/JSONL round-trip actual multiline candidate strings. External marker absence, empty runtime temp directory and unchanged manifest bytes check dry-run side effects; existing execution negative controls remain enabled.
3. **Contract checks and maintainability.** Machine candidate keys must equal both schema required and property key sets; version is checked against literal 2 and schema const. These are contract/key checks, not a claim of general JSON Schema validation. Full all-format test exceeded the clippy line threshold; extracted its assertions into a test-only helper without changing coverage. Workspace all-target/all-feature clippy then passed; no allowances were added.

## Verification evidence

- RED: `cargo test -p hoimin-cli --test plan verify_preview -- --test-threads=1`: 4 passed, 4 failed, exit 101. Failures show schema 1 versus expected 2 or absent new fields.
- Escaping RED: `cargo test -p hoimin-cli --test plan verify_preview_details_escape -- --test-threads=1`: 1 failed, exit 101. Real human row omitted column/operator/before-after text, failing the literal escaped suffix.
- GREEN: full preview filter: 8 passed, exit 0, including both new regression tests.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: exit 0 after test-helper extraction.
- Workspace and vendored-parser formatting and `git diff --check`: exit 0.
- Locked vendored-parser clippy: exit 0.
- Existing README workflow checks: `.venv/bin/python -m unittest tests.test_ranked_plan_docs -q`: 2 passed.
- `cargo test --workspace -- --test-threads=1`: exit 0; 2292 passed, 0 failed, 22 ignored across 95 test/doc-test binaries, including the final test-helper refactor.

Resource flags: `CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/target/batch-verify`, one build job, dev/test debug information disabled, incremental compilation disabled. No Lean runs or Python edits were required; only the existing README unittest contract was executed.

## Independent review

The Issue 609 implementer independently reviewed the diff while the full suite ran. Parent relayed no blocking findings: detail fields copied from the same validated candidate, unchanged ID/selection ordering, closed schema 2 with column minimum 0, debug escaping and literal multiline fixtures, and dry-run marker checks were assessed.
