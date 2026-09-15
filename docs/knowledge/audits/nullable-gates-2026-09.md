---
type: Audit
title: nullable型変異の適用条件の追加監査
description: 型名の再束縛と複数型引数内の対象外要素を見落とす問題、Leanの木モデルと公開CLIの照合。
status: draft
catalog_revision: 5e631ef
sources:
  - id: report
    resource: ../../audits/2026-09-15-nullable-gates/README.md
    working_tree: untracked
    sha256: 13a3b384def390e0596119cc5901ef1b12ac7cd32bbd4ae73266000a6e207b52
  - id: model
    resource: ../../audits/2026-09-15-nullable-gates/GateModel.lean
    working_tree: untracked
    sha256: e5b61c5243c6e0fe2e1d9bc3a6c344161fe5495fe5d5a301c22fc546ba6ae2f5
---

# 確認した問題

nullable追加が型名の再束縛を見落とす問題を[#564](https://github.com/tokyogas-tech/hoimin/issues/564)に起票した。同名の利用者定義クラスや値に対しても、組込み型と同様に `| None` を追加してしまう。[^report]

複数型引数の内部で対象外構文を見落とす問題を[#565](https://github.com/tokyogas-tech/hoimin/issues/565)に起票した。`dict[str,Any]` のsliceはExpr::Tupleとなるが、helperがその子へ再帰しない。単一引数のlist[Any]は正しく拒否される。[^report]

# 証拠と限界

Leanは名前のtrusted条件と型引数の木を分離し、候補許可にはtrustedが必要なこと、任意の深さの子孫に対象外要素があれば拒否することをモデル内で証明した。深さ0〜2の木を列挙し、名前条件・tuple内部検査を省く壊した規則を検出した。[^model][^report]

Lean生成15入力はdebug/releaseとも4 match / 11 mismatch / 実行基盤エラー0だった。名前再束縛4件では変異後の注釈評価が例外となり、型引数内部7件では設計上の対象外候補を生成した。既存テスト25件は成功した。15fixtureはstrict、全木と一般定理はmodel-onlyである。型チェッカーのbaseline・スコアやRust全体の証明は含まない。[^report]

# 再確認の契機

nullable_add_allowed、is_supported_annotation、contains_disallowed_annotation、名前解決の追跡対象を変更するときに再実行する。通常のint/list/dictと、単一型引数の除外を保持する。実装修正と正式CIへのケース移行は未実施である。

[^report]: [監査報告と再現手順](../../audits/2026-09-15-nullable-gates/README.md)。
[^model]: [GateModel.lean](../../audits/2026-09-15-nullable-gates/GateModel.lean)。
