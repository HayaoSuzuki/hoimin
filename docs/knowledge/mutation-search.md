---
type: Decision
title: 制約付き変異探索と差分テスト生成の採否
description: proofreadtoolsとDjangoの単一編集探索、HypothesisとCrossHairの実測範囲と採否。
status: draft
catalog_revision: f397262
sources:
- id: django-experiment
  resource: ../superpowers/reports/2026-10-11-issue-790-django.md
  revision: 93116fed96602628d04a2139645b3dfe1166e168
  working_tree: modified
  sha256: 7ae28e09ba8ddc8e2ad6f1dd8263cefb4ea770dfb58731cd565b67cf578c5689
- id: experiment
  resource: ../superpowers/reports/2026-10-11-issue-790-proofreadtools.md
  revision: 93116fed96602628d04a2139645b3dfe1166e168
  working_tree: modified
  sha256: cc82e35bb1e1d24398826289ca1225aa80a900e620be580163bf66d8175e387e
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

## 終了判断と保存範囲

2026-10-11、調査結果だけを残し、研究用コード・契約テスト・CI変更を最終PRから外して終了した。
自動生成の成立例は得られたが、誘導探索の優位と継続保守に見合う効果は確認できず、製品機能への採用と追加実験は見送る。
設計・計画・レビューは実験時の記録であり、今後の実装予定ではない。
再実行には両報告が示す過去の実験コミットを使う。[^experiment][^django-experiment]

## 実験時の適用範囲

実験時の単一AST編集とHypothesisによる入力生成は、信頼する固定純粋関数への開発用PoCに限定した。
無作為と誘導は同じ候補を各3反復で発見し、この小さい有限空間では誘導の優位を示せなかった。
CrossHairでも同じ差を実CPythonで確認できたが、今回のCLI費用はHypothesisより大きく、製品組込みを見送る。[^experiment]

生成した期待値はcharacterizationであり、人による仕様確認とテスト採用は未実施。
抽出関数内の差だけをkillとせず、実source pathの確認、独立した元・変異観測、新テストのfingerprintとworkerを必要とする。
通常CLI、plan schema、既定operator、Rust製品依存、配布仕様は変更しない。[^design]

## 限界と再検討条件

全体軽量suiteはnative threadを各1、1GiB・16プロセスの上限で5,053件成功したが、optional PDF依存6件のskipとslow53件の除外を含む。
未実測OS、任意の外部入力、汎用adapter、高次変異、モデル取得へ保証を広げない。
人手で調査しにくい別の具体的survivorが継続して発生した場合に、保守費用とレビュー費用を含む次の評価を検討する。[^experiment]

## Djangoへの拡張

固定SHA `dab0a5c47f159eb58472110dcbc10bd4a9b01adb` の3関数を5 seed・3方式で評価した。
既存236候補・生成162候補のpoolから71回・35候補IDを評価し、41回はassertion検出、30回はその他エラー、生存は0件だった。
Djangoの入力探索・生成テスト照合は起動しておらず、新しい採用根拠は得られていない。[^django-experiment]

候補開始予算30秒に対してreset等を含む実wallは最大35.547秒となった。
fixedの対象関数の偏りと少ない評価件数もあり、優位性を示す比較ではない。
repository規模だけを増やす前に、評価件数の確保、関数ごとの配分、復元費用の内訳、型不整合を減らす候補生成を検討する。[^django-experiment]

## 出典

[^experiment]: [実験報告と機械可読の証拠](../superpowers/reports/2026-10-11-issue-790-proofreadtools.md)。
[^design]: [承認済み設計](../superpowers/specs/2026-10-11-issue-790-proofreadtools-design.md)。

[^django-experiment]: [Djangoでの制約付き変異探索](../superpowers/reports/2026-10-11-issue-790-django.md)。
