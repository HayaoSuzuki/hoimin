## 問題

Python 3.14の遅延評価される型注釈について、定義時点のtyping importを評価時点でも有効として扱っている。関数定義後にSequenceをsetへ再代入すると、実際は `set[int]` の注釈なのに `type_list_sequence` が `Sequence[int] → list[int]` を生成する。置換先のSequenceやクラスの後続ローカル束縛でも再現する。

対象HEAD `5e631ef`、macOS arm64、CPython 3.14.7。debug/releaseで再現。#556のwith例外抑制とは独立しており、この入力にはwith/try/finallyがない。

## 最小再現

`subject.py`:

```python
from typing import Sequence
def f(x: Sequence[int]): pass
Sequence = set
observed = f.__annotations__['x']
```

```sh
hoimin plan --root /path/to/project --file subject.py \
  --allow-best-effort-memory --operators type_list_sequence -- true
```

期待: 候補0。実際: `Sequence[int] → list[int]` の1候補。元ソースのobservedは `set[int]`、生成候補では `list[int]` となる。

`check.py`を次の内容にしてrunすると、baseline成功、killed=1、survived=0、score=1.0、complete=true、exit=0となった。

```python
import subject
assert subject.observed == set[int]
```

```sh
hoimin run --root /path/to/project --file subject.py \
  --allow-best-effort-memory --operators type_list_sequence \
  --format json -- /absolute/path/to/python3.14 check.py
```

同じ再代入で、注釈を `list[int]` にすると、置換先として `Sequence[int]` を使い、実際は `set[int]` になる。クラス内で `def f(x: Sequence[int]): ...` の後に `Sequence = set` を置いても、C.fの注釈はsetであり候補が残る。

## 原因と契約

Python 3.14では注釈はアクセス時に遅延評価される。[公式annotationlib文書](https://docs.python.org/3.14/library/annotationlib.html#annotation-semantics)

`AnnotationCollector` は注釈の構文位置の `KnownImports` を使い、`collection_replacements` の `resolved_name` と `spelling_for` はこの状態に依存する。[実装](https://github.com/tokyogas-tech/hoimin/blob/5e631ef/crates/hoimin-cli/src/analyzer/rust.rs#L6152)

#264の設計はdefinition-time lookupと後続の再代入がsnapshotへ遡及しないことを前提としている。その前提が3.14の遅延評価と合わない。#548では組込みのlist/set/dictに対して後続の束縛を考慮するannotation resolverが入ったが、typing aliasの由来・置換先の綴りには適用されていない。

後続再代入を一律に拒否するだけでは精度を失う。再代入前に `f.__annotations__` を読む正例では、値が保持されるため元注釈はtyping.Sequenceのままで、現行候補は正しい。再代入後、初回参照前にtyping importを復元する正例もある。

## 形式検証と実行結果

Leanでcurrent bindingと初回評価キャッシュを分けた遷移を定義した。任意長の後続操作について、初回評価後の値保持、再代入→初回評価ならcustom値保持、初回評価→再代入ならtyping値保持を証明した。宣言時snapshotを固定する壊したモデルとキャッシュを無視するモデルを、それぞれ固定witnessで検出した。

6つのLean生成fixtureを公開planで照合し、各binaryで3 match / 3 mismatch。元ソースと生成候補をCPythonで評価した。比較の期待値はoriginal/replacementの組で、ソースspanも確認している。全Python実装や任意の評価タイミングをLeanが証明したという意味ではない。

監査資料は作業ツリー `docs/audits/2026-09-15-annotation-followup/` に保存した（起票時点では未コミット）。

## 受け入れ条件

- Python 3.14の評価時期を考慮し、sourceとdestination双方のtyping/collections.abc aliasの由来を検証する。
- 最小例、置換先の再代入、クラス後続束縛の不適切な候補を抑制する。
- import不変の正例を保持する。評価済みキャッシュと参照前のimport復元について、対応可能な精度と保守的な除外を明文化し、単なる全候補拒否を避ける。
- Python 3.13以前、future annotations、type alias、generic bound/defaultとの評価時期の違いを整理し、既存#264のsource-order契約を更新する。
- 公開runで不適切な型pairをkilledへ計上しない回帰テストを追加する。
