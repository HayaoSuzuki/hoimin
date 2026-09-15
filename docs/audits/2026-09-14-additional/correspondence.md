# モデル化前の対応表

対象HEADはf110135。主張は「型注釈のlist/Sequence、set/AbstractSet、dict/Mappingの置換では、具体型側の名前がその場所で組込み型を参照する」。抽象型側は通常のtyping importとし、その再代入・import失敗を今回のモデルから除外する。

| 前提・観測 | Lean | 実装・公開観測 | mode |
| --- | --- | --- | --- |
| 組込み名の通常解決 | Fact.builtin | importだけのmoduleでrecord.__annotations__を評価 | strict |
| moduleで具体型名を再代入 | Fact.shadowed | list=tuple、set=frozenset、dict=tuple | strict |
| generic functionの型パラメータ | Fact.shadowed | `def record[list/set/dict](value: 抽象型): pass` | strict |
| builtinがsource/destinationの2方向 | Bool reverse | 両向きのplan候補数と生成descriptorを保存 | strict |
| 不明なprovenance | Fact.unknown | 今回は実CLI入力を作らない | model-only |
| 3種類のpair | source/target/operator文字列 | type_list_sequence/type_set_abstract_set/type_dict_mapping | strict |

全候補descriptorのLean期待値は生成しない。照合する意味的な観測は候補数。descriptorとPythonでのannotation評価は追加証拠として保存する。OS/DB/並行性、動的namespace、全Python構文は対象外。
