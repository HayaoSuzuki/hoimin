# モデル化前の対応表

対象 `5e631ef`。契約は、型演算子が既知の標準型の組合せを変更し、置換先を既存の安全なimportで綴ることである。宣言時のtyping importと評価時のimportはPython 3.14で異なる場合がある。typing.AbstractSetに対応するcollections.abcの型はSetである。

| 前提・観測 | Lean表現 | 公開入力と観測 | mode |
| --- | --- | --- | --- |
| 関数定義時にSequenceはtyping由来 | State.current=true | typing importの後にdef | strict |
| 定義後の再代入・復元 | rebind/restore | Sequence=set、再import | strict |
| 初回の注釈参照は値を保持する | observe、cached | f.__annotations__を参照 | strict |
| 後続から参照する前に再代入 | rebind,observe | 元注釈と候補をCPythonで評価 | strict |
| クラスの後続ローカル束縛 | 同じrebind規則 | C.Sequence=set、C.fの注釈 | strict |
| 正しい標準型名 | ProviderとMember | typing.AbstractSet、collections.abc.Set | strict |
| 候補のoriginal/replacement集合 | Lean生成pairs | 公開planのdescriptorを射影 | strict |
| 全イベント列 | 3イベント、深さ0〜4 | 全列のPython入力は作らない | model-only |
| 初回評価後の値保持 | 任意長のtraceに関する定理 | 指定fixtureの有限観測のみ | model-only |

入力はASCII、単一注釈、既知の標準import、通常のmodule/class。import失敗、任意の動的hook、注釈キャッシュの手動削除、future annotations、Python 3.13以前、async、並行評価は除外する。標準ライブラリのメンバー集合はモデル内の前提であり、CPython照合と公式文書で確認する。

各Lean処理は20秒・2GiB・50ms監視、1スレッド、既定heartbeat上限。探索は深さ0から1段ずつ、3イベントの最短優先・安定順序、対称性による削減なし。internal-fixtureは用いない。
