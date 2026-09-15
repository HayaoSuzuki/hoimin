---
type: Audit
title: 値なし注釈による候補欠落の追加監査
description: 実行時の再代入と関数ローカル宣言を区別する候補精度改善案とLeanによる確認。
status: draft
catalog_revision: 5e631ef
sources:
  - id: report
    resource: ../../audits/2026-09-15-declaration-only/README.md
    working_tree: untracked
    sha256: 4272c093852eab370489e8345c9ed515f20cdf26f2e5f13a4d495977db8636c9
  - id: model
    resource: ../../audits/2026-09-15-declaration-only/DeclarationModel.lean
    working_tree: untracked
    sha256: 9456f5e7d57b3a9f18d3da4c95a6f8b61f16d893506d320ff10a77a6b36d060f
---

# 候補精度の改善案

`5e631ef`で、値のない型注釈がbuiltin・operator aliasの候補を消すことを確認し、[#562](https://github.com/tokyogas-tech/hoimin/issues/562)へ起票した。module/classの名前への値なし注釈では実行時の参照先は変わらない。関数内の注釈はローカル宣言となるので区別する。既存設計が保守的な候補欠落を許容するため、今回の分類は候補精度の改善である。[^report]

# 証拠と限界

Leanはscope・局所値・外側の参照先・RHS有無を分離し、値なし注釈による値保持、任意回数の反復、関数ローカル未束縛時のfallback禁止をモデル内で証明した。36有限ケースで境界と優先順位の壊した規則を検出した。[^model][^report]

Lean生成14入力を公開planで照合し、debug/releaseとも7 match / 7 mismatch / 実行基盤エラー0となった。mismatchは提案する精度との不一致であり、現行契約の安全性違反を意味しない。全元ソースと提案する10置換はCPythonで正常実行でき、置換後の値が変わった。関連テスト27件は成功した。全状態列挙と一般定理はmodel-only、14fixtureはstrictとして区別する。[^report]

# 再確認の契機

NameResolutionBuilder、operator用ImportScan、AnnAssignの扱いを変更するときに再実行する。属性・subscript targetの評価副作用、関数のローカル宣言、実際の再代入の抑制を保持する必要がある。提案の実装修正と正式CIへのケース移行は未実施である。

[^report]: [監査報告と再現手順](../../audits/2026-09-15-declaration-only/README.md)。
[^model]: [DeclarationModel.lean](../../audits/2026-09-15-declaration-only/DeclarationModel.lean)。
