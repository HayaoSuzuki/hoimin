---
type: Contract
title: コレクション型注釈の組込み型の参照先
description: list・set・dictを含む型注釈変異の両方向で、注釈scopeの組込み型provenanceを確認する条件。
status: draft
catalog_revision: f11013542ccd735ab9741b5079c0b39a517df256
sources:
- id: issue-564-design
  resource: ../../superpowers/specs/2026-09-24-issue-564-nullable-provenance.md
  revision: c1ce10402df3a26ce0c9e3eb1e7d524fcf4c2aca
  working_tree: clean
  sha256: a0237a6eca382f65069b6a15eb7681c155f6b94f468d8b09725cff7a9f514f8e
- id: issue-564-tests
  resource: ../../../crates/hoimin-cli/tests/nullable_builtin_provenance.rs
  revision: c1ce10402df3a26ce0c9e3eb1e7d524fcf4c2aca
  working_tree: untracked
  sha256: bad1dd2514554710dd6b388caa74e6833e63bfd77119e6c0104bc2d7b25cd3a4
- id: issue-548-design
  resource: ../../superpowers/specs/2026-09-15-issue-548-annotation-builtins-design.md
  revision: f11013542ccd735ab9741b5079c0b39a517df256
  working_tree: untracked
  sha256: 064489054f3c1e2bb0959094f1b81258d02fd2eec5ce89712509931b9d998218
- id: issue-548-implementation
  resource: ../../../crates/hoimin-cli/src/analyzer/rust.rs
  revision: f11013542ccd735ab9741b5079c0b39a517df256
  working_tree: modified
  sha256: ce8edf8778d99da3bba284c21c859d3c331aeec5455745984988f62e59b2893d
- id: issue-548-tests
  resource: ../../../crates/hoimin-cli/tests/collection_annotation_builtins.rs
  working_tree: modified
  sha256: 6b9824b1114fde4cb059cc0ebfb1898c03138e49e57aae5eb3e6cfab5bab514e
- id: issue-548-lean
  resource: ../../../formal/HoiminOracle/HoiminOracle/CollectionAnnotationModel.lean
  working_tree: modified
  sha256: fa0dba0ccc0b596b346dbf88abd7a671c628db8d5494b000cdb6e650db97c194
- id: issue-548-report
  resource: ../../superpowers/reports/2026-09-15-issue-548-annotation-builtins-review.md
  revision: f11013542ccd735ab9741b5079c0b39a517df256
  working_tree: untracked
  sha256: 20a2db1609dc6fc42f31a451b1a3a21113cba2d04cebc6847c77e43afd7176a4
- id: audit-promotion
  resource: ../../superpowers/reports/2026-09-15-audit-verification-promotion.md
  working_tree: untracked
  sha256: 3a3a79631919bc7f2d52b7e73c888345bf12050e7b5a2b198b7b24986b5191b1
---

# 対象と契約

`list`/`Sequence`、`set`/`AbstractSet`、`dict`/`Mapping` の変異では、sourceまたはdestinationに含む具体型名が、注釈のscopeで組込み型を参照すると確認できる場合だけ候補を生成する。型パラメータ、他の値への代入、動的な名前変更などでshadowedまたはunknownとなる場合は抑制する。抽象型同士のSequence/Iterable変異は、この具体型名の判定に依存しない。[^issue-548-design]

# 注釈scopeと遅延評価

関数引数・戻り値の注釈は、その関数の通常localと分けて解決する。同名の引数や本体の代入はheaderの組込み型を隠さないが、外側の関数localと型パラメータは隠す。class直下の注釈はclass変数を参照し、methodの通常lexical探索と内側classでは外側classを飛ばす。CPython3.14の遅延評価に備え、module/classの後続束縛も不確実性として扱う。[^issue-548-design]

# 検証と限界

Leanモデルは3値の許可条件を対象とし、生成したfixtureを公開planへ渡して比較する。コンパイルだけでなく `__annotations__` の評価を確認する。元からTypeVarへ添字を付ける入力の評価エラーは、正常な元入力の検証とは分ける。任意の実行時書換えや外部moduleによるbuiltins変更の完全な解析は保証しない。[^issue-548-design]

resolver、type parameterのscope、注釈の遅延評価、collection pairを変更した場合は、両方向の候補と注釈評価を再確認する。[^issue-548-design]

[^issue-548-design]: [Issue #548: Collection annotation builtin provenance](../../superpowers/specs/2026-09-15-issue-548-annotation-builtins-design.md)。

# 今回の実装と観測

注釈位置のscopeを既存の名前解決索引へ記録し、alias valueと型パラメータのbound/defaultにも同じ走査を行う。dictも束縛の追跡対象へ追加した。[^issue-548-implementation]

初回の114件に監査原文の未収録7件を追加し、121件のLean生成fixtureを公開planと照合した。監査15入力の残る8件は既存fixtureと原文が一致する。[^audit-promotion]

初回検証では、別途42件の注釈位置・alias、48件のalias value・bound/default、抽象型同士の1件を確認した。CPython3.14.7では元入力と保持候補の注釈または遅延値を実際に評価した。元から不正なgeneric concrete sourceの評価エラーは別扱いである。[^issue-548-tests][^issue-548-report]

Leanは独立したCollectionAnnotationModelで、許可からbuiltinであることを導く性質と、shadowed/unknownの拒否を確認する。fixtureのscopeがモデル前提に対応することは、有限入力の実装比較による証拠である。[^issue-548-lean]

[^issue-548-implementation]: [rust.rs](../../../crates/hoimin-cli/src/analyzer/rust.rs)。

[^issue-548-tests]: [collection_annotation_builtins.rs](../../../crates/hoimin-cli/tests/collection_annotation_builtins.rs)。

[^issue-548-lean]: [CollectionAnnotationModel.lean](../../../formal/HoiminOracle/HoiminOracle/CollectionAnnotationModel.lean)。生成は[専用生成器](../../../formal/HoiminOracle/CollectionAnnotationAuditMain.lean)から行う。

[^issue-548-report]: [2026-09-15-issue-548-annotation-builtins-review.md](../../superpowers/reports/2026-09-15-issue-548-annotation-builtins-review.md)。

[^audit-promotion]: [監査コード・ケースの正式な検証への移行](../../superpowers/reports/2026-09-15-audit-verification-promotion.md)。

# nullable追加への適用（Issue #564）

nullable追加にも注釈scopeの組込み型判定を適用する。str/int/float/bool/bytesを名前追跡の対象に加え、list/set/dictの型構成子と入れ子の型引数内の組込み名を確認する。通常の組込み型と、標準ライブラリから導入した型構成子の候補は保持する。collection変異とnullable除去の条件は維持する。[^issue-564-design]

公開planの回帰テストでは、CPython3.14で元の注釈を評価してから候補を照合する。独自メタクラスのunion演算が例外となるクラス、各組込み名の先行・後続再代入、通常の組込み型と保持した候補の注釈評価を対象とする。型チェッカーのbaseline・スコアや、任意の動的書換えの解析を保証するものではない。[^issue-564-tests]

[^issue-564-design]: [Issue 564: nullable annotation builtin provenance](../../superpowers/specs/2026-09-24-issue-564-nullable-provenance.md)。
[^issue-564-tests]: [nullable追加の公開planテスト](../../../crates/hoimin-cli/tests/nullable_builtin_provenance.rs)。
