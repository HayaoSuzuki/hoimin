---
type: Contract
title: survivorの評価と実行結果の分離
description: 候補の同定、評価履歴、stale判定、実行結果へ影響させない境界の実装契約とモデル証明の範囲。
status: draft
catalog_revision: d46f13a43667323b8d0374d6c5409942f4c3c822
sources:
- id: issue-875-indexes
  resource: ../../../crates/hoimin-cli/src/review/mod.rs
  revision: 49a34a63ac4e995bfefb156f85ff3a934b594e6b
  working_tree: modified
  sha256: a9dbab96ba87528d7c89c684ca6f1b3f1d75d093384d7cfa55d3eee614ddc5f0
- id: issue-875-input
  resource: ../../../crates/hoimin-cli/src/review/input.rs
  revision: 49a34a63ac4e995bfefb156f85ff3a934b594e6b
  working_tree: modified
  sha256: 74bbf62383d4faeba56df171681eb1fb2fdd2622480012a8bc5d3b2847a76336
- id: issue-875-record
  resource: ../../superpowers/plans/2026-10-14-issue-875.md
  working_tree: untracked
  sha256: 61a3e70da87b03be3b37156873e811196cf6e3894448e71af775463c81cb870e
- id: issue-711-survivor-review-design
  resource: ../../superpowers/specs/2026-10-11-issue-711-survivor-review-design.md
  revision: fdf80af
  working_tree: clean
  sha256: de751d275bf32380e52515eee21a1ae364c35c39f0ee984828a7b87d3f67886d
- id: issue-711-review-report
  resource: ../../superpowers/reports/2026-10-11-issue-711-survivor-review.md
  working_tree: untracked
  sha256: 2f388bf417df4054d41765ec4b66d74815880752cfb377956949bc628871c699
---

# survivorの評価と実行結果の分離

## 対象と契約

`review list/show/record` は保存planの候補に人の評価と理由を紐づけ、独立SQLite sidecarへ履歴を保存する。
project、候補identity、originalを含む完全descriptor、ソースの鮮度が一致する場合だけ評価を適用する。
評価はactionable、not_useful、suspected_equivalent、needs_investigationで、未評価をnot_usefulと混同しない。[^issue-711-survivor-review-design]

## 更新と表示

更新にはexpected revisionを必要とし、履歴追加とrevision更新を同一transactionにする。
source変更時はstaleとして表示し、新しい候補へ評価を移さない。
plan単独では実行結果を推測せず、明示的に付けた検証済みreportからだけstatusを表示する。[^issue-711-survivor-review-design]

## 判断と限界

既存rusqliteを使う独立DBにすることで、実行sessionやスコア計算への依存を避ける。
評価はランキング、kill率、progress判断を変更しない。
Leanでは誤帰属の防止条件、更新拒否時の状態不変、更新成功時の履歴保存を証明した。
168ケースのLean生成corpusでRust純粋処理と比較した。
Rust/SQLite/OSまで形式検証した保証ではなく、ローカルWindows Lean 4.34.1とCI pinned 4.32.2の確認を分ける。[^issue-711-review-report]

## Candidate ID joins

The review command builds borrowed HashMap indexes once for checked plan
candidates, loaded report mutants, and the loaded review snapshot. The same plan
index serves report validation and stale membership; the same report index
serves current and stale execution lookup. The original selected-plan and snapshot
vectors determine display order. Indexes store references to existing IDs and
records, without cloning candidate data or history.[^issue-875-indexes]

Checked plan and report readers reject duplicate IDs before indexing. Store
snapshots validate their key/ID pairs and use unique SQLite primary keys. These
invariants permit direct iterator collection; invalid internal duplicate vectors
have no separate compatibility policy. A stable ID lookup still requires the
existing full-descriptor comparisons. Report decoding, project comparison,
identity validation, descriptor validation, source freshness, and record append
retain their existing error order.[^issue-875-input][^issue-875-record]

Index construction uses expected O(P + R + S) hash-map work and O(P + R + S)
additional key/value references and bucket storage for P plan entries, R report
entries and S snapshot entries. Lookup counts describe logical ID joins, not
bucket probes, elapsed time, total command memory, or source-validation work.
Per-candidate source checks and checked-reader costs remain outside this claim.
The implementation record distinguishes real-command work-count tests and review
regressions from the existing Lean applicability/update model, which does not
model hash-map complexity or Rust error ordering.[^issue-875-record]

## 再確認の条件

実装、保存schema、project同定、report読取り、ランキングへの接続を変更する際に設計との対応を再確認する。
実装計画・実装・テストの各段階でも最低5回のセルフレビューを別途行う。

[^issue-711-survivor-review-design]: [#711 survivorの評価と履歴の保存設計](../../superpowers/specs/2026-10-11-issue-711-survivor-review-design.md)。2026-10-11の実装前設計。

[^issue-711-review-report]: [#711 survivor評価の実装と検証](../../superpowers/reports/2026-10-11-issue-711-survivor-review.md)。実装、試験、証明の条件と限界。

[^issue-875-indexes]: [Review command indexes](../../../crates/hoimin-cli/src/review/mod.rs).
[^issue-875-record]: [Issue #875 design, implementation and validation](../../superpowers/plans/2026-10-14-issue-875.md).

[^issue-875-input]: [Checked plan and report matching](../../../crates/hoimin-cli/src/review/input.rs).
