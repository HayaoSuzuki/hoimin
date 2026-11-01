# Issue #545: 暗黙例外の finally 入口

## 問題と契約

基準版は `f11013542ccd735ab9741b5079c0b39a517df256`。通常式が失敗すると後続 import を飛ばして finally に入る。注釈に対する typing 由来の判定は、この入口と正常入口の双方で一致する場合だけ保持する。

## 採用する設計

`ControlFlowExits` に暗黙例外の環境を追加する。同じ例外分類の入口は交差でまとめ、保持する環境を1件に抑える。通常式の評価前の環境から式内の束縛変更を保守的に無効化し、到達可能な文だけで記録する。call、subscript、attribute に加え、名前読取り・演算・比較・真偽判定・コンテナのhash/展開・書式化・assert・反復・context manager・class 構築を例外入口として扱う。複数代入、unpack、with/for target、match capture は例外前に一部が束縛される可能性を考え、対応する名前を無効化する。class 本体の global/nonlocal 名は外側環境で無効化し、class ローカル環境を外へ伝播しない。handler の型式は handler 入口で評価する。文の子 suite は個別の制御フロー解析に任せ、関数本体や lambda 本体の遅延実行を外側の入口として扱わない。

finally は暗黙例外も注釈の入口集合へ加える。finally が通常終了した暗黙例外は引き続き暗黙例外として外側へ渡す。finally 内の break/continue/return/raise は既存の置換規則に従う。正常 fallthrough へ暗黙例外を合流しない。

全 try の初期環境を無条件で加える案は、import が hazard より前にある正例まで抑制するため採用しない。暗黙例外を既存 terminates へ直接入れる案は、return と明示的 raise の既存対応観測を変えるため採用しない。

## 検証と限界

Lean の小モデルから Python fixture と候補数を生成し、公開 plan へ適用する。call/subscript/attribute の負例、import 先行、明示的 raise、正常注釈を比較する。Rust の追加ケースで入れ子、else/handler、到達不能、遅延関数、finally 上書き、および演算・文単位の例外入口を確認する。

import 自体の失敗、任意の動的 hook、副作用の完全な評価順序、全 Python 構文はこの小モデルの証明対象外。既存 NestedTryFlow と break/continue/return の対応試験を別途実行する。
