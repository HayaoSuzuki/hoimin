## 確認した問題

型注釈の具体型/抽象型変異が、`list`・`set`・`dict`の実際の束縛を確認せず、名前の綴りだけで組込み型と判断する。moduleでの再代入とPEP 695型パラメータの両方で再現した。

対象: `f11013542ccd735ab9741b5079c0b39a517df256`、macOS arm64、CPython 3.14.7、debug/release CLI。優先度案: P2。

## 最小例1: 変異先が型パラメータ

```python
from typing import Sequence
def record[list](value: Sequence[int]):
    pass
```

```sh
hoimin plan --root /path/to/project --file subject.py \
  --operators type_list_sequence --allow-best-effort-memory -- true
```

期待: `list`は型パラメータなので候補を出さない。

実際: `Sequence[int] → list[int]` を1件生成する。元ソースの `record.__annotations__['value']` は `typing.Sequence[int]`。変異後は `TypeError: 'typing.TypeVar' object is not subscriptable`。compileや関数定義だけでは失敗せず、遅延された型注釈を評価した時点で失敗する。

## 最小例2: 通常の再代入

```python
from typing import Sequence
list = tuple
def record(value: Sequence[int]):
    pass
```

同じ操作で `Sequence[int] → list[int]` を生成するが、変異先は `tuple[int]` になる。sourceを `list[int]` にした逆方向でも `list[int] → Sequence[int]` を生成し、実際にはtupleをSequenceへ変更している。

`set = frozenset` と `AbstractSet`、`dict = tuple` と `Mapping` でも同じ問題がある。演算子はそれぞれ `type_set_abstract_set`、`type_dict_mapping`。

これは変異が例外を起こすこと自体を問題にするものではない。意図した型pairとは別の型やTypeVarを、組込み型と誤認している問題。

## 原因

- [KnownImports::resolved_name](https://github.com/tokyogas-tech/hoimin/blob/f11013542ccd735ab9741b5079c0b39a517df256/crates/hoimin-cli/src/analyzer/rust.rs#L3721) はdirect/modulesにない名前を、そのままの文字列として返す。shadowされた具体型名と本当のbuiltinを区別しない。
- [collection_replacements](https://github.com/tokyogas-tech/hoimin/blob/f11013542ccd735ab9741b5079c0b39a517df256/crates/hoimin-cli/src/analyzer/rust.rs#L6005) は `list`/`set`/`dict` の文字列をsourceとして受理し、逆方向では同じ名前を無条件でreplacementに使う。
- `enter_type_params`で記録したtype_varsも、このreplacement名の解決には使用されない。
- 同じ解析器のruntime builtin pairにはsource/destinationの名前解決があるが、この型注釈経路に接続していない。

## 既存Issue・検証との差

#486はruntimeの `list(...) → tuple(...)` 等についてPEP 695のscopeを修正したもの。本件は型注釈の具体型/抽象型変異で、通常のmodule再代入でも発生する。#545の暗黙例外やfinallyも不要な独立した原因。

今回、既存 `lean_annotation_scope_oracle` と `type_parameter_bindings` は全12件成功した。既存のscope検査が今回の具体型名の両方向を保証しているとは言えない。compileの成功だけでも、PEP 695の遅延された注釈評価エラーを検出できない。

## Leanと実行確認

Leanの最小モデルでbuiltin/shadowed/unknownの3値と、具体型名がbuiltinである場合だけ許可する述語を定義した。許可からbuiltinを導く定理、shadowed/unknownの拒否、綴りだけを信頼する壊した述語の検出をカーネル検査した。

3状態×2方向×3型pairの18述語ケースで壊したモデルとの差は12件。実CLIには、各pairについて正常2方向・module shadow2方向・genericのdestination shadowの5種類、計15のLean生成fixtureを適用した。debug/releaseそれぞれ6 match / 9 mismatch。CPythonで全元ソースの注釈評価が成功し、全変異の注釈も評価した。genericの3件は上記TypeError、module shadowの6件は別の型を観測した。

Leanが証明したのはこのモデルの性質。実装との照合は生成した15入力に限定し、全Python scope・全入力への証明とはしない。

## 対応案・受け入れ条件

- sourceとdestinationの具体型名のprovenanceを、型注釈の実際のscopeで確認する。shadowed/unknownなら候補を抑制する。
- 型パラメータ・module再代入・関数local・closure/classの可視性を考慮し、runtime向け解決器を流用する場合も注釈のscopeとの差を確認する。
- list/Sequence、set/AbstractSet、dict/Mappingの両方向に負例と正例を追加する。
- PEP 695の例はcompileに加え、CPython 3.14で `__annotations__` を評価する。
- builtinが通常どおり見える正例は維持し、単に全型候補を抑制して通さない。

今回production修正はしていない。
