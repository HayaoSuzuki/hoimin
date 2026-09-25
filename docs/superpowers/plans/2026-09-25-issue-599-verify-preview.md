# Verify selection preview implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Preview a saved plan's actual verification selection without executing tests or creating runtime artifacts.

**Architecture:** Reuse `prepare_verify_selection` and its validation rediscovery. Attach compact preview metadata to `VerifiedPlan`; dispatch a shared writer before entering the shell for dry-run.

**Tech Stack:** Rust, clap, serde_json, Tokio; existing Rust integration tests and JSON Schema.

**Spec:** `docs/superpowers/specs/2026-09-25-issue-599-verify-preview-design.md`

## Global constraints

- Keep normal verify selection, execution, validation and report schema semantics unchanged.
- Preview JSON schema version 1; plan schema 4 and ranking rule 4 are separate fields.
- JSON/JSONL: one object plus newline; human: metadata and ordered rows.
- `--dry-run` conflicts with `--metrics`; valid preview exit 0, invalid input exit 2.
- No shell entry, baseline/test child process, worker copy, session creation or runtime metrics.
- Work only in this issue's worktree, use `CARGO_BUILD_JOBS=2`, commit design/plan before implementation.

## Review focus

1. Explicit IDs whose saved ranks and input order differ from discovery order: use rediscovery IDs; test reversed/duplicated IDs and tampered saved sequence.
2. Borrowed vs owned dispatch: public binary and run_with_io tests exercise both, including writer failure.
3. Truncation with a clipped tail: assert retained count, scope, truncation and requested/selected distinction.
4. Resource settings in a valid plan: preview remains validation-only and never enters worker preflight.
5. Invalid inputs and output destinations: compare preview and normal diagnostics; prove marker/session/TMPDIR/metrics remain unchanged.

### Task 1: Public preview and shared validation output

**Files:** modify `crates/hoimin-cli/src/{cli.rs,lib.rs,plan.rs}`; create `crates/hoimin-cli/src/plan/preview.rs`; modify `crates/hoimin-cli/tests/plan.rs`.

**Interfaces:** consume `prepare_verify_selection(...) -> Result<VerifiedPlan, PlanError>` and existing `VerificationSelection`; produce `VerifyArgs.dry_run: bool`, `VerifiedPlan.preview: VerifyPreview`, and `VerifyPreview::write(&self, format: OutputFormat, writer: &mut impl Write) -> Result<(), String>`. Change internal candidate validation from unit to selected discovery IDs while retaining all validation.

- [ ] Add public CLI tests with this assertion shape (reuse existing Project and plan helpers):

```rust
let output = std::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
    .args(["verify"]).arg(&path).args(["--top", "6", "--dry-run"])
    .output().unwrap();
assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
let preview: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
assert_eq!(preview["kind"], "verify_preview");
assert!(!marker.exists());
```

Use six ranked candidates in a.py/a.py/b.py/b.py/c.py/d.py; diverse positions are [0,2,4,1,3,5]. Compare preview rows against these independently specified positions and normal mutant_started IDs. Check `selection_order == index + 1`, saved rank/path/line, metadata, and unchanged file/temp snapshots before normal execution.

- [ ] Run `CARGO_BUILD_JOBS=2 cargo test -p hoimin-cli --test plan verify_preview -- --nocapture`; expect failure because --dry-run is unknown.
- [ ] Add clap dry_run with metrics conflict, carry it into VerifyArgs and fix plan help. Define serializable preview/row structs. Return `discovery.candidates.iter().filter(|c| requested_ids.contains(&c.id)).map(|c| c.id.clone()).collect()` from the validator after all checks. Resolve ranked IDs using existing selection; explicit IDs use returned discovery order. Populate rows from a manifest ID map.
- [ ] Add preview branches to both dispatches, using the same writer and returning 0 on success or the existing error-to-2 path on failure. Human header exposes the same selector fields as JSON.
- [ ] Run the targeted test command again; expect all preview tests pass. Commit implementation and tests.

### Task 2: Boundary verification, output contract and documentation

**Files:** extend `crates/hoimin-cli/tests/plan.rs`; create `docs/json-schema/verify-preview.schema.json`; modify `README.md`, `docs/knowledge/design/selection-plan-verify.md`, `docs/knowledge/design/index.md`, `docs/knowledge/references/{design-documents,audit-documents}.md`; update review report.

**Interfaces:** consume Task 1 preview JSON contract; no new selector API.

- [ ] Add tests for JSONL's single object, human metadata, failing borrowed writer, explicit duplicates/reversed order with altered saved sequence, empty/truncated plan, offset beyond end, max_mutants, stale source/fingerprint, corrupt schema/rank/descriptor, metrics conflict. Invalid cases run both preview and normal verify and compare diagnostics with no side effects. Run failing regressions before any fixes.
- [ ] Add draft-2020-12 schema with required fields and closed objects for preview and candidate rows. Reuse literal enum contracts from core report selection. Validate actual output using the repository's JSON Schema test facilities if present, otherwise available Python jsonschema.
- [ ] Document preview CLI and independent schema, one-based batch order, explicit discovery order, JSONL single record, truncation and no-runtime limitations. Update OKF source metadata/footnotes and design/report catalogs with the committed spec and report.
- [ ] Conduct implementation reviews for dispatch/side effects, order/validation, API/schema; conduct test reviews for independently computed expectations, negative controls, and boundary coverage. Record concrete findings and repairs in `docs/superpowers/reports/2026-09-25-issue-599-verify-preview-review.md`.
- [ ] Run `cargo fmt --all -- --check`, `CARGO_BUILD_JOBS=2 cargo test --workspace`, and `CARGO_BUILD_JOBS=2 cargo clippy --workspace --all-targets -- -D warnings`. Run OKF YAML/reserved-file and changed-source/footnote/link checks. Expected: all pass; record actual totals and any limits.
- [ ] Commit final tests/docs/report. Send root commit IDs, verification results, review ledger and limitations for independent review and PR creation.

## Pre-implementation review corrections

The repository has no existing JSON Schema validator test helper. Use available `.venv/bin/python` with `jsonschema` to validate captured public CLI output; check availability before choosing that command, and report an unavailable validator rather than claiming conformance. The Rust tests assert required fields and types independently. For temporary artifacts use `.env("TMPDIR", isolated_tmp.path())` on child commands; global environment mutation is prohibited. Construct preview within the existing parsed manifest lifetime, with no second manifest read. Retain existing `PlanError` precedence by collecting the discovery ID vector only after the validation loop succeeds.
