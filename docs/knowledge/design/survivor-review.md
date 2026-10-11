---
type: Contract
title: survivorの評価と実行結果の分離
description: 候補の同定、評価履歴、stale判定、実行結果へ影響させない境界の実装前設計。
status: draft
catalog_revision: d46f13a43667323b8d0374d6c5409942f4c3c822
sources:
- id: issue-711-survivor-review-design
  resource: ../../superpowers/specs/2026-10-11-issue-711-survivor-review-design.md
  working_tree: untracked
  sha256: de751d275bf32380e52515eee21a1ae364c35c39f0ee984828a7b87d3f67886d
---

# survivorの評価と実行結果の分離

## 対象と契約

#711の実装前設計では、保存planの候補に人の評価と理由を紐づけ、独立SQLite sidecarへ履歴を保存する。
project、候補identity、originalを含む完全descriptor、ソースの鮮度が一致する場合だけ評価を適用する。
評価はactionable、not_useful、suspected_equivalent、needs_investigationで、未評価をnot_usefulと混同しない。[^issue-711-survivor-review-design]

## 更新と表示

更新にはexpected revisionを必要とし、履歴追加とrevision更新を同一transactionにする。
source変更時はstaleとして表示し、新しい候補へ評価を移さない。
plan単独では実行結果を推測せず、明示的に付けた検証済みreportからだけstatusを表示する。[^issue-711-survivor-review-design]

## 判断と限界

既存rusqliteを使う独立DBにすることで、実行sessionやスコア計算への依存を避ける。
評価はランキング、kill率、progress判断を変更しない。
Leanでは誤帰属の防止条件、更新拒否時の状態不変、更新成功時の履歴保存をモデル化する予定である。
現時点では機能も証明も未実装であり、Rust/SQLite/OSまで形式検証した保証ではない。[^issue-711-survivor-review-design]

## 再確認の条件

実装、保存schema、project同定、report読取り、ランキングへの接続を変更する際に設計との対応を再確認する。
実装計画・実装・テストの各段階でも最低5回のセルフレビューを別途行う。

[^issue-711-survivor-review-design]: [#711 survivorの評価と履歴の保存設計](../../superpowers/specs/2026-10-11-issue-711-survivor-review-design.md)。2026-10-11の実装前設計。
