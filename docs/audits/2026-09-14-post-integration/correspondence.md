# モデル化前の対応表

対象は `f110135`。主張は「finally 内で typing 由来として置換する名前は、その場所へ到達する全経路で typing 由来である」。既存 NestedTryFlow は渡された exit の合流を扱う。今回の対象は、その入力 exit に暗黙の例外経路が含まれるかである。

| 前提・観測 | Lean | 実装入力・観測 | 根拠 | mode |
| --- | --- | --- | --- | --- |
| 初期の Sequence は set | `known := false` | `Sequence = set` | CPython の finally 内 `record.__annotations__` | strict |
| import 成功後は typing.Sequence | `importTyping` | `from typing import Sequence` | CPython の型注釈評価 | strict |
| 呼出しが成功するか、束縛を変更せず例外になる | `mayRaise` | 外部から与える `hazard` が pass / KeyError | 同じ生成ソースを2種類の hazard で実行 | strict |
| 明示的 raise は後続を実行しない | `raiseNow` | `raise KeyError()` | 同上 | strict |
| finally への全入口で provenance が一致する | `entries.all id` | plan が生成する type_list_sequence 候補の有無 | analyzer/rust.rs `visit_try`, `apply_finally` | strict |
| 3イベントの全順序、深さ0〜4 | 最短優先列挙 | 全列挙分の CLI replay はしない | Lean の有限探索のみ | model-only |
| import は成功する | 決定的 import | 標準ライブラリ typing が利用可能な fixture | import 自体の失敗・動的 import hook は除外 | strict |
| annotation collector の入れ子再走査 | `loopVisits` | 変化しない空の import 状態、for の入れ子 | `loop_head_fixed_point` と `visit_loop` | model-only |
| 文ごとの全状態 clone の処理量 | `cloneFloor` | import N件の後に annotation M件 | `visit_statement_flow`, `visit_suite_flow` | model-only |

整数オーバーフロー、OS資源制御、全Python構文、並行実行は新モデルの範囲外。性能モデルの操作数は内部計数との照合をしていない。CLI実測とは独立の証拠として記録する。
