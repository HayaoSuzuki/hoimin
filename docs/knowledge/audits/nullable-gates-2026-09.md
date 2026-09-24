---
type: Audit
title: nullable型変異の適用条件の追加監査
description: 型名の再束縛と複数型引数内の対象外要素を見落とす問題、Leanの木モデルと公開CLIの照合。
status: draft
catalog_revision: 5e631ef
sources:
  - id: issue-564-review
    resource: ../../superpowers/reports/2026-09-24-issue-564-review.md
    revision: f6f5d96c099fb880884b2b7cb29717ff33d70d75
    working_tree: modified
    sha256: a7d9fc90f5b4b7fb15420f969cf99b85120f20cd97b7a6562811960d4c435a92
  - id: issue-564-tests
    resource: ../../../crates/hoimin-cli/tests/nullable_builtin_provenance.rs
    revision: f6f5d96c099fb880884b2b7cb29717ff33d70d75
    working_tree: clean
    sha256: bad1dd2514554710dd6b388caa74e6833e63bfd77119e6c0104bc2d7b25cd3a4

  - id: issue-565-design
    resource: ../../superpowers/specs/2026-09-24-issue-565-annotation-descendants.md
    revision: 75c1ddfd59a1c2ebf1bc9de05efb33c571c7c328
    working_tree: clean
    sha256: b13589887e2a7fc2f3386c0d284825582139503f3deab0504e79c13d7cb76f8f
  - id: issue-565-model
    resource: ../../../formal/HoiminOracle/HoiminOracle/NullableGateModel.lean
    revision: 75c1ddfd59a1c2ebf1bc9de05efb33c571c7c328
    working_tree: clean
    sha256: d55bbd0f28c1e25e9fa55afb4f53d52938804f88336b8531e947d8db3c57503f
  - id: issue-565-implementation
    resource: ../../../crates/hoimin-cli/src/analyzer/rust.rs
    revision: 75c1ddfd59a1c2ebf1bc9de05efb33c571c7c328
    working_tree: clean
    sha256: e979b73edd09d07dc625ffe282a8ff709e30dcdb9f86ca76134bbd8d703db69b
  - id: issue-565-tests
    resource: ../../../crates/hoimin-cli/tests/lean_nullable_gate_oracle.rs
    revision: 75c1ddfd59a1c2ebf1bc9de05efb33c571c7c328
    working_tree: clean
    sha256: ac025b363720f97b814b0e578f886f7ae182a3093e0e7298f5da95370f8060f3
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

# Issue #565の修正と正式テストへの追加（2026-09-24）

共通の対象外判定がTuple・Listの全要素とStarredの値へ再帰するよう変更した。key/valueの両側、深い型引数、別名import、3番目の要素、unpackを含む負例と正例を追加した。単純なtuple全体を拒否せず、対象内の引数だけなら従来の候補を保持する。[^issue-565-design][^issue-565-implementation]

既存の任意深さの子孫拒否定理を維持し、key側・3番目の子孫の拒否と、左の子だけを検査する規則の反例を追加した。有限検査は従来どおり深さ0〜2で、2・8・74個の木を扱う。これはモデル内の構造的性質であり、Rust実装やPythonの型システム全体の証明ではない。[^issue-565-model]

Leanから生成する65入力すべてを公開CLIでstrictに照合する。構造検査と七つの型演算子に関する61入力に、#564の名前再束縛4入力を含める。CPython 3.14では元注釈の評価と生成候補のコンパイルを確認する。#564を併合する前はその4入力をreport-onlyで観測していたが、併合後は期待値を変更せずstrictへ移行し、report-only指定を拒否する。型チェッカーのscore、任意の実行可能な注釈式、Windows/Linuxの実行比較は今回の確認範囲に含めない。[^issue-565-tests][^issue-565-design]

[^issue-565-design]: [Issue 565: recursively exclude disallowed annotation arguments](../../superpowers/specs/2026-09-24-issue-565-annotation-descendants.md)。
[^issue-565-model]: [正式検証用のNullableGateModel](../../../formal/HoiminOracle/HoiminOracle/NullableGateModel.lean)。
[^issue-565-implementation]: [注釈判定の実装](../../../crates/hoimin-cli/src/analyzer/rust.rs)。
[^issue-565-tests]: [公開CLI対応テスト](../../../crates/hoimin-cli/tests/lean_nullable_gate_oracle.rs)。

# 再確認の契機

nullable_add_allowed、is_supported_annotation、contains_disallowed_annotation、名前解決の追跡対象を変更するときに再実行する。通常のint/list/dictと、単一型引数の除外を保持する。監査時点では実装修正と正式CIへのケース移行は未実施だった。

[^report]: [監査報告と再現手順](../../audits/2026-09-15-nullable-gates/README.md)。
[^model]: [GateModel.lean](../../audits/2026-09-15-nullable-gates/GateModel.lean)。

# Issue #564の修正と2026-09-24の検証

nullable追加の条件へ既存の注釈scopeによる組込み型判定を追加し、5つのscalar名を追跡対象に加えた。8種類の組込み名、module/class/function/type-parameterの境界、入れ子の型引数、標準ライブラリのaliasを単体テストで確認する。公開planでは元の注釈をCPython3.14で評価し、再束縛した型名を除外しながら、通常の組込み型の候補とspanを保持することを照合する。[^issue-564-review][^issue-564-tests]

単体テスト2件と公開planテスト1件は修正前に余分な候補を検出し、修正後に成功した。元の注釈評価が成功することと、独自メタクラスに対するunion演算が例外になることも別に確認した。これは名前解決の回帰検証であり、上記15入力全体の再照合や#565の対象外構文検査の修正を意味しない。[^issue-564-review][^issue-564-tests]

[^issue-564-review]: [Issue 564 review log](../../superpowers/reports/2026-09-24-issue-564-review.md)。
[^issue-564-tests]: [nullable追加の公開planテスト](../../../crates/hoimin-cli/tests/nullable_builtin_provenance.rs)。
