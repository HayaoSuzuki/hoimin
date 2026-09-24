---
type: Audit
title: 評価順序と未選択methodの確保の追加監査
description: 5e631efのbuiltin解決における文字位置と評価順序の違い、およびmethod replacementの不要な確保。
status: draft
catalog_revision: 5e631ef
sources:
  - id: issue-560-review
    resource: ../../superpowers/reports/2026-09-24-issue-560-review.md
    revision: ffb65c051014f3d9601df2deb0bfeb5ff55c7c38
    working_tree: modified
    sha256: a96d2f42bd974491c68dfa0bf9bee040b6c43725ad70ec515536ebc412dbb146
  - id: issue-560-tests
    resource: ../../../crates/hoimin-cli/tests/builtin_evaluation_order.rs
    revision: ffb65c051014f3d9601df2deb0bfeb5ff55c7c38
    working_tree: untracked
    sha256: 9775e388b7c951d7bcacea8a228e24af94d0864e240310f6fa5c377f6bc5b5b1
  - id: report
    resource: ../../audits/2026-09-15-evaluation-order/README.md
    working_tree: untracked
  - id: model
    resource: ../../audits/2026-09-15-evaluation-order/OrderModel.lean
    working_tree: untracked
---

# 2026-09-15に確認した問題

多重代入の後続targetやstarred引数を評価するとき、既に再代入された名前を組込みと誤認する問題を[#560](https://github.com/tokyogas-tech/hoimin/issues/560)に起票した。名前解決がソースoffsetを評価時点として扱うことが原因である。sourceとdestination両方に影響する。[^report]

未選択のappend変異でも呼出し全体を複製する改善を[#561](https://github.com/tokyogas-tech/hoimin/issues/561)に起票した。深さ32では500KB以上の確保要求の累積量が約97MB、同じ長さの対照は約1MBだった。debug計測であり、peak RSSや速度改善率を示す値ではない。[^report]

# 証拠と限界

Leanは束縛とlookupの順序をモデル化し、再代入後の任意長の後続列で候補を拒否することと、先行lookupを保持することを証明した。組込みへ戻す操作はモデルに含まない。壊した遅延書込み規則は深さ2で検出した。[^model][^report]

Lean生成7入力のstrict照合はdebug/releaseとも2 match / 5 mismatch、実行基盤エラー0だった。元ソース・生成候補は全件CPythonで評価し、公開runのkilled計上も確認した。関連テスト27件は成功した。全イベント列と定理はmodel-onlyであり、Rust全体の証明ではない。確保量は独立した実測で、releaseの確保量は未検証である。[^report]

# 再確認の契機

NameResolutionBuilderの束縛記録やresolve_ordered_at、method replacement helperを変更するときに正例・負例と計測を再実行する。性能修正では、未選択の親callの子にある選択済み演算子の探索を維持する。未選択methodの確保に関する修正後の正式CIへの対応付けは、この文書では確認していない。

[^report]: [追加監査と再現手順](../../audits/2026-09-15-evaluation-order/README.md)。
[^model]: [OrderModel.lean](../../audits/2026-09-15-evaluation-order/OrderModel.lean)。

# Issue #560の修正と2026-09-24の検証

名前の出現位置と評価イベント番号を分離し、右辺の評価、targetへの逐次格納、位置引数・starred引数からkeyword引数への評価順序で組込み名を照会する実装へ変更した。先行する右辺の参照と拡張代入のtarget内の参照は保持する。設計・計画・実装・テストをそれぞれ3回自己レビューし、削除対象にも逐次評価が必要であることを追加確認した。[^issue-560-review]

公開plan/runの2テストは修正前の実装で失敗し、修正後に成功した。公開planの入力には、先行targetの格納後に入れ子の展開が失敗する例と、最初の格納前に失敗する例を含む。後者は到達経路を精密に判定せず、候補を保守的に抑制する。これらはRust・CPythonの回帰検証であり、上記のLeanモデルを使った全実装の証明や、#561の確保量の再計測ではない。[^issue-560-tests][^issue-560-review]

[^issue-560-review]: [Issue 560 review log](../../superpowers/reports/2026-09-24-issue-560-review.md)。
[^issue-560-tests]: [公開plan/runの評価順序テスト](../../../crates/hoimin-cli/tests/builtin_evaluation_order.rs)。
