---
type: Audit
title: nullable型変異の適用条件の追加監査
description: 型名の再束縛と複数型引数内の対象外要素を見落とす問題、Leanの木モデルと公開CLIの照合。
status: draft
catalog_revision: 5e631ef
sources:
  - id: issue-564-review
    resource: ../../superpowers/reports/2026-09-24-issue-564-review.md
    revision: c1ce10402df3a26ce0c9e3eb1e7d524fcf4c2aca
    working_tree: modified
    sha256: 02295dbe32519ee3671cceb9226770ddbf44167cbaa1eda76b338715396bb9d9
  - id: issue-564-tests
    resource: ../../../crates/hoimin-cli/tests/nullable_builtin_provenance.rs
    revision: c1ce10402df3a26ce0c9e3eb1e7d524fcf4c2aca
    working_tree: untracked
    sha256: bad1dd2514554710dd6b388caa74e6833e63bfd77119e6c0104bc2d7b25cd3a4
  - id: report
    resource: ../../audits/2026-09-15-nullable-gates/README.md
    working_tree: untracked
    sha256: 13a3b384def390e0596119cc5901ef1b12ac7cd32bbd4ae73266000a6e207b52
  - id: model
    resource: ../../audits/2026-09-15-nullable-gates/GateModel.lean
    working_tree: untracked
    sha256: e5b61c5243c6e0fe2e1d9bc3a6c344161fe5495fe5d5a301c22fc546ba6ae2f5
---

# 2026-09-15に確認した問題

nullable追加が型名の再束縛を見落とす問題を[#564](https://github.com/tokyogas-tech/hoimin/issues/564)に起票した。同名の利用者定義クラスや値に対しても、組込み型と同様に `| None` を追加してしまう。[^report]

複数型引数の内部で対象外構文を見落とす問題を[#565](https://github.com/tokyogas-tech/hoimin/issues/565)に起票した。`dict[str,Any]` のsliceはExpr::Tupleとなるが、helperがその子へ再帰しない。単一引数のlist[Any]は正しく拒否される。[^report]

# 証拠と限界

Leanは名前のtrusted条件と型引数の木を分離し、候補許可にはtrustedが必要なこと、任意の深さの子孫に対象外要素があれば拒否することをモデル内で証明した。深さ0〜2の木を列挙し、名前条件・tuple内部検査を省く壊した規則を検出した。[^model][^report]

Lean生成15入力はdebug/releaseとも4 match / 11 mismatch / 実行基盤エラー0だった。名前再束縛4件では変異後の注釈評価が例外となり、型引数内部7件では設計上の対象外候補を生成した。既存テスト25件は成功した。15fixtureはstrict、全木と一般定理はmodel-onlyである。型チェッカーのbaseline・スコアやRust全体の証明は含まない。[^report]

# 再確認の契機

nullable_add_allowed、is_supported_annotation、contains_disallowed_annotation、名前解決の追跡対象を変更するときに再実行する。通常のint/list/dictと、単一型引数の除外を保持する。#565の実装修正と正式CIへのケース移行は、この文書では確認していない。

[^report]: [監査報告と再現手順](../../audits/2026-09-15-nullable-gates/README.md)。
[^model]: [GateModel.lean](../../audits/2026-09-15-nullable-gates/GateModel.lean)。

# Issue #564の修正と2026-09-24の検証

nullable追加の条件へ既存の注釈scopeによる組込み型判定を追加し、5つのscalar名を追跡対象に加えた。8種類の組込み名、module/class/function/type-parameterの境界、入れ子の型引数、標準ライブラリのaliasを単体テストで確認する。公開planでは元の注釈をCPython3.14で評価し、再束縛した型名を除外しながら、通常の組込み型の候補とspanを保持することを照合する。[^issue-564-review][^issue-564-tests]

単体テスト2件と公開planテスト1件は修正前に余分な候補を検出し、修正後に成功した。元の注釈評価が成功することと、独自メタクラスに対するunion演算が例外になることも別に確認した。これは名前解決の回帰検証であり、上記15入力全体の再照合や#565の対象外構文検査の修正を意味しない。[^issue-564-review][^issue-564-tests]

[^issue-564-review]: [Issue 564 review log](../../superpowers/reports/2026-09-24-issue-564-review.md)。
[^issue-564-tests]: [nullable追加の公開planテスト](../../../crates/hoimin-cli/tests/nullable_builtin_provenance.rs)。
