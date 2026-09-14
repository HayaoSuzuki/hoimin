# Issue #456 Validation Context Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development for implementation and reviewed execution. User authorizes autonomous execution through PR creation.

**Goal:** Share source preprocessing once per requested file during verify.
**Architecture:** Group candidate references by path, lazily validate one file, retain per-ID results and consume them in existing order. Release source bytes before reading the next file.
**Tech Stack:** Rust 1.98.0, Tokio, existing hoimin-core validation APIs.
**Spec:** ../specs/2026-09-14-issue-456-validation-context-design.md

## Global Constraints

- Preserve all descriptor validations and first-error ordering.
- Keep schema/ranking versions, candidate IDs, rediscovery and timeout semantics unchanged.
- No source byte cloning or new runtime dependencies.
- Each requested step receives at least three recorded self-reviews.

## Task 1: File-scoped descriptor validation

Files: modify `crates/hoimin-cli/src/plan.rs`; add focused tests under `crates/hoimin-cli/src/plan/validation_tests.rs` if needed.

- [x] Add regression tests for the real verify descriptor path. Observe actual context builds and source bytes under cfg(test), using per-call counters rather than global shared mutable counters. Assert one build per file for candidate counts 1/2/4 and the same stable IDs. The repeated-context implementation must fail the cost assertion.
- [x] Run `cargo test -p hoimin-cli --lib plan --offline`; record the expected failing assertion before changing runtime behavior.
- [x] Group references with `BTreeMap<Utf8PathBuf, Vec<&MutationCandidate>>`; use `CandidateValidationContext::new(&source)` once per first requested path; call `validate_candidate_with_context(&context, &candidate_descriptor(candidate))` for each member. Store the existing `PlanError` result per ID and remove it when the original loop reaches that ID. Unknown IDs and unselected targets retain their original checks.
- [x] Add diagnostic parity cases for modified hash/span/original/line/column/ID, invalid UTF-8, multiple files, and interleaved requested ID ordering. Assert literal error variants/messages where meaningful, alongside legacy API comparison.
- [x] Run focused tests and review acceptance coverage, diagnostic/memory boundaries, and the revised full diff separately. Record all three passes.

## Task 2: Evidence, OKF and PR

- [x] Run release comparison for 1/100/500 candidates with fixed source size and 1/2/4 files; assert identical stable IDs, save commands, repeats and environment. Distinguish isolated validation measurements from CLI timings.
- [x] Run applicable development quality gates. Record failures and unverified platforms explicitly.
- [x] Update `docs/knowledge/design/selection-plan-verify.md`, design and report indexes, source metadata and hashes; parse frontmatter with PyYAML and check local links/source footnotes. Review format, factual correspondence and Japanese prose separately.
- [ ] Commit only this Issue's files; review PR scope/closing reference, validation evidence, then final staged diff and branch status. Push the issue branch and create a PR against main using the repository template.

## Planセルフレビュー

1. 設計と対応付け、各受け入れ条件の実装・テスト・性能測定をTask 1/2へ割り当てた。
2. 借用contextをbytesと同じmapへ入れる自己参照構造を避けた。cfg(test)観測は並行テスト間で共有せず、一呼び出しの値に限定する。
3. コマンド・変更ファイル・診断優先順・未検証条件を再点検した。既存の単件APIとの比較だけに依存しない独立したエラー期待値も指定した。
