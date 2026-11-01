# Issue #549: Sliceを含むtupleの候補除外

## 契約と原因

Ruffは `x[:,]` の添字を、直接のSlice要素を含むExprTupleとして表す。collection_list_tupleは現在これを `[:,]` にし、Pythonの式ではないcolonをリスト内へ移してしまう。対象の比較元は f11013542ccd735ab9741b5079c0b39a517df256。

## 採用する設計

collect_tuple_literalで直接の要素にExpr::Sliceがあれば候補生成を返す。通常の式、starred、ネストしたtupleは従来の条件で生成する。返る範囲は収集関数だけなのでvisitorの子探索を続け、start/stop/stepのtuple候補を保持する。

subscript全体の除外は `x[1, 2]` の合法な候補も失う。置換文字列からcolonを検索すると文字列や入れ子のsubscriptまで除外するため採用しない。helper内で全構文を再parseする方法は、このAST条件に対して不要な処理を増やす。

## 検証の境界

Leanは直接の要素を式とSliceに分け、Sliceなしの場合に限る許可と、常に許可する壊したモデルの反例を検査する。長さ1〜3の14入力、start/stop/step、合法なstarredと入れ子を既存の有効Python corpusへ追加する。既存adapterで元ソースcompile、候補位置、公開plan、適用後compileを確認する。import-onlyの公開runはkilledが0であることを独立に確認する。全Python構文やRust全体の証明とはしない。

実施手順は[計画](../plans/2026-09-15-issue-549-slice-tuple.md)、各段階の点検と実行結果は[報告](../reports/2026-09-15-issue-549-slice-tuple.md)に残す。
