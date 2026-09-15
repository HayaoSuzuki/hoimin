---
type: Contract
title: コレクション型注釈の組込み型の参照先
description: list・set・dictを含む型注釈変異の両方向で、注釈scopeの組込み型provenanceを確認する条件。
status: draft
catalog_revision: f11013542ccd735ab9741b5079c0b39a517df256
sources:
- id: issue-548-design
  resource: ../../superpowers/specs/2026-09-15-issue-548-annotation-builtins-design.md
  revision: f11013542ccd735ab9741b5079c0b39a517df256
  working_tree: untracked
  sha256: 064489054f3c1e2bb0959094f1b81258d02fd2eec5ce89712509931b9d998218
- id: issue-548-implementation
  resource: ../../../crates/hoimin-cli/src/analyzer/rust.rs
  revision: f11013542ccd735ab9741b5079c0b39a517df256
  working_tree: modified
  sha256: 03a99d8aa327a0081245c1e98ff7c4ce00854b27b53062f5c74caf100ad840bf
- id: issue-548-tests
  resource: ../../../crates/hoimin-cli/tests/collection_annotation_builtins.rs
  revision: f11013542ccd735ab9741b5079c0b39a517df256
  working_tree: untracked
  sha256: b86a5a5830d386568f6cded7bfcc47a5e2398d598f8931828ac2e6729eb7c4dc
- id: issue-548-lean
  resource: ../../../formal/HoiminOracle/CollectionAnnotationAuditMain.lean
  revision: f11013542ccd735ab9741b5079c0b39a517df256
  working_tree: untracked
  sha256: 156319a1307d4e3876eb23f4ffdb910d04f291de06d27ebb3d519d6b2d3eff3c
- id: issue-548-report
  resource: ../../superpowers/reports/2026-09-15-issue-548-annotation-builtins-review.md
  revision: f11013542ccd735ab9741b5079c0b39a517df256
  working_tree: untracked
  sha256: 20a2db1609dc6fc42f31a451b1a3a21113cba2d04cebc6847c77e43afd7176a4
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

114件のLean生成fixtureは公開planと一致し、別途42件の注釈位置・alias、48件のalias value・bound/default、抽象型同士の1件を確認した。CPython3.14.7では元入力と保持候補の注釈または遅延値を実際に評価した。元から不正なgeneric concrete sourceの評価エラーは別扱いである。[^issue-548-tests][^issue-548-report]

Leanは許可からbuiltinであることを導く性質と、shadowed/unknownの拒否を確認する。fixtureのscopeがモデル前提に対応することは、有限入力の実装比較による証拠である。[^issue-548-lean]

[^issue-548-implementation]: [rust.rs](../../../crates/hoimin-cli/src/analyzer/rust.rs)。

[^issue-548-tests]: [collection_annotation_builtins.rs](../../../crates/hoimin-cli/tests/collection_annotation_builtins.rs)。

[^issue-548-lean]: [CollectionAnnotationAuditMain.lean](../../../formal/HoiminOracle/CollectionAnnotationAuditMain.lean)。

[^issue-548-report]: [2026-09-15-issue-548-annotation-builtins-review.md](../../superpowers/reports/2026-09-15-issue-548-annotation-builtins-review.md)。
