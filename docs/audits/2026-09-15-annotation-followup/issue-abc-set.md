## 問題

`type_set_abstract_set` がcollections.abcの集合抽象型を `AbstractSet` と誤って綴る。`import collections.abc as abc` があるだけで、`set[int] → abc.AbstractSet[int]` という存在しない属性への変異を生成する。注釈評価でAttributeErrorとなり、実runではkilledに計上される。

正しい型は `collections.abc.Set`。typing側の名前は `typing.AbstractSet` であり、同名ではない。[collections.abc公式文書](https://docs.python.org/3.14/library/collections.abc.html#collections.abc.Set)、[typing公式文書](https://docs.python.org/3.14/library/typing.html#typing.AbstractSet)

対象HEAD `5e631ef`、macOS arm64、CPython 3.14.7、debug/release両方で再現した。#548の組込み型shadowing修正後にも残り、今回のソースにはshadowingも再代入もない。

## 最小再現

`subject.py`:

```python
import collections.abc as abc
def f(x: set[int]): pass
observed = f.__annotations__['x']
```

```sh
hoimin plan --root /path/to/project --file subject.py \
  --allow-best-effort-memory --operators type_set_abstract_set -- true
```

期待: `set[int] → abc.Set[int]`。実際: `set[int] → abc.AbstractSet[int]`。元ソースは注釈評価まで成功。生成候補は `AttributeError: module 'collections.abc' has no attribute 'AbstractSet'` となる。

`check.py`は `import subject` の1行だけでよい。次のrunではbaseline成功、killed=1、score=1.0、complete=true、exit=0となる。正しい置換先のabc.Set[int]は評価可能なので、これは存在しない名前に起因する失敗である。

```sh
hoimin run --root /path/to/project --file subject.py \
  --allow-best-effort-memory --operators type_set_abstract_set \
  --format json -- /absolute/path/to/python3.14 check.py
```

## 同じ原因による候補欠落

```python
from collections.abc import Set
def f(x: set[int]): pass
```

この入力では `set[int] → Set[int]` が期待されるが0候補となる。逆方向の `def f(x: Set[int])` も `Set[int] → set[int]` を生成しない。`import typing as t` の `set[int] → t.AbstractSet[int]` は生成・評価とも正常である。

## 原因

- [is_known_type_name](https://github.com/tokyogas-tech/hoimin/blob/5e631ef/crates/hoimin-cli/src/analyzer/rust.rs#L3874) が `AbstractSet` を持つ一方、collections.abcの `Set` を認識しない。
- [collection_replacements](https://github.com/tokyogas-tech/hoimin/blob/5e631ef/crates/hoimin-cli/src/analyzer/rust.rs#L6420) のsource/destination表に `collections.abc.AbstractSet` が入っている。
- `is_supported_annotation` にも同じ誤った名前があるため、nullable等の認識も一緒に点検する必要がある。
- `spelling_for` の修飾名生成は先頭targetの末尾名を使う箇所があり、providerごとに名前が違う対応表へ直す際にはその仮定も点検する。

## 検証と受け入れ条件

Leanのprovider別メンバー表ではtyping→AbstractSet、abc→Setを選ぶ。各providerで選んだメンバーが存在することを証明し、両providerへAbstractSetを使う壊した規則を検出した。標準APIの表自体はモデルの前提であり、Python実装全体の証明ではない。

4つのLean生成fixtureを公開planで照合し、debug/releaseそれぞれ1 match / 3 mismatch。元ソースと生成候補のCPython評価、公開runまで実施した。監査資料は作業ツリー `docs/audits/2026-09-15-annotation-followup/` に保存した（起票時点では未コミット）。

- provider別の正しい名前を使い、collections.abc.Setの両方向を扱う。
- module import、alias付きmodule、直接import、直接importのaliasを検証する。
- typing.AbstractSetの既存挙動とshadowing対策を保持する。
- generated candidateをcompileだけでなく注釈評価まで検証し、AttributeErrorをkilledに数える回帰を防ぐ。
