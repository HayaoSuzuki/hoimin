# モデル化前の対応表

主張: collection_list_tupleによるリストへの変換は、リスト内で合法な式だけを要素として持つ場合に限る。RuffのExprTupleは、通常の式タプルに加えて多次元subscriptのslice集合も表す。

| 前提・観測 | Lean | 公開入力・観測 | mode |
| --- | --- | --- | --- |
| 通常の式要素 | Item.scalar | 整数1 | strict |
| 添字専用のスライス要素 | Item.slice | `:` | strict |
| タプル状の添字、1〜3要素 | List Item | `x[items,]` の関数をplanへ渡す | strict |
| リストへ移動できるか | eligible | collection_list_tupleの候補数 | strict |
| 元構文・変異の合法性 | eligibleの独立検証 | CPython3.14 compile | strict |

全候補descriptorのLean期待値は作らず、候補数を照合する。descriptorとcompile観測は別途保存する。空の添字、代入先、全Python式、全operator、OSや並行状態は範囲外。モデルに永続ID・transaction・retryはない。
