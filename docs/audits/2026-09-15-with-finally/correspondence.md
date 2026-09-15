# モデル化前の対応表

対象: `5e631ef`。主張は「with後でtyping由来と判断する名前は、その位置へ到達する全経路でtyping由来である」。例外を抑制するcontext managerでは、本体の例外終了もwith後へ到達する。既存のfinallyモデルには、この例外から正常継続への変換がない。

| 前提・観測 | Lean表現 | 公開入力・観測 | 証拠 | mode |
| --- | --- | --- | --- | --- |
| 初期Sequenceはset | `typing := false` | `Sequence = set` | CPythonの注釈評価 | strict |
| typing importは成功 | `importTyping` | 標準typingのimport | plan入力、CPython | strict |
| callが正常終了またはKeyError | `mayRaise` | 外から渡すhazardを2通り実行 | CPython | strict |
| KeyErrorを抑制 | `afterWith true` | `contextlib.suppress(KeyError)` | CPython | strict |
| 型候補を許可する条件 | 到達するnormalが全てtrue | type_list_sequence候補数 | 公開plan | strict |
| 深さ0〜4の全イベント列 | 3種のEvent | 全列の実装fixtureは生成しない | Lean有限探索 | model-only |
| finally本体を記録と転送で二重走査 | `visits (n+1) = 2 * visits n` | 入れ子try/pass/finallyのplan所要時間 | apply_finally / route_finally_entry | model-only |

対象外: import失敗、例外型の細分、context managerの動的な名前変更、return/break/continue、async実行、無限ループ。性能モデルの訪問回数はRustカウンタでは観測しないため、時間と定理をstrict比較しない。宣言された全経路合流と、visit_withが本体のfallthroughだけを維持する現状を分ける。

探索は最短優先、alphabet=3、深さ0から4まで1段ずつ。各Leanプロセス20秒・2GiB・単一スレッド、既定heartbeat上限を保持。性能測定も1深さずつ、子プロセス10秒で停止する。
