---
type: Decision
title: 制約付き変異探索と差分テスト生成の採否
description: proofreadtoolsの固定関数による単一編集探索、HypothesisとCrossHairの実測範囲と採否。
status: draft
catalog_revision: f397262
sources:
- id: experiment
  resource: ../superpowers/reports/2026-10-11-issue-790-proofreadtools.md
  working_tree: untracked
  sha256: eba7a49de174695b7978efe50a7a282110db5ab23630a2032e4bfcecbfa6b9fa
- id: design
  resource: ../superpowers/specs/2026-10-11-issue-790-proofreadtools-design.md
  revision: 28fd9bf
  working_tree: clean
  sha256: 6958f99389c4eb4c23b69211da0cd9b493bed00beed2630f549b60924e4c758e
---

# 制約付き変異探索と差分テスト生成の採否

## 対象と結果

#790・#776・#777の研究対象として、proofreadtoolsの固定SHA `409ed955c0695ec04976842ba9fd22c64c03c33a` の長さ比関数をWindows・CPython 3.14で評価した。
生成した18候補のうち既存operatorと正規化ASTが一致するものは4件で、既存の関連テストを通る別の候補1件から具体例テストを生成し、実workerで元成功・変異assertion失敗を確認した。[^experiment]

## 採用範囲

単一AST編集とHypothesisによる入力生成は、信頼する固定純粋関数への開発用PoCに限定する。
無作為と誘導は同じ候補を各3反復で発見し、この小さい有限空間では誘導の優位を示せなかった。
CrossHairでも同じ差を実CPythonで確認できたが、今回のCLI費用はHypothesisより大きく、製品組込みを見送る。[^experiment]

生成した期待値はcharacterizationであり、人による仕様確認とテスト採用は未実施。
抽出関数内の差だけをkillとせず、実source pathの確認、独立した元・変異観測、新テストのfingerprintとworkerを必要とする。
通常CLI、plan schema、既定operator、Rust製品依存、配布仕様は変更しない。[^design]

## 限界と再検討条件

全体軽量suiteはnative threadを各1、1GiB・16プロセスの上限で5,053件成功したが、optional PDF依存6件のskipとslow53件の除外を含む。
未実測OS、任意の外部入力、汎用adapter、高次変異、モデル取得へ保証を広げない。
人手で調査しにくい別の具体的survivorが継続して発生した場合に、保守費用とレビュー費用を含む次の評価を検討する。[^experiment]

## 出典

[^experiment]: [実験報告と機械可読の証拠](../superpowers/reports/2026-10-11-issue-790-proofreadtools.md)。
[^design]: [承認済み設計](../superpowers/specs/2026-10-11-issue-790-proofreadtools-design.md)。
