---
type: Audit
title: 値なし注釈による候補欠落の追加監査
description: 実行時の再代入と関数ローカル宣言を区別する候補精度改善案とLeanによる確認。
status: draft
catalog_revision: 5e631ef
sources:
  - id: issue-562-design
    resource: ../../superpowers/specs/2026-09-25-issue-562-declaration-only-design.md
    revision: 3fedcb1bfdbd464600052416c3086662698805f8
    working_tree: clean
    sha256: 993b396d274da71197f29a4bbd07126bb49c0ece5a23df46217d93e6f6389874
  - id: issue-562-tests
    resource: ../../../crates/hoimin-cli/tests/declaration_only_annotations.rs
    revision: 3fedcb1bfdbd464600052416c3086662698805f8
    working_tree: clean
    sha256: 1f5062bd20e30ddc3dec175608c8324ce0dc8672c11a669468d38be483b7041f
  - id: issue-562-review
    resource: ../../superpowers/reviews/2026-09-25-issue-562.md
    revision: 3fedcb1bfdbd464600052416c3086662698805f8
    working_tree: clean
    sha256: 7de08a6682841e1c47db2d82710486b0e847d3384fc8909ddc39138377a71c7a
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

NameResolutionBuilder、operator用ImportScan、AnnAssignの扱いを変更するときに再実行する。属性・subscript targetの評価副作用、関数のローカル宣言、実際の再代入の抑制を保持する必要がある。監査時点では提案の実装修正と正式CIへのケース移行は未実施だった。後続修正は以下に記録する。

[^report]: [監査報告と再現手順](../../audits/2026-09-15-declaration-only/README.md)。
[^model]: [DeclarationModel.lean](../../audits/2026-09-15-declaration-only/DeclarationModel.lean)。

# Issue #562の修正（2026-09-25）

module/class内の名前への値なし注釈では、既知の組込み参照と無条件module importの参照先を保持する。関数内では値なし注釈もローカル宣言となるため、従来どおり候補を抑制する。値を伴う代入、既存のshadowing、動的namespaceの抑制も保持し、属性・subscript targetの評価式を走査する。class内のoperator参照に対する既存のメタクラス対策は変更しない。[^issue-562-design]

正式な公開planテストは、既存のLean生成14入力について候補の組を照合し、置換範囲を確認する。元コードと生成候補をCPython 3.14で実行し、候補がある入力では観測値の変化も確認する。解析器の単体テストには繰り返し宣言、source/destinationの両方、入れ子scope、既存束縛、複雑なtargetの副作用を加えた。実行結果と各3回のセルフレビューは作業記録を参照する。[^issue-562-tests][^issue-562-review]

この修正は候補精度の改善であり、任意のPythonプログラムに対する候補の完全性やRust実装全体の正しさを証明するものではない。上記の7 match / 7 mismatchは旧版の結果として保持する。

[^issue-562-design]: [修正設計](../../superpowers/specs/2026-09-25-issue-562-declaration-only-design.md)。
[^issue-562-tests]: [回帰テスト](../../../crates/hoimin-cli/tests/declaration_only_annotations.rs)。
[^issue-562-review]: [レビューと検証記録](../../superpowers/reviews/2026-09-25-issue-562.md)。
