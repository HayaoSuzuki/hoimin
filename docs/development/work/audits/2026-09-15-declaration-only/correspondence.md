# モデル化前の対応表

対象HEADは5e631ef。候補精度の改善案は「値を伴わない名前の型注釈では、module/classの実行時の束縛と、既存のmodule importの同一性を維持する」。関数のローカル宣言と実行時の値は区別する。

既存設計は保守的な候補欠落を許容するため、候補が欠落した観測を安全性違反とは呼ばない。Pythonの実行意味を根拠に候補精度の改善を提案する。Leanの期待値は改善後の契約であり、現行の完全性保証ではない。

| 前提・観測 | Lean表現 | 実装との対応 | mode |
| --- | --- | --- | --- |
| module/class/function | Scope | 公開Python fixtureの宣言位置 | strict |
| 局所値なし・既知・別物 | Value.missing/known/other | 組込みへのfallback、無条件module import、利用者関数 | strict |
| 外側に既知の名前がある | fallback Bool | any/allまたは外側の利用者関数 | strict |
| 値のない名前の注釈 | annotate false | AnnAssign.value=None | strict |
| 利用者関数を値として代入する注釈 | annotate true | AnnAssign.value=Some(lambda) | strict |
| functionの注釈対象はローカル宣言 | scope判定 | fixtureを呼びUnboundLocalErrorを捕捉 | strict |
| 変異の可否 | resolvesKnown | operator限定planのoriginal/replacement組 | strict |
| 有限な抽象状態の全組合せ | 3 scope × 3 value × 2 fallback × 2 RHS | 全組合せのPython化は行わない | model-only |
| 反復する値なし注釈の値保持 | 任意Nat回の定理 | 有限fixtureでは一般性を観測できない | model-only |

関数内のlocal importは現行operator設計の対象外なので、候補追加を要求しない。属性・subscript targetの評価副作用、global/nonlocal、メタクラス、注釈式の評価、副作用を伴う旧Pythonの注釈評価、例外・並行処理は除外する。全fixtureはCPython 3.14.7で注釈値を参照せず評価する。

評価はimportされるモデルと独立した生成器に分ける。3種類のscopeを1つずつ追加し、各段階の有限状態数・遷移数・経過時間を記録する。深い探索は不要。各Leanプロセスは20秒、2GiB、50ms監視、単一スレッド、既定heartbeat上限で実行する。
