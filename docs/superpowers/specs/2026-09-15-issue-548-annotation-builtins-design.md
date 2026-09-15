# Issue #548: Collection annotation builtin provenance

## 契約

`list`/`Sequence`、`set`/`AbstractSet`、`dict`/`Mapping` の両方向で、具体型名が注釈のscopeから組込み型を参照すると確認できる場合だけ候補を作る。shadowedまたはunknownは除外する。抽象型同士の候補は独立して維持する。

## 原因と選択

基準版は `f11013542ccd735ab9741b5079c0b39a517df256`。`KnownImports::resolved_name` は未登録名を綴りとして返し、`collection_replacements` は具体型名を無条件に採用する。import flowへ新しい束縛状態を複製する案は、既存のlexical scope解析との二重管理になる。runtime用の位置順resolverをそのまま呼ぶ案も、CPython3.14で注釈が遅延評価される点を扱えない。

既存の `NameResolutionIndex` に注釈位置を登録し、同じscopeと束縛集合から注釈用の保守的な解決を行う。関数引数・戻り値の注釈は関数本体のlocalを見ず、型パラメータと外側のscopeを見る。class直下の注釈はclass変数を見て、method本体や内側のclassは外側classを飛ばす。遅延評価されるmodule/classの名前は後続の束縛もunknownとして扱う。

type aliasのvalueと型パラメータのbound/defaultも注釈位置として登録する。alias自身の型パラメータscopeを作り、bound/defaultでは当該宣言の全型パラメータを確認する。Ruffの通常visitorがこれらを単なる式として訪問する点を補う。

## 検証

公開planで3組・両方向の正負例、moduleの前後の再代入、関数local、closure、class、PEP695の型パラメータを比較する。CPython3.14でcompileに加えて実際に `__annotations__` を評価し、維持した候補の型のoriginを確認する。型パラメータ自体を添字アクセスする元入力は評価時に失敗するため、その負例はコンパイルと候補抑制を確認する。

Leanはbuiltin/shadowed/unknownの許可条件と壊した述語の検出を証明し、期待値付きfixtureを生成する。Rust adapterは公開planに渡し、期待値を再実装しない。任意の実行時書換え、外部moduleによるbuiltins変更、Python全scopeの完全性は保証しない。
