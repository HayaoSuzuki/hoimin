---
type: Audit
title: 遅延注釈とcollections.abc.Setの追加監査
description: 5e631efにおけるtyping aliasの遅延評価と集合ABCの綴り、モデル証明と公開CLI照合。
status: draft
catalog_revision: 5e631ef
sources:
  - id: issue-558-design
    resource: ../../superpowers/specs/2026-09-24-issue-558-deferred-imports-design.md
    revision: a49467ded359416a6ee743536100634f1cced3e2
    working_tree: modified
    sha256: 5ce8d07b9fa810c34577123b722408a7b4621039f30269935dee7101f6271997
  - id: report
    resource: ../../audits/2026-09-15-annotation-followup/README.md
    working_tree: untracked
  - id: model
    resource: ../../audits/2026-09-15-annotation-followup/AnnotationModel.lean
    working_tree: untracked
  - id: issue-559-repair
    resource: ../../superpowers/specs/2026-09-24-issue-559-design.md
    revision: 98e78166b43940df38a8bb8099c9c6af6004ba5a
    working_tree: clean

---

# 確認した問題

`5e631ef`の追加監査で、Python 3.14の遅延注釈が参照するtyping aliasの後続再代入を無視する問題を[#558](https://github.com/tokyogas-tech/hoimin/issues/558)に起票した。置換元と置換先、クラスの後続束縛で不適切な候補が残る。初回参照で値を保持する正例も確認した。[^report]

集合抽象型については、collections.abc.SetをAbstractSetと綴る問題を[#559](https://github.com/tokyogas-tech/hoimin/issues/559)に起票した。module importでは存在しない属性への候補、直接importでは両方向の候補欠落となる。[^report]

# 証拠と限界

Lean生成の10入力を公開planで照合し、debug/releaseそれぞれ4 match / 6 mismatchとなった。元ソースの評価は全件成功。生成候補のabc.AbstractSetはAttributeErrorとなる。Leanが期待する7置換は全てCPythonで評価でき、2件の公開runで不適切なkilledの計上を確認した。既存テスト102件は成功した。[^report]

モデルでは初回注釈評価後の値保持を任意長の操作列について証明し、provider別の置換先メンバーの存在を確認した。有限探索は3イベント、深さ0〜4。10fixtureの候補組比較はstrict、全列と任意長の定理はmodel-onlyとして区別する。Rust実装全体や任意のPython評価時期の証明ではない。[^model][^report]

# #558の修正範囲（2026-09-24）

#558では、宣言位置のimport情報だけでなく、注釈から見えるscopeの後続束縛を確認するように変更した。置換元・置換先の再代入とクラスの後続束縛を除外し、不変のimportは維持する。公開runの回帰テストでは、元注釈が`set[int]`となる入力で不適切な候補をkilledに計上しないことを確認する。[^issue-558-design]

初回評価済みキャッシュと、注釈より後のimport復元は、正しい候補も保守的に除外する。関数内変数の注釈は実行時評価されないため従来のsource-order条件を維持する。上記の監査結果は`5e631ef`の履歴であり、この修正の実行結果に読み替えない。#559のcollections.abc.Setの綴りは、併合元の修正に含まれる。[^issue-558-design]

# 再確認の契機

AnnotationCollectorの評価時期、KnownImports、collection_replacements、spelling_for、既知の型名一覧を変更するときに、監査の正例・負例を再実行する。修正時には#264のdefinition-time契約との違い、future annotations、旧Pythonの扱いを整理し、正式CIへの回帰ケース追加を記録する。

[^report]: [追加監査・再現手順](../../audits/2026-09-15-annotation-followup/README.md)。
[^model]: [AnnotationModel.lean](../../audits/2026-09-15-annotation-followup/AnnotationModel.lean)。

## 2026-09-24: Issue #559の修正

2026-09-24の#559修正では、集合抽象型の対応をtyping.AbstractSetとcollections.abc.Setへ訂正した。module importと直接importの両方で別名と双方向の候補を扱い、置換先の修飾名には参照元モジュールのメンバー名を使う。typing.Setはこの抽象型の組合せへ追加しない。#558の遅延評価は別Issueとして扱う。[^issue-559-repair]

[^issue-559-repair]: [修正設計](../../superpowers/specs/2026-09-24-issue-559-design.md)。

[^issue-558-design]: [Issue #558: imported names in deferred annotations](../../superpowers/specs/2026-09-24-issue-558-deferred-imports-design.md)。
