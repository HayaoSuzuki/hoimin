# モデル化前の対応表

対象HEADは5e631ef。既存の型アノテーション設計にある「既知の型構成子に限定し、対象外構文を含む注釈にはnullable追加を行わない」を確認する。今回は名前解決と、複数型引数内部の検査を分ける。

| 前提・観測 | Lean表現 | 公開Python入力・観測 | mode |
| --- | --- | --- | --- |
| 対象型名が既知の組込みを指す | trusted Bool | int/str/listを再束縛していない入力と、前に再束縛した入力 | strict |
| 対象内の原子型 | Tree.atom true | int/strの型引数 | strict |
| 対象外構文 | Tree.atom false | Any、object、TypeVar、前方参照、Annotated、Callable | strict |
| 単一型引数・ネスト | Tree.one | list[T] | strict |
| 複数型引数 | Tree.pair | dict[K,V]のExpr::Tuple slice | strict |
| nullable追加可否 | trusted && clean tree | operator限定planのoriginal/replacement組 | strict |
| 任意の木・禁止要素の子孫 | ContainsBlocked、帰納的定理 | 任意のASTは実行しない | model-only |
| 深さ0〜2の全木 | 2原子・unary・binary、固定順序 | 全木はPython化しない | model-only |

注釈はPython 3.14.7のannotationlibで明示評価し、元と実際の候補の結果を別々に記録する。評価例外は候補の意味の観測であり、プロセスの起動失敗・timeoutとは区別する。型チェッカーのスコア、型の部分型関係、後続再代入による遅延注釈の参照先変化（#558）、collection演算子の解決（#548/#559）、構文全般、任意の属性解決は対象外。

名前のtrusted値は解析器から逆算せず、fixtureの明示的な宣言・代入から与える。木の形は手作りPython fixtureの型引数構造と対応する。既存設計が対象外とする条件をモデルから除かない。Leanはモデルと生成器を分離し、深さ0から1ずつ増やして測定する。20秒・2GiB・50ms監視、単一スレッド、既定heartbeat上限を維持する。
