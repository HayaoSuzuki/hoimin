---
type: Audit
title: 評価順序と未選択methodの確保の追加監査
description: 5e631efのbuiltin解決における文字位置と評価順序の違い、およびmethod replacementの不要な確保。
status: draft
catalog_revision: 5e631ef
sources:
  - id: report
    resource: ../../audits/2026-09-15-evaluation-order/README.md
    working_tree: untracked
  - id: model
    resource: ../../audits/2026-09-15-evaluation-order/OrderModel.lean
    working_tree: untracked
---

# 確認した問題

多重代入の後続targetやstarred引数を評価するとき、既に再代入された名前を組込みと誤認する問題を[#560](https://github.com/tokyogas-tech/hoimin/issues/560)に起票した。名前解決がソースoffsetを評価時点として扱うことが原因である。sourceとdestination両方に影響する。[^report]

未選択のappend変異でも呼出し全体を複製する改善を[#561](https://github.com/tokyogas-tech/hoimin/issues/561)に起票した。深さ32では500KB以上の確保要求の累積量が約97MB、同じ長さの対照は約1MBだった。debug計測であり、peak RSSや速度改善率を示す値ではない。[^report]

# 証拠と限界

Leanは束縛とlookupの順序をモデル化し、再代入後の任意長の後続列で候補を拒否することと、先行lookupを保持することを証明した。組込みへ戻す操作はモデルに含まない。壊した遅延書込み規則は深さ2で検出した。[^model][^report]

Lean生成7入力のstrict照合はdebug/releaseとも2 match / 5 mismatch、実行基盤エラー0だった。元ソース・生成候補は全件CPythonで評価し、公開runのkilled計上も確認した。関連テスト27件は成功した。全イベント列と定理はmodel-onlyであり、Rust全体の証明ではない。確保量は独立した実測で、releaseの確保量は未検証である。[^report]

# 再確認の契機

NameResolutionBuilderの束縛記録やresolve_ordered_at、method replacement helperを変更するときに正例・負例と計測を再実行する。性能修正では、未選択の親callの子にある選択済み演算子の探索を維持する。修正後の正式CIへの対応付けは未実施である。

[^report]: [追加監査と再現手順](../../audits/2026-09-15-evaluation-order/README.md)。
[^model]: [OrderModel.lean](../../audits/2026-09-15-evaluation-order/OrderModel.lean)。
